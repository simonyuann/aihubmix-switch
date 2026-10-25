mod agents;
mod service;
mod store;
mod types;

use serde_json::Value;
use service::Backend;
use tauri::Manager;
use tokio::sync::Mutex;

#[tauri::command]
async fn dispatch(
    command: String,
    payload: Option<Value>,
    state: tauri::State<'_, Mutex<Backend>>,
) -> Result<Value, String> {
    state
        .lock()
        .await
        .dispatch(&command, payload.unwrap_or(Value::Null))
        .await
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let dir = dirs::data_dir()
                .ok_or("无法定位应用数据目录")?
                .join("aihubmix-switch");
            let backend = Backend::new(store::Store::load(dir)?)?;
            app.manage(Mutex::new(backend));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![dispatch])
        .run(tauri::generate_context!())
        .expect("无法启动 AIHubMix Switch");
}

#[cfg(test)]
mod tests;
