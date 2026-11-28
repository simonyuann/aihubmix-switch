//! 测试共享的环境变量守卫。
//!
//! 各 *_config.rs / 服务测试曾各自实现"保存旧值 → 设置新值 → Drop 恢复"的
//! 夹具样板(codex_history_migration / dsh_config / services::skill /
//! aihubmix 等 ×8);本模块是它的单一权威实现。(治理 T14,见
//! docs/governance/TASKS.md;触碰 env 的测试需配合 `#[serial_test::serial]`。)

/// 把环境变量设为 `value` 并记住旧值;Drop 时恢复旧值(未设置过则删除)。
pub(crate) struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    pub(crate) fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(previous) => std::env::set_var(self.key, previous),
            None => std::env::remove_var(self.key),
        }
    }
}
