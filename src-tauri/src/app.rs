use std::sync::Arc;

use tauri::{
    webview::PageLoadEvent,
    window::{ProgressBarState, ProgressBarStatus},
    AppHandle, Manager, RunEvent, State, WindowEvent,
};

use crate::desktop::{
    lifecycle::{AppLifecycle, CloseAction},
    navigation, tray,
};
use crate::runtime::{manager::RuntimeManager, status::RuntimeStatus};
use crate::updater::{
    manager::UpdateManager,
    presentation::{update_presentation, UpdateTaskbarProgress},
    service::spawn_automatic_check,
};

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

/// 页面导航可能覆盖标题，因此启动、加载完成和更新状态变化时都会重设原生展示。
pub(crate) fn update_main_window_presentation(app: &AppHandle) {
    let base_title = format!("DSH Desktop {}", app.package_info().version);
    let update = app
        .try_state::<Arc<UpdateManager>>()
        .map(|manager| update_presentation(&manager.status()));
    if let Some(window) = app.get_webview_window("main") {
        let title = update
            .as_ref()
            .and_then(|presentation| presentation.title_suffix.as_deref())
            .map(|suffix| format!("{base_title} · {suffix}"))
            .unwrap_or(base_title);
        let _ = window.set_title(&title);
        let progress_state = match update.map(|presentation| presentation.taskbar_progress) {
            Some(UpdateTaskbarProgress::Percentage(progress)) => ProgressBarState {
                status: Some(ProgressBarStatus::Normal),
                progress: Some(progress),
            },
            Some(UpdateTaskbarProgress::Indeterminate) => ProgressBarState {
                status: Some(ProgressBarStatus::Indeterminate),
                progress: None,
            },
            _ => ProgressBarState {
                status: Some(ProgressBarStatus::None),
                progress: None,
            },
        };
        let _ = window.set_progress_bar(progress_state);
    }
}

/// 创建并运行 DSH Desktop 的 Tauri 应用。
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(navigation::init())
        .manage(Arc::new(RuntimeManager::new()))
        .manage(Arc::new(UpdateManager::new()))
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
            update_main_window_presentation(app);
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
            update_main_window_presentation(app.handle());
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            tray::setup(app)?;
            let handle = app.handle().clone();
            let runtime = app.state::<Arc<RuntimeManager>>().inner().clone();
            let _ = runtime.start(&handle);
            spawn_automatic_check(handle);
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
