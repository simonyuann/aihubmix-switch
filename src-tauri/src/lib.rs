mod agents;
mod bridge;
mod codex_history;
mod config_formats;
mod detection;
mod diagnostics;
mod payments;
mod protocol;
mod service;
mod sessions;
mod store;
mod types;
mod updates;
mod verification;

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
async fn list_sessions(
    query: Option<String>,
    agent: Option<String>,
    state: tauri::State<'_, Arc<Mutex<Backend>>>,
) -> Result<sessions::Index, String> {
    let context = state.lock().await.context.clone();
    let query = query
        .unwrap_or_default()
        .chars()
        .take(256)
        .collect::<String>();
    let agent = agent.unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || sessions::list(&context, &query, &agent))
        .await
        .map_err(|_| "读取会话失败，请重试".into())
}
#[tauri::command]
async fn read_session(
    id: String,
    state: tauri::State<'_, Arc<Mutex<Backend>>>,
) -> Result<sessions::Detail, String> {
    let (context, cli_paths) = {
        let backend = state.lock().await;
        (
            backend.context.clone(),
            backend.store.settings.cli_paths.clone(),
        )
    };
    tauri::async_runtime::spawn_blocking(move || sessions::detail(&context, &id, &cli_paths))
        .await
        .map_err(|_| "读取会话失败，请重试".to_string())?
}

#[tauri::command]
async fn dispatch(
    command: String,
    payload: Option<Value>,
    state: tauri::State<'_, Arc<Mutex<Backend>>>,
    payments: tauri::State<'_, payments::Payments>,
) -> Result<Value, String> {
    if command.starts_with("payment-") {
        return payments::dispatch(
            state.inner(),
            payments.inner(),
            &command,
            payload.unwrap_or(Value::Null),
        )
        .await;
    }
    if command == "diagnose" {
        let input = payload.unwrap_or(Value::Null);
        let task = state
            .lock()
            .await
            .prepare_diagnostic(&types::string(&input, "agent"))
            .await?;
        // Model requests must not hold the store mutex or delay the Claude bridge.
        return Ok(diagnostics::run(task).await);
    }
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Updates::default())
        .manage(payments::Payments::default())
        .setup(|app| {
            let dir = dirs::data_dir()
                .ok_or("无法定位应用数据目录")?
                .join("aihubmix-switch");
            let mut backend = Backend::new(store::Store::load(dir)?)?;
            // Codex 官方重新登录或其他工具会重写 config.toml，丢失统一分组的镜像；
            // 启动时按账本补回（未开启统一历史时什么也不做），否则 /resume 会再次分裂。
            let _ = codex_history::reconcile(&backend.context);
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
                    .state::<updates::Updates>()
                    .keep_running()
                    || window
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
        .invoke_handler(tauri::generate_handler![
            dispatch,
            list_sessions,
            read_session,
            updates::update_status,
            updates::check_update,
            updates::download_update,
            updates::install_update
        ])
        .run(tauri::generate_context!())
        .expect("无法启动 AIHubMix Switch");
}
#[cfg(test)]
mod tests;

/// Read-only diagnostic used by release verification; never opens the vault or UI.
pub fn update_format() -> &'static str {
    updates::current_format()
}
