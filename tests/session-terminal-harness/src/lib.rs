//! Run the actual terminal module's regressions without rebuilding the desktop UI.
#![allow(dead_code)]

#[cfg(windows)]
mod commands {
    pub(crate) fn run_windows_start_command(_: &[&str], _: &str) -> Result<(), String> {
        panic!("Terminal regression tests must not launch a desktop window")
    }
}

#[path = "../../../src-tauri/src/session_manager/terminal/mod.rs"]
mod terminal;
