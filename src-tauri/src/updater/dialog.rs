use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tokio::sync::oneshot;

const MAX_RELEASE_NOTES_CHARS: usize = 2_000;

/// 限制原生对话框中的更新说明长度，避免超长Release内容遮挡操作按钮。
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
pub(super) async fn confirm_install(app: &AppHandle, version: &str, notes: Option<&str>) -> bool {
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

/// 显示无需用户选择的信息提示，不阻塞当前异步任务。
pub(super) fn show_info(app: &AppHandle, title: &str, message: &str) {
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::Ok)
        .show(|_| {});
}

/// 显示带确定按钮的原生错误提示。
pub(super) fn show_error(app: &AppHandle, title: &str, message: &str) {
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Error)
        .buttons(MessageDialogButtons::Ok)
        .show(|_| {});
}
