use std::sync::Arc;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    App, AppHandle, Manager, Wry,
};

use super::{lifecycle::AppLifecycle, terminal::open_dsh_powershell};
use crate::runtime::{manager::RuntimeManager, status::RuntimeStatus};

#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

const OPEN_ID: &str = "tray-open";
const STATUS_ID: &str = "tray-status";
const TERMINAL_ID: &str = "tray-terminal";
const RESTART_ID: &str = "tray-restart";
const QUIT_ID: &str = "tray-quit";

pub struct DesktopTray {
    _tray: TrayIcon<Wry>,
    status_item: MenuItem<Wry>,
    restart_item: MenuItem<Wry>,
}

pub struct TrayRuntimePresentation {
    pub status_label: &'static str,
    pub restart_enabled: bool,
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

pub fn setup(app: &mut App) -> tauri::Result<()> {
    let presentation = runtime_presentation(&app.state::<Arc<RuntimeManager>>().status());
    let open_item = MenuItem::with_id(app, OPEN_ID, "打开 DSH Desktop", true, None::<&str>)?;
    let status_item = MenuItem::with_id(
        app,
        STATUS_ID,
        presentation.status_label,
        false,
        None::<&str>,
    )?;
    let terminal_item =
        MenuItem::with_id(app, TERMINAL_ID, "打开 DSH PowerShell", true, None::<&str>)?;
    let restart_item = MenuItem::with_id(
        app,
        RESTART_ID,
        "重新启动 DeepSeek Harness",
        presentation.restart_enabled,
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
            &separator_one,
            &terminal_item,
            &restart_item,
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
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;
    app.manage(DesktopTray {
        _tray: tray,
        status_item,
        restart_item,
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
            if let Err(error) = open_dsh_powershell(app) {
                app.state::<Arc<RuntimeManager>>()
                    .record_system_diagnostic(&format!("打开 DSH PowerShell 失败：{error}"));
                show_native_error("无法打开 DSH PowerShell", &error.to_string());
            }
        }
        RESTART_ID => {
            let runtime = app.state::<Arc<RuntimeManager>>().inner().clone();
            if let Err(error) = runtime.start(app) {
                show_native_error("无法重新启动 DeepSeek Harness", &error);
            }
        }
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

#[cfg(windows)]
fn show_native_error(title: &str, message: &str) {
    let title = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let message = message
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
fn show_native_error(_title: &str, message: &str) {
    eprintln!("{message}");
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
