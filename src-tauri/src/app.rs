use std::sync::Arc;

use tauri::{webview::PageLoadEvent, AppHandle, Manager, RunEvent, State, WindowEvent};

use crate::desktop::{
    lifecycle::{AppLifecycle, CloseAction},
    navigation, tray,
};
use crate::runtime::{manager::RuntimeManager, status::RuntimeStatus};

#[tauri::command]
fn runtime_status(runtime: State<'_, Arc<RuntimeManager>>) -> RuntimeStatus {
    runtime.status()
}

#[tauri::command]
fn runtime_diagnostics(runtime: State<'_, Arc<RuntimeManager>>) -> String {
    runtime.diagnostics()
}

#[tauri::command]
fn restart_runtime(app: AppHandle, runtime: State<'_, Arc<RuntimeManager>>) -> Result<(), String> {
    runtime.inner().clone().start(&app)
}

/// 创建并运行 DSH Desktop 的 Tauri 应用。
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(navigation::init())
        .manage(Arc::new(RuntimeManager::new()))
        .manage(AppLifecycle::new())
        .invoke_handler(tauri::generate_handler![
            runtime_status,
            runtime_diagnostics,
            restart_runtime
        ])
        .on_page_load(|webview, payload| {
            if webview.label() != "main" || payload.event() != PageLoadEvent::Finished {
                return;
            }
            let app = webview.app_handle();
            app.state::<Arc<RuntimeManager>>()
                .mark_page_ready(app, payload.url());
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.state::<AppLifecycle>().close_action() == CloseAction::Hide {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(|app| {
            tray::setup(app)?;
            let handle = app.handle().clone();
            let runtime = app.state::<Arc<RuntimeManager>>().inner().clone();
            let _ = runtime.start(&handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("DSH Desktop 构建失败");

    app.run(|app, event| {
        if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
            app.state::<AppLifecycle>().request_quit();
            app.state::<Arc<RuntimeManager>>().stop();
        }
    });
}
