mod agents;
mod bridge;
mod protocol;
mod service;
mod store;
mod types;

use serde_json::Value;
use service::Backend;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};
use tokio::sync::Mutex;
struct Background(Arc<AtomicBool>);

#[tauri::command]
async fn dispatch(
    command: String,
    payload: Option<Value>,
    state: tauri::State<'_, Arc<Mutex<Backend>>>,
) -> Result<Value, String> {
    state
        .lock()
        .await
        .dispatch(&command, payload.unwrap_or(Value::Null))
        .await
}
fn show_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_window(app)
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let dir = dirs::data_dir()
                .ok_or("无法定位应用数据目录")?
                .join("aihubmix-switch");
            let mut backend = Backend::new(store::Store::load(dir)?)?;
            let listener = backend.bind_bridge();
            app.manage(Background(backend.compatibility_active.clone()));
            let backend = Arc::new(Mutex::new(backend));
            app.manage(backend.clone());
            if let Some(listener) = listener {
                let state = backend.clone();
                tauri::async_runtime::spawn(async move {
                    if bridge::serve(listener, state.clone()).await.is_err() {
                        let mut backend = state.lock().await;
                        backend.bridge_error = Some("本地兼容服务已停止，请重启应用".into());
                        backend.compatibility_active.store(false, Ordering::Relaxed);
                    }
                });
            }
            let show = MenuItem::with_id(app, "show", "打开 AIHubMix Switch", true, None::<&str>)?;
            let quit =
                MenuItem::with_id(app, "quit", "退出（停止本地兼容连接）", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("AIHubMix Switch")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window
                    .app_handle()
                    .state::<Background>()
                    .0
                    .load(Ordering::Relaxed)
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![dispatch])
        .run(tauri::generate_context!())
        .expect("无法启动 AIHubMix Switch");
}
#[cfg(test)]
mod tests;
