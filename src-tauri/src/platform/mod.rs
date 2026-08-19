use std::path::PathBuf;

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::desktop::terminal::TerminalError;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(unix)]
mod unix_terminal;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as current;
#[cfg(target_os = "macos")]
use macos as current;
#[cfg(windows)]
use windows as current;

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
compile_error!("DSH Desktop当前只支持Windows、macOS和Linux桌面目标");

/// 解析Tauri打包后与主程序同目录的Node Sidecar路径。
pub fn resolve_node_executable() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map_err(|error| format!("无法解析桌面程序路径：{error}"))?
        .parent()
        .map(|directory| directory.join(current::NODE_EXECUTABLE_NAME))
        .ok_or_else(|| "桌面程序路径没有父目录".to_string())
}

/// 按当前平台打开外部终端。
pub fn open_external_terminal(app: &AppHandle) -> Result<(), TerminalError> {
    current::open_external_terminal(app)
}
/// 使用已有Tauri对话框插件展示跨平台原生错误。
pub fn show_native_error(app: &AppHandle, title: &str, message: &str) {
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::Ok)
        .show(|_| {});
}
