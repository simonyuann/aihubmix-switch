//! Codex service-tier evidence and API reference prices. Tier prices stay in
//! `pricing_model`, so normal cost backfills and rollups use the same basis.

use rust_decimal::Decimal;
use serde_json::Value;

const TIER_MARKER: &str = ":service-tier=";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum CodexServiceTier {
    #[default]
    Unspecified,
    Standard,
    Priority,
    Other(String),
}

impl CodexServiceTier {
    fn from_value(value: &Value) -> Self {
        if value.is_null() {
            return Self::Standard;
        }
        let tier = value
            .as_str()
            .unwrap_or("unknown")
            .trim()
            .to_ascii_lowercase();
        match tier.as_str() {
            "default" | "standard" => Self::Standard,
            "priority" | "fast" => Self::Priority,
            _ => Self::Other(
                if !tier.is_empty()
                    && tier.len() <= 64
                    && tier
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                {
                    tier
                } else {
                    "unknown".to_string()
                },
            ),
        }
    }

    /// Missing fields preserve the setting; explicit null turns Fast off.
    pub(crate) fn observe(&mut self, settings: &Value) {
        if let Some(value) = settings.get("service_tier") {
            *self = Self::from_value(value);
        }
    }

    /// A request's own evidence overrides the setting for this request only.
    pub(crate) fn for_request(&self, sources: &[&Value]) -> Self {
        sources
            .iter()
            .find_map(|source| source.get("service_tier"))
            .map(Self::from_value)
            .unwrap_or_else(|| self.clone())
    }

    pub(crate) fn pricing_model(&self, model: &str) -> String {
        match self {
            Self::Unspecified | Self::Standard => model.to_string(),
            Self::Priority => format!("{model}{TIER_MARKER}priority"),
            Self::Other(tier) => format!("{model}{TIER_MARKER}{tier}"),
        }
    }
}

pub(crate) fn split_tier_pricing_model(model: &str) -> Option<(&str, &str)> {
    let (base, tier) = model.split_once(TIER_MARKER)?;
    (!base.is_empty() && !tier.is_empty()).then_some((base, tier))
}

/// Rates verified against the deployed pricing table on 2026-10-27. Do not
/// assume every model or tier costs 2x: an exact custom tier price wins in the
/// shared pricing lookup; unverified variants keep the standard estimate.
pub(crate) fn tier_price_multiplier(model: &str, tier: &str) -> Decimal {
    if tier == "priority"
        && matches!(
            model,
            "gpt-6.1-sol"
                | "gpt-6-sol"
                | "gpt-6-astra"
                | "gpt-6-luna"
                | "gpt-5.6-sol"
                | "gpt-5.6-terra"
                | "gpt-5.6-luna"
                | "gpt-5.5"
                | "gpt-5.4"
                | "gpt-5.4-mini"
                | "gpt-5.3-codex"
                | "gpt-5.2"
                | "gpt-5.2-codex"
                | "gpt-5.1-codex"
        )
    {
        Decimal::from(2)
    } else {
        Decimal::ONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use serde_json::json;

    #[test]
    fn settings_and_per_request_overrides_do_not_leak() {
        let mut tier = CodexServiceTier::default();
        tier.observe(&json!({"service_tier": "priority"}));
        tier.observe(&json!({"model": "gpt-6.1-sol"}));
        assert_eq!(tier, CodexServiceTier::Priority);
        assert_eq!(
            tier.for_request(&[&json!({"service_tier": "default"})]),
            CodexServiceTier::Standard
        );
        assert_eq!(tier.for_request(&[&json!({})]), CodexServiceTier::Priority);
        tier.observe(&json!({"service_tier": null}));
        assert_eq!(tier, CodexServiceTier::Standard);
        tier.observe(&json!({"service_tier": " FAST "}));
        assert_eq!(tier, CodexServiceTier::Priority);
        tier.observe(&json!({"service_tier": 7}));
        assert_eq!(tier, CodexServiceTier::Other("unknown".into()));
    }

    #[test]
    fn tier_prices_cover_all_components_and_respect_custom_prices(
    ) -> Result<(), crate::error::AppError> {
        use crate::database::{lock_conn, Database};
        use crate::services::usage_stats::find_model_pricing;
        let db = Database::memory()?;
        let conn = lock_conn!(db.conn);
        for model in [
            "gpt-6.1-sol",
            "gpt-6-sol",
            "gpt-6-astra",
            "gpt-6-luna",
            "gpt-5.6-sol",
            "gpt-5.6-terra",
            "gpt-5.6-luna",
            "gpt-5.5",
            "gpt-5.4",
            "gpt-5.4-mini",
            "gpt-5.3-codex",
            "gpt-5.2",
            "gpt-5.2-codex",
            "gpt-5.1-codex",
        ] {
            let base = find_model_pricing(&conn, model).unwrap();
            let fast = find_model_pricing(&conn, &CodexServiceTier::Priority.pricing_model(model))
                .unwrap();
            assert_eq!(
                fast.input_cost_per_million,
                base.input_cost_per_million * Decimal::from(2)
            );
            assert_eq!(
                fast.output_cost_per_million,
                base.output_cost_per_million * Decimal::from(2)
            );
            assert_eq!(
                fast.cache_read_cost_per_million,
                base.cache_read_cost_per_million * Decimal::from(2)
            );
            assert_eq!(
                fast.cache_creation_cost_per_million,
                base.cache_creation_cost_per_million * Decimal::from(2)
            );
        }
        for (model, tier) in [
            ("gpt-5.4-pro", "priority"),
            ("gpt-6.1-sol", "flex"),
            ("gpt-6.1-sol", "auto"),
        ] {
            let base = find_model_pricing(&conn, model).unwrap();
            let estimate =
                find_model_pricing(&conn, &format!("{model}{TIER_MARKER}{tier}")).unwrap();
            assert_eq!(base.input_cost_per_million, estimate.input_cost_per_million);
        }
        conn.execute(
            "INSERT INTO model_pricing (model_id, display_name, input_cost_per_million,
                 output_cost_per_million, cache_read_cost_per_million, cache_creation_cost_per_million)
             VALUES ('gpt-6.1-sol:service-tier=priority', 'Custom Priority', '7', '33', '0.7', '9')", [],
        )?;
        let fast = find_model_pricing(&conn, "gpt-6.1-sol:service-tier=priority").unwrap();
        assert_eq!(fast.input_cost_per_million, Decimal::from(7));
        assert_eq!(fast.output_cost_per_million, Decimal::from(33));
        // Standard must not pick up a service-tier variant through prefix lookup.
        conn.execute(
            "DELETE FROM model_pricing WHERE model_id = 'gpt-6.1-sol'",
            [],
        )?;
        assert!(find_model_pricing(&conn, "gpt-6.1-sol").is_none());
        Ok(())
    }
}
