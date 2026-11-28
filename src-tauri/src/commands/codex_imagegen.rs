use serde::Serialize;

use crate::store::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexImagegenGuide {
    /// This only describes the file on disk, not a running Desktop process.
    status: &'static str,
}

#[tauri::command]
pub fn codex_imagegen_guide(state: tauri::State<'_, AppState>) -> CodexImagegenGuide {
    CodexImagegenGuide {
        status: crate::services::codex_imagegen::initialize(&state.db),
    }
}
