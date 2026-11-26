//! S3 sync protocol layer.
//!
//! Implements manifest-based synchronization on top of the S3 transport
//! primitives in [`super::s3`]. Artifact set: `db.sql` + `skills.zip`.

use chrono::Utc;
use serde_json::Value;

use crate::error::AppError;
use crate::services::s3::{self, S3Credentials};
use crate::settings::{update_s3_sync_status, S3SyncSettings, WebDavSyncStatus};

pub(crate) use super::sync_protocol::run_with_sync_lock;
use super::sync_protocol::{
    apply_snapshot, build_local_snapshot, localized, persist_sync_success_best_effort, sha256_hex,
    validate_manifest_compat, verify_artifact, RemoteLayout, SyncManifest, DB_COMPAT_VERSION,
    MAX_MANIFEST_BYTES, MAX_SYNC_ARTIFACT_BYTES, REMOTE_DB_SQL, REMOTE_MANIFEST, REMOTE_SKILLS_ZIP,
};

#[cfg(test)]
pub(crate) fn sync_mutex() -> &'static tokio::sync::Mutex<()> {
    super::sync_protocol::sync_mutex()
}

// ─── Public API ──────────────────────────────────────────────

/// Check S3 connectivity by issuing a HEAD request against the bucket.
pub async fn check_connection(settings: &S3SyncSettings) -> Result<(), AppError> {
    settings.validate()?;
    let creds = creds_for(settings);
    s3::test_connection(&creds).await
}

/// Upload local snapshot (db + skills) to remote S3.
pub async fn upload(
    db: &crate::database::Database,
    settings: &mut S3SyncSettings,
) -> Result<Value, AppError> {
    settings.validate()?;
    let creds = creds_for(settings);

    let snapshot = build_local_snapshot(db)?;

    let manifest_hash = snapshot
        .publish("application/json", |name, bytes, mime| {
            let key = s3_key(settings, RemoteLayout::Current, &name);
            let creds = &creds;
            async move { s3::put_object(creds, &key, bytes, mime).await }
        })
        .await?;
    let manifest_key = s3_key(settings, RemoteLayout::Current, REMOTE_MANIFEST);

    // Fetch etag (best-effort, don't fail the upload)
    let etag = match s3::head_object(&creds, &manifest_key).await {
        Ok(e) => e,
        Err(e) => {
            log::debug!("[S3] Failed to fetch ETag after upload: {e}");
            None
        }
    };

    let _persisted =
        persist_sync_success_best_effort(settings, manifest_hash, etag, persist_sync_success);
    Ok(serde_json::json!({ "status": "uploaded" }))
}

/// Download remote snapshot and apply to local database + skills.
pub async fn download(
    db: &crate::database::Database,
    settings: &mut S3SyncSettings,
) -> Result<Value, AppError> {
    settings.validate()?;
    let creds = creds_for(settings);

    let (layout, manifest_bytes, etag) =
        find_manifest(settings, &creds).await?.ok_or_else(|| {
            localized(
                "s3.sync.remote_empty",
                "远端没有可下载的同步数据",
                "No downloadable sync data found on the remote.",
            )
        })?;

    let manifest: SyncManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|e| AppError::Json {
            path: REMOTE_MANIFEST.to_string(),
            source: e,
        })?;

    validate_manifest_compat(&manifest, layout)?;

    // Download and verify artifacts
    let db_sql = download_and_verify(settings, &creds, layout, REMOTE_DB_SQL, &manifest).await?;
    let skills_zip =
        download_and_verify(settings, &creds, layout, REMOTE_SKILLS_ZIP, &manifest).await?;

    // Apply snapshot
    apply_snapshot(db, &db_sql, &skills_zip)?;

    let manifest_hash = sha256_hex(&manifest_bytes);
    let _persisted =
        persist_sync_success_best_effort(settings, manifest_hash, etag, persist_sync_success);
    Ok(serde_json::json!({ "status": "downloaded" }))
}

/// Fetch remote manifest info without downloading artifacts.
pub async fn fetch_remote_info(settings: &S3SyncSettings) -> Result<Option<Value>, AppError> {
    settings.validate()?;
    let creds = creds_for(settings);
    let Some((layout, bytes, _)) = find_manifest(settings, &creds).await? else {
        return Ok(None);
    };

    let manifest: SyncManifest = serde_json::from_slice(&bytes).map_err(|e| AppError::Json {
        path: REMOTE_MANIFEST.to_string(),
        source: e,
    })?;

    let compatible = validate_manifest_compat(&manifest, layout).is_ok();

    let payload = serde_json::json!({
        "deviceName": manifest.device_name,
        "createdAt": manifest.created_at,
        "snapshotId": manifest.snapshot_id,
        "version": manifest.version,
        "protocolVersion": manifest.version,
        "dbCompatVersion": manifest.db_compat_version,
        "compatible": compatible,
        "artifacts": manifest.artifacts.keys().collect::<Vec<_>>(),
        "layout": layout.as_str(),
        "remotePath": s3_dir_display(settings, layout),
    });

    Ok(Some(payload))
}

async fn find_manifest(
    settings: &S3SyncSettings,
    creds: &S3Credentials,
) -> Result<Option<(RemoteLayout, Vec<u8>, Option<String>)>, AppError> {
    for layout in [RemoteLayout::Current, RemoteLayout::V2Database] {
        let key = s3_key(settings, layout, REMOTE_MANIFEST);
        if let Some((bytes, etag)) = s3::get_object(creds, &key, MAX_MANIFEST_BYTES).await? {
            return Ok(Some((layout, bytes, etag)));
        }
    }
    Ok(None)
}

// ─── Sync status persistence ─────────────────────────────────

fn persist_sync_success(
    settings: &mut S3SyncSettings,
    manifest_hash: String,
    etag: Option<String>,
) -> Result<(), AppError> {
    let status = WebDavSyncStatus {
        last_sync_at: Some(Utc::now().timestamp()),
        last_error: None,
        last_error_source: None,
        last_local_manifest_hash: Some(manifest_hash.clone()),
        last_remote_manifest_hash: Some(manifest_hash),
        last_remote_etag: etag,
    };
    settings.status = status.clone();
    update_s3_sync_status(status)
}

// ─── Download & verify ───────────────────────────────────────

async fn download_and_verify(
    settings: &S3SyncSettings,
    creds: &S3Credentials,
    layout: RemoteLayout,
    artifact_name: &str,
    manifest: &SyncManifest,
) -> Result<Vec<u8>, AppError> {
    let (object, meta) = manifest.artifact(artifact_name)?;
    let key = s3_key(settings, layout, &object);
    let (bytes, _) = s3::get_object(creds, &key, MAX_SYNC_ARTIFACT_BYTES as usize)
        .await?
        .ok_or_else(|| {
            localized(
                "s3.sync.remote_missing_artifact",
                format!("远端缺少 artifact 文件: {artifact_name}"),
                format!("Remote artifact file missing: {artifact_name}"),
            )
        })?;

    verify_artifact(&bytes, artifact_name, meta)?;
    Ok(bytes)
}

// ─── S3 key helpers ──────────────────────────────────────────

fn s3_key(settings: &S3SyncSettings, layout: RemoteLayout, artifact: &str) -> String {
    format!("{}/{artifact}", s3_dir_display(settings, layout))
}

fn s3_dir_display(settings: &S3SyncSettings, layout: RemoteLayout) -> String {
    format!(
        "{}/v{}/db-v{}/{}",
        settings.remote_root,
        layout.version(),
        DB_COMPAT_VERSION,
        settings.profile
    )
}

fn creds_for(settings: &S3SyncSettings) -> S3Credentials {
    S3Credentials {
        access_key_id: settings.access_key_id.clone(),
        secret_access_key: settings.secret_access_key.clone(),
        region: settings.region.clone(),
        bucket: settings.bucket.clone(),
        endpoint: settings.endpoint.clone(),
    }
}

// ─── Tests ───────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_settings() -> S3SyncSettings {
        S3SyncSettings {
            remote_root: "cc-switch-sync".to_string(),
            profile: "default".to_string(),
            ..S3SyncSettings::default()
        }
    }

    #[test]
    fn s3_key_uses_v3_and_correct_format() {
        let settings = test_settings();
        let key = s3_key(&settings, RemoteLayout::Current, "manifest.json");
        assert_eq!(key, "cc-switch-sync/v3/db-v6/default/manifest.json");
    }

    #[test]
    fn s3_key_with_custom_profile() {
        let settings = S3SyncSettings {
            remote_root: "my-root".to_string(),
            profile: "work".to_string(),
            ..S3SyncSettings::default()
        };
        assert_eq!(
            s3_key(&settings, RemoteLayout::Current, "db.sql"),
            "my-root/v3/db-v6/work/db.sql"
        );
    }

    #[test]
    fn s3_key_matches_expected_pattern() {
        let settings = test_settings();
        let key = s3_key(&settings, RemoteLayout::Current, "skills.zip");
        // Should follow {remote_root}/v{version}/db-v{db}/{profile}/{artifact}
        let parts: Vec<&str> = key.splitn(5, '/').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0], "cc-switch-sync");
        assert_eq!(parts[1], "v3");
        assert_eq!(parts[2], "db-v6");
        assert_eq!(parts[3], "default");
        assert_eq!(parts[4], "skills.zip");
    }

    #[test]
    fn sync_mutex_is_singleton() {
        let m1 = sync_mutex();
        let m2 = sync_mutex();
        assert!(
            std::ptr::eq(m1, m2),
            "sync_mutex must return the same instance"
        );
    }

    #[test]
    fn creds_for_maps_all_fields() {
        let settings = S3SyncSettings {
            access_key_id: "AKID".to_string(),
            secret_access_key: "SECRET".to_string(),
            region: "us-west-2".to_string(),
            bucket: "my-bucket".to_string(),
            endpoint: "minio.local:9000".to_string(),
            ..S3SyncSettings::default()
        };
        let creds = creds_for(&settings);
        assert_eq!(creds.access_key_id, "AKID");
        assert_eq!(creds.secret_access_key, "SECRET");
        assert_eq!(creds.region, "us-west-2");
        assert_eq!(creds.bucket, "my-bucket");
        assert_eq!(creds.endpoint, "minio.local:9000");
    }
}
