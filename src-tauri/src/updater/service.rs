use std::{
    fmt::Display,
    sync::Arc,
    time::{Duration, Instant},
};

use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

use crate::{desktop::lifecycle::AppLifecycle, runtime::manager::RuntimeManager};

use super::{
    dialog::{confirm_install, show_error, show_info},
    download::{download_with_resume, DownloadError},
    manager::UpdateManager,
    progress::calculate_transfer_measurement,
    signature::verify_signature_file,
    status::{UpdateCheckSource, UpdateStatus},
};

const UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const PROGRESS_REFRESH_INTERVAL: Duration = Duration::from_millis(250);

/// 串行完成检查、用户确认、验签下载和安装，所有失败路径都保留可恢复状态。
pub(super) async fn run_check(app: AppHandle, source: UpdateCheckSource) {
    let manager = app.state::<Arc<UpdateManager>>().inner().clone();
    let Some(_guard) = manager.try_begin(source) else {
        if source == UpdateCheckSource::Manual {
            show_info(&app, "正在检查更新", "已有更新任务正在进行，请稍候。");
        }
        return;
    };
    refresh_update_surfaces(&app);

    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(error) => {
            handle_failure(&app, &manager, source, "初始化更新器失败", &error);
            return;
        }
    };
    let update = match tokio::time::timeout(UPDATE_CHECK_TIMEOUT, updater.check()).await {
        Ok(Ok(update)) => update,
        Ok(Err(error)) => {
            handle_failure(&app, &manager, source, "检查更新失败", &error);
            return;
        }
        Err(_) => {
            handle_failure(
                &app,
                &manager,
                source,
                "检查更新失败",
                &"连接更新服务器超时",
            );
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

    let public_key = match updater_public_key(&app) {
        Ok(public_key) => public_key,
        Err(error) => {
            handle_download_failure(&app, &manager, source, &error);
            return;
        }
    };
    let progress_app = app.clone();
    let progress_manager = manager.clone();
    let progress_version = version.clone();
    let retry_app = app.clone();
    let retry_manager = manager.clone();
    let retry_version = version.clone();
    let mut last_refresh = None::<Instant>;
    let mut progress_initial_bytes = None::<u64>;
    let mut progress_started_at = None::<Instant>;
    set_status(
        &app,
        &manager,
        UpdateStatus::Downloading {
            version: version.clone(),
            downloaded: 0,
            total: None,
            bytes_per_second: None,
            eta_seconds: None,
        },
    );
    let downloaded_update = match download_with_resume(
        &app,
        &update,
        move |downloaded, total| {
            let initial_bytes = *progress_initial_bytes.get_or_insert(downloaded);
            let started_at = *progress_started_at.get_or_insert_with(Instant::now);
            let measurement = calculate_transfer_measurement(
                initial_bytes,
                downloaded,
                total,
                started_at.elapsed(),
            );
            progress_manager.set_status(UpdateStatus::Downloading {
                version: progress_version.clone(),
                downloaded,
                total,
                bytes_per_second: measurement.bytes_per_second,
                eta_seconds: measurement.eta_seconds,
            });
            let completed = total.is_some_and(|total| total > 0 && downloaded >= total);
            let should_refresh = last_refresh
                .map(|last| last.elapsed() >= PROGRESS_REFRESH_INTERVAL)
                .unwrap_or(true)
                || completed;
            if should_refresh {
                last_refresh = Some(Instant::now());
                refresh_update_surfaces(&progress_app);
            }
        },
        move |downloaded, total, next_attempt, max_attempts, error| {
            retry_manager.set_status(UpdateStatus::Retrying {
                version: retry_version.clone(),
                downloaded,
                total,
                bytes_per_second: None,
                eta_seconds: None,
                next_attempt,
                max_attempts,
            });
            refresh_update_surfaces(&retry_app);
            retry_app
                .state::<Arc<RuntimeManager>>()
                .record_system_diagnostic(&format!(
                    "更新包连接中断，将从 {downloaded} 字节续传（第{next_attempt}/{max_attempts}次）：{error}"
                ));
        },
    )
    .await
    {
        Ok(downloaded_update) => downloaded_update,
        Err(error) => {
            handle_download_failure(&app, &manager, source, &error);
            return;
        }
    };

    set_status(
        &app,
        &manager,
        UpdateStatus::Verifying {
            version: version.clone(),
        },
    );
    if let Err(error) =
        verify_signature_file(downloaded_update.path(), &update.signature, &public_key)
    {
        let _ = downloaded_update.clear();
        handle_download_failure(&app, &manager, source, &error);
        return;
    }

    let bytes = match downloaded_update.read_for_install().await {
        Ok(bytes) => bytes,
        Err(error) => {
            handle_download_failure(&app, &manager, source, &error);
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
    match update.install(&bytes) {
        Ok(()) => {
            let _ = downloaded_update.clear();
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
    refresh_update_surfaces(app);
}

fn refresh_update_surfaces(app: &AppHandle) {
    crate::desktop::tray::update_updater_status(app);
    crate::desktop::menu::update_updater_status(app);
    crate::app::update_main_window_presentation(app);
}

fn updater_public_key(app: &AppHandle) -> Result<String, DownloadError> {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|config| config.get("pubkey"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| DownloadError::Configuration("缺少updater.pubkey".to_string()))
}

/// 下载已由用户确认，因此失败时始终给出明确的恢复建议并保留原始诊断。
fn handle_download_failure(
    app: &AppHandle,
    manager: &UpdateManager,
    source: UpdateCheckSource,
    error: &DownloadError,
) {
    let technical_message = error.to_string();
    let user_message = if error.is_retryable() {
        "更新包多次续传后仍未完成。请检查网络或安全软件后重新下载。".to_string()
    } else {
        "更新包下载或签名校验失败，请稍后重新下载。".to_string()
    };
    set_status(
        app,
        manager,
        UpdateStatus::Failed {
            source,
            message: user_message.clone(),
        },
    );
    app.state::<Arc<RuntimeManager>>()
        .record_system_diagnostic(&format!("{user_message} 技术信息：{technical_message}"));
    show_error(
        app,
        "无法下载更新",
        &format!(
            "{user_message}\n\n错误详情：{technical_message}\n\n可从顶部“帮助 > 复制诊断信息”复制脱敏诊断。"
        ),
    );
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
