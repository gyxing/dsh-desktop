use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Manager, Wry,
};

use super::{lifecycle::AppLifecycle, terminal::open_dsh_terminal};
use crate::{
    platform,
    runtime::{manager::RuntimeManager, status::RuntimeStatus},
    updater::{manager::UpdateManager, status::UpdateStatus},
};

const OPEN_ID: &str = "tray-open";
const STATUS_ID: &str = "tray-status";
const UPDATE_STATUS_ID: &str = "tray-update-status";
const TERMINAL_ID: &str = "tray-terminal";
const RESTART_ID: &str = "tray-restart";
const CHECK_UPDATE_ID: &str = "tray-check-update";
const QUIT_ID: &str = "tray-quit";

pub struct DesktopTray {
    _tray: TrayIcon<Wry>,
    status_item: MenuItem<Wry>,
    restart_item: MenuItem<Wry>,
    update_status_item: MenuItem<Wry>,
    check_update_item: MenuItem<Wry>,
}

pub struct TrayRuntimePresentation {
    pub status_label: &'static str,
    pub restart_enabled: bool,
}

/// 描述更新状态在托盘中的可见文字与交互可用性。
pub struct UpdateTrayPresentation {
    pub status_label: String,
    pub check_enabled: bool,
}

pub fn runtime_presentation(status: &RuntimeStatus) -> TrayRuntimePresentation {
    match status {
        RuntimeStatus::Starting { .. } => TrayRuntimePresentation {
            status_label: "状态：正在启动",
            restart_enabled: false,
        },
        RuntimeStatus::Probing { .. } => TrayRuntimePresentation {
            status_label: "状态：正在检查服务",
            restart_enabled: false,
        },
        RuntimeStatus::Loading { .. } => TrayRuntimePresentation {
            status_label: "状态：正在加载页面",
            restart_enabled: false,
        },
        RuntimeStatus::Ready { .. } => TrayRuntimePresentation {
            status_label: "状态：运行中",
            restart_enabled: true,
        },
        RuntimeStatus::Failed { .. } => TrayRuntimePresentation {
            status_label: "状态：启动失败",
            restart_enabled: true,
        },
        RuntimeStatus::Exited { .. } => TrayRuntimePresentation {
            status_label: "状态：已退出",
            restart_enabled: true,
        },
    }
}

/// 把更新内部状态映射为托盘文字和手动检查可用性。
pub fn update_presentation(status: &UpdateStatus) -> UpdateTrayPresentation {
    let (status_label, check_enabled) = match status {
        UpdateStatus::Idle => ("更新：尚未检查".to_string(), true),
        UpdateStatus::Checking { .. } => ("更新：正在检查".to_string(), false),
        UpdateStatus::UpToDate => ("更新：已是最新".to_string(), true),
        UpdateStatus::Available { version, .. } => (format!("更新：发现 {version}"), true),
        UpdateStatus::Downloading { version, .. } => (format!("更新：正在下载 {version}"), false),
        UpdateStatus::Installing { version } => (format!("更新：正在安装 {version}"), false),
        UpdateStatus::Failed { .. } => ("更新：检查失败".to_string(), true),
    };
    UpdateTrayPresentation {
        status_label,
        check_enabled,
    }
}

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let runtime_presentation = runtime_presentation(&app.state::<Arc<RuntimeManager>>().status());
    let update_presentation = update_presentation(&app.state::<Arc<UpdateManager>>().status());
    let open_item = MenuItem::with_id(app, OPEN_ID, "打开 DSH Desktop", true, None::<&str>)?;
    let status_item = MenuItem::with_id(
        app,
        STATUS_ID,
        runtime_presentation.status_label,
        false,
        None::<&str>,
    )?;
    let update_status_item = MenuItem::with_id(
        app,
        UPDATE_STATUS_ID,
        update_presentation.status_label,
        false,
        None::<&str>,
    )?;
    let terminal_item = MenuItem::with_id(app, TERMINAL_ID, "打开 DSH 终端", true, None::<&str>)?;
    let restart_item = MenuItem::with_id(
        app,
        RESTART_ID,
        "重新启动 DeepSeek Harness",
        runtime_presentation.restart_enabled,
        None::<&str>,
    )?;
    let check_update_item = MenuItem::with_id(
        app,
        CHECK_UPDATE_ID,
        "检查更新",
        update_presentation.check_enabled,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, QUIT_ID, "退出", true, None::<&str>)?;
    let separator_one = PredefinedMenuItem::separator(app)?;
    let separator_two = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &open_item,
            &status_item,
            &update_status_item,
            &separator_one,
            &terminal_item,
            &restart_item,
            &check_update_item,
            &separator_two,
            &quit_item,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("DSH Desktop")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        });
    #[cfg(target_os = "macos")]
    {
        let icon = tauri::image::Image::from_bytes(include_bytes!(
            "../../../assets/icons/tray-template.png"
        ))?;
        builder = builder.icon(icon).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;
    app.manage(DesktopTray {
        _tray: tray,
        status_item,
        restart_item,
        update_status_item,
        check_update_item,
    });
    Ok(())
}

pub fn update_runtime_status(app: &AppHandle, status: &RuntimeStatus) {
    let Some(tray) = app.try_state::<DesktopTray>() else {
        return;
    };
    let presentation = runtime_presentation(status);
    let _ = tray.status_item.set_text(presentation.status_label);
    let _ = tray.restart_item.set_enabled(presentation.restart_enabled);
}

/// 使用当前更新状态刷新托盘文字和“检查更新”菜单可用性。
pub fn update_updater_status(app: &AppHandle) {
    let Some(tray) = app.try_state::<DesktopTray>() else {
        return;
    };
    let Some(manager) = app.try_state::<Arc<UpdateManager>>() else {
        return;
    };
    let presentation = update_presentation(&manager.status());
    let _ = tray.update_status_item.set_text(presentation.status_label);
    let _ = tray
        .check_update_item
        .set_enabled(presentation.check_enabled);
}

pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    if window.is_minimized().unwrap_or(false) {
        let _ = window.unminimize();
    }
    let _ = window.show();
    let _ = window.set_focus();
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        OPEN_ID => show_main_window(app),
        TERMINAL_ID => {
            if let Err(error) = open_dsh_terminal(app) {
                app.state::<Arc<RuntimeManager>>()
                    .record_system_diagnostic(&format!("打开 DSH 终端失败：{error}"));
                platform::show_native_error(app, "无法打开 DSH 终端", &error.to_string());
            }
        }
        RESTART_ID => {
            let runtime = app.state::<Arc<RuntimeManager>>().inner().clone();
            if let Err(error) = runtime.start(app) {
                platform::show_native_error(app, "无法重新启动 DeepSeek Harness", &error);
            }
        }
        CHECK_UPDATE_ID => crate::updater::service::spawn_manual_check(app),
        QUIT_ID => {
            let lifecycle = app.state::<AppLifecycle>();
            if lifecycle.request_quit() {
                app.state::<Arc<RuntimeManager>>().stop();
                app.exit(0);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::runtime_presentation;
    use crate::runtime::status::{RuntimeErrorCode, RuntimeStatus};

    #[test]
    fn tray_disables_restart_while_runtime_is_starting() {
        for status in [
            RuntimeStatus::starting(),
            RuntimeStatus::probing(),
            RuntimeStatus::loading(),
        ] {
            let presentation = runtime_presentation(&status);
            assert!(!presentation.restart_enabled);
        }
    }

    #[test]
    fn tray_enables_restart_after_ready_failure_or_exit() {
        let statuses = [
            RuntimeStatus::Ready {
                message: "已就绪".to_string(),
                url: "http://127.0.0.1:45140/".to_string(),
            },
            RuntimeStatus::failed(RuntimeErrorCode::StartupTimeout, "启动超时"),
            RuntimeStatus::Exited {
                code: RuntimeErrorCode::ProcessExited,
                message: "进程已退出".to_string(),
            },
        ];

        for status in statuses {
            let presentation = runtime_presentation(&status);
            assert!(presentation.restart_enabled);
            assert!(!presentation.status_label.is_empty());
        }
    }
}
