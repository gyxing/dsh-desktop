use std::{fmt::Display, sync::Arc, time::Duration};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::UpdaterExt;
use tokio::sync::oneshot;

use crate::{desktop::lifecycle::AppLifecycle, runtime::manager::RuntimeManager};

use super::{
    manager::UpdateManager,
    status::{UpdateCheckSource, UpdateStatus},
};

const AUTOMATIC_CHECK_DELAY: Duration = Duration::from_secs(30);
const MAX_RELEASE_NOTES_CHARS: usize = 2_000;

/// 延迟检查一次更新，避免阻塞 DSH 启动和页面加载。
pub fn spawn_automatic_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(AUTOMATIC_CHECK_DELAY).await;
        run_check(app, UpdateCheckSource::Automatic).await;
    });
}

/// 立即执行用户从托盘发起的更新检查。
pub fn spawn_manual_check(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run_check(app, UpdateCheckSource::Manual).await;
    });
}

/// 串行完成检查、用户确认、验签下载和安装，所有失败路径都保留可恢复状态。
async fn run_check(app: AppHandle, source: UpdateCheckSource) {
    let manager = app.state::<Arc<UpdateManager>>().inner().clone();
    let Some(_guard) = manager.try_begin(source) else {
        if source == UpdateCheckSource::Manual {
            show_info(&app, "正在检查更新", "已有更新任务正在进行，请稍候。");
        }
        return;
    };
    crate::desktop::tray::update_updater_status(&app);

    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(error) => {
            handle_failure(&app, &manager, source, "初始化更新器失败", &error);
            return;
        }
    };
    let update = match updater.check().await {
        Ok(update) => update,
        Err(error) => {
            handle_failure(&app, &manager, source, "检查更新失败", &error);
            return;
        }
    };

    let Some(update) = update else {
        set_status(&app, &manager, UpdateStatus::UpToDate);
        if source == UpdateCheckSource::Manual {
            show_info(&app, "检查更新", "当前已是最新版本。");
        }
        return;
    };

    let version = update.version.clone();
    let notes = update.body.clone();
    set_status(
        &app,
        &manager,
        UpdateStatus::Available {
            version: version.clone(),
            notes: notes.clone(),
        },
    );
    if !confirm_install(&app, &version, notes.as_deref()).await {
        return;
    }

    set_status(
        &app,
        &manager,
        UpdateStatus::Downloading {
            version: version.clone(),
            downloaded: 0,
            total: None,
        },
    );
    let progress_app = app.clone();
    let progress_manager = manager.clone();
    let progress_version = version.clone();
    let mut downloaded = 0_u64;
    let bytes = match update
        .download(
            move |chunk, total| {
                downloaded += chunk as u64;
                progress_manager.set_status(UpdateStatus::Downloading {
                    version: progress_version.clone(),
                    downloaded,
                    total,
                });
                crate::desktop::tray::update_updater_status(&progress_app);
            },
            || {},
        )
        .await
    {
        Ok(bytes) => bytes,
        Err(error) => {
            handle_failure(&app, &manager, source, "下载或验签更新失败", &error);
            return;
        }
    };

    set_status(
        &app,
        &manager,
        UpdateStatus::Installing {
            version: version.clone(),
        },
    );
    // 只有签名校验完成后才停止 Sidecar，避免下载或验签失败影响现有会话。
    let runtime = app.state::<Arc<RuntimeManager>>().inner().clone();
    runtime.stop();
    match update.install(bytes) {
        Ok(()) => {
            app.state::<AppLifecycle>().request_quit();
            app.request_restart();
        }
        Err(error) => {
            let message = format!("安装更新失败：{error}");
            set_status(
                &app,
                &manager,
                UpdateStatus::Failed {
                    source,
                    message: message.clone(),
                },
            );
            runtime.record_system_diagnostic(&message);
            if let Err(restart_error) = runtime.start(&app) {
                runtime
                    .record_system_diagnostic(&format!("更新失败后恢复 DSH 失败：{restart_error}"));
            }
            show_error(&app, "无法安装更新", &message);
        }
    }
}

fn set_status(app: &AppHandle, manager: &UpdateManager, status: UpdateStatus) {
    manager.set_status(status);
    crate::desktop::tray::update_updater_status(app);
}

/// 限制原生对话框中的更新说明长度，避免超长 Release 内容遮挡操作按钮。
fn format_release_notes(notes: Option<&str>) -> String {
    let notes = notes.unwrap_or_default().trim();
    if notes.is_empty() {
        return "此版本未提供更新说明。".to_string();
    }
    let mut chars = notes.chars();
    let mut result = chars
        .by_ref()
        .take(MAX_RELEASE_NOTES_CHARS)
        .collect::<String>();
    if chars.next().is_some() {
        result.push_str("……");
    }
    result
}

/// 等待原生对话框结果，确保用户明确同意后才开始下载完整更新包。
async fn confirm_install(app: &AppHandle, version: &str, notes: Option<&str>) -> bool {
    let message = format!(
        "发现 DSH Desktop {version}。\n\n{}\n\n更新将下载完整安装包，并在验签后重新启动应用。",
        format_release_notes(notes)
    );
    let (sender, receiver) = oneshot::channel();
    app.dialog()
        .message(message)
        .title(format!("发现新版本 {version}"))
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "下载并安装".into(),
            "稍后".into(),
        ))
        .show(move |accepted| {
            let _ = sender.send(accepted);
        });
    receiver.await.unwrap_or(false)
}

fn show_info(app: &AppHandle, title: &str, message: &str) {
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::Ok)
        .show(|_| {});
}

fn show_error(app: &AppHandle, title: &str, message: &str) {
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::Ok)
        .show(|_| {});
}

/// 自动检查只写入诊断；手动检查额外弹窗，避免后台网络波动打断用户。
fn handle_failure(
    app: &AppHandle,
    manager: &UpdateManager,
    source: UpdateCheckSource,
    context: &str,
    error: &dyn Display,
) {
    let message = format!("{context}：{error}");
    set_status(
        app,
        manager,
        UpdateStatus::Failed {
            source,
            message: message.clone(),
        },
    );
    app.state::<Arc<RuntimeManager>>()
        .record_system_diagnostic(&message);
    if source == UpdateCheckSource::Manual {
        show_error(app, "无法检查更新", &message);
    }
}
