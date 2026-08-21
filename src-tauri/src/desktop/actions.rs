use std::sync::Arc;

use tauri::{AppHandle, Manager};
use url::Url;

use super::{lifecycle::AppLifecycle, navigation, terminal::open_dsh_terminal};
use crate::{
    platform,
    runtime::manager::RuntimeManager,
    updater::{dialog, manager::UpdateManager, status::UpdateStatus},
};

const RELEASES_URL: &str = "https://github.com/gyxing/dsh-desktop/releases";

pub fn open_terminal(app: &AppHandle) {
    if let Err(error) = open_dsh_terminal(app) {
        app.state::<Arc<RuntimeManager>>()
            .record_system_diagnostic(&format!("打开 DSH 终端失败：{error}"));
        platform::show_native_error(app, "无法打开 DSH 终端", &error.to_string());
    }
}

pub fn restart_runtime(app: &AppHandle) {
    let runtime = app.state::<Arc<RuntimeManager>>().inner().clone();
    if let Err(error) = runtime.start(app) {
        platform::show_native_error(app, "无法重新启动 DeepSeek Harness", &error);
    }
}

pub fn check_update(app: &AppHandle) {
    crate::updater::schedule::spawn_manual_check(app);
}

pub fn show_update_notes(app: &AppHandle) {
    match app.state::<Arc<UpdateManager>>().status() {
        UpdateStatus::Available { version, notes } => {
            dialog::show_release_notes(app, &version, notes.as_deref());
        }
        _ => platform::show_native_info(app, "更新内容", "当前没有待安装的新版本。"),
    }
}

pub fn copy_diagnostics(app: &AppHandle) {
    let diagnostics = app.state::<Arc<RuntimeManager>>().diagnostics();
    let result =
        arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(diagnostics));
    match result {
        Ok(()) => platform::show_native_info(app, "诊断信息", "诊断信息已复制到剪贴板。"),
        Err(error) => {
            app.state::<Arc<RuntimeManager>>()
                .record_system_diagnostic(&format!("复制诊断信息失败：{error}"));
            platform::show_native_error(app, "无法复制诊断信息", &error.to_string());
        }
    }
}

pub fn open_releases(app: &AppHandle) {
    let target = Url::parse(RELEASES_URL).expect("固定发布地址应有效");
    if let Err(error) = navigation::open_external(app, &target) {
        app.state::<Arc<RuntimeManager>>()
            .record_system_diagnostic(&format!("打开发布页面失败：{error}"));
        platform::show_native_error(app, "无法打开发布页面", &error);
    }
}

pub fn quit(app: &AppHandle) {
    let lifecycle = app.state::<AppLifecycle>();
    if lifecycle.request_quit() {
        app.state::<Arc<RuntimeManager>>().stop();
        app.exit(0);
    }
}
