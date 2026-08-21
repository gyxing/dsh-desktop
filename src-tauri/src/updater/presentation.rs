use super::status::UpdateStatus;

const BYTES_PER_MEBIBYTE: f64 = 1024.0 * 1024.0;

/// 描述更新状态在原生标题栏、托盘和任务栏中的统一展示。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdatePresentation {
    pub title_suffix: Option<String>,
    pub tray_status_label: String,
    pub action_label: String,
    pub action_enabled: bool,
    pub taskbar_progress: UpdateTaskbarProgress,
}

/// 将平台任务栏进度限制为隐藏、不确定或0到100的确定进度。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateTaskbarProgress {
    Hidden,
    Indeterminate,
    Percentage(u64),
}

/// 把更新内部状态映射为所有原生界面共用的可见文案和交互状态。
pub fn update_presentation(status: &UpdateStatus) -> UpdatePresentation {
    match status {
        UpdateStatus::Idle => presentation("更新：尚未检查", "检查更新", true),
        UpdateStatus::Checking { .. } => presentation("更新：正在检查", "正在检查更新", false),
        UpdateStatus::UpToDate => presentation("更新：已是最新", "检查更新", true),
        UpdateStatus::Available { version, .. } => UpdatePresentation {
            title_suffix: Some(format!("有更新 {version}")),
            tray_status_label: format!("更新：发现 {version}"),
            action_label: format!("下载并安装 {version}"),
            action_enabled: true,
            taskbar_progress: UpdateTaskbarProgress::Hidden,
        },
        UpdateStatus::Downloading {
            downloaded,
            total,
            bytes_per_second,
            eta_seconds,
            ..
        } => {
            let (summary, progress) = download_summary(
                "正在下载",
                *downloaded,
                *total,
                *bytes_per_second,
                *eta_seconds,
            );
            UpdatePresentation {
                title_suffix: Some(summary.clone()),
                tray_status_label: format!("更新：{summary}"),
                action_label: "下载中，请稍候".to_string(),
                action_enabled: false,
                taskbar_progress: progress,
            }
        }
        UpdateStatus::Retrying {
            downloaded,
            total,
            bytes_per_second,
            eta_seconds,
            next_attempt,
            max_attempts,
            ..
        } => {
            let action = format!("正在续传 {next_attempt}/{max_attempts}");
            let (summary, progress) = download_summary(
                &action,
                *downloaded,
                *total,
                *bytes_per_second,
                *eta_seconds,
            );
            UpdatePresentation {
                title_suffix: Some(summary.clone()),
                tray_status_label: format!("更新：{summary}"),
                action_label: "正在恢复下载".to_string(),
                action_enabled: false,
                taskbar_progress: progress,
            }
        }
        UpdateStatus::Verifying { .. } => UpdatePresentation {
            title_suffix: Some("正在校验更新".to_string()),
            tray_status_label: "更新：正在校验".to_string(),
            action_label: "正在校验更新".to_string(),
            action_enabled: false,
            taskbar_progress: UpdateTaskbarProgress::Indeterminate,
        },
        UpdateStatus::Installing { .. } => UpdatePresentation {
            title_suffix: Some("正在安装更新".to_string()),
            tray_status_label: "更新：正在安装".to_string(),
            action_label: "正在安装更新".to_string(),
            action_enabled: false,
            taskbar_progress: UpdateTaskbarProgress::Indeterminate,
        },
        UpdateStatus::Failed { .. } => presentation("更新：操作失败", "重新检查更新", true),
    }
}

fn presentation(
    status_label: &str,
    action_label: &str,
    action_enabled: bool,
) -> UpdatePresentation {
    UpdatePresentation {
        title_suffix: None,
        tray_status_label: status_label.to_string(),
        action_label: action_label.to_string(),
        action_enabled,
        taskbar_progress: UpdateTaskbarProgress::Hidden,
    }
}

fn download_summary(
    action: &str,
    downloaded: u64,
    total: Option<u64>,
    bytes_per_second: Option<u64>,
    eta_seconds: Option<u64>,
) -> (String, UpdateTaskbarProgress) {
    let (mut summary, progress) = match total.filter(|total| *total > 0) {
        Some(total) => {
            let downloaded = downloaded.min(total);
            let percentage = ((downloaded as u128 * 100) / total as u128) as u64;
            (
                format!(
                    "{action} {percentage}%（{:.1} / {:.1} MB）",
                    downloaded as f64 / BYTES_PER_MEBIBYTE,
                    total as f64 / BYTES_PER_MEBIBYTE
                ),
                UpdateTaskbarProgress::Percentage(percentage),
            )
        }
        None => (
            format!(
                "{action}（已完成 {:.1} MB）",
                downloaded as f64 / BYTES_PER_MEBIBYTE
            ),
            UpdateTaskbarProgress::Indeterminate,
        ),
    };
    if let Some(bytes_per_second) = bytes_per_second.filter(|speed| *speed > 0) {
        summary.push_str(&format!(
            " · {:.1} MB/s",
            bytes_per_second as f64 / BYTES_PER_MEBIBYTE
        ));
    }
    if let Some(eta_seconds) = eta_seconds.filter(|seconds| *seconds > 0) {
        summary.push_str(&format!(" · {}", format_eta(eta_seconds)));
    }
    (summary, progress)
}

fn format_eta(seconds: u64) -> String {
    if seconds < 60 {
        format!("约 {seconds} 秒")
    } else {
        let minutes = seconds / 60;
        let remaining_seconds = seconds % 60;
        if remaining_seconds == 0 {
            format!("约 {minutes} 分钟")
        } else {
            format!("约 {minutes} 分 {remaining_seconds} 秒")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::update_presentation;
    use crate::updater::status::UpdateStatus;

    #[test]
    fn download_presentation_includes_speed_and_eta_after_the_sample_stabilizes() {
        let presentation = update_presentation(&UpdateStatus::Downloading {
            version: "0.1.3".to_string(),
            downloaded: 24 * 1024 * 1024,
            total: Some(54 * 1024 * 1024),
            bytes_per_second: Some(2 * 1024 * 1024),
            eta_seconds: Some(15),
        });

        let title = presentation.title_suffix.expect("下载状态应显示在标题栏");
        assert!(title.contains("44%"));
        assert!(title.contains("2.0 MB/s"));
        assert!(title.contains("约 15 秒"));
    }

    #[test]
    fn retry_presentation_includes_the_attempt_number() {
        let presentation = update_presentation(&UpdateStatus::Retrying {
            version: "0.1.3".to_string(),
            downloaded: 24 * 1024 * 1024,
            total: Some(54 * 1024 * 1024),
            bytes_per_second: None,
            eta_seconds: None,
            next_attempt: 3,
            max_attempts: 5,
        });

        assert!(presentation
            .title_suffix
            .expect("续传状态应显示在标题栏")
            .contains("3/5"));
    }
}
