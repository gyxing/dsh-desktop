use std::time::Duration;

use tauri::AppHandle;

use super::{service::run_check, status::UpdateCheckSource};

const AUTOMATIC_CHECK_DELAY: Duration = Duration::from_secs(30);
const AUTOMATIC_CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// 启动后延迟检查，并为长期驻留托盘的应用每天重新检查一次。
pub fn spawn_automatic_check(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(AUTOMATIC_CHECK_DELAY).await;
        loop {
            run_check(app.clone(), UpdateCheckSource::Automatic).await;
            tokio::time::sleep(AUTOMATIC_CHECK_INTERVAL).await;
        }
    });
}

/// 立即执行用户从托盘或顶部菜单发起的更新检查。
pub fn spawn_manual_check(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run_check(app, UpdateCheckSource::Manual).await;
    });
}
