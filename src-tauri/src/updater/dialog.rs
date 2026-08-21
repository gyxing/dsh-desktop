use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tokio::sync::oneshot;
use url::Url;

pub const UPDATE_DIALOG_LABEL: &str = "update-dialog";
const MAX_RELEASE_NOTES_CHARS: usize = 20_000;
const UPDATE_DIALOG_WIDTH: f64 = 560.0;
const UPDATE_DIALOG_HEIGHT: f64 = 420.0;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDialogPayload {
    pub version: String,
    pub notes: String,
    pub confirmation: bool,
}

struct UpdateDialogState {
    payload: Option<UpdateDialogPayload>,
    response: Option<oneshot::Sender<bool>>,
}

/// 管理本地更新窗口的数据和一次性确认通道，不向远程DSH页面暴露状态。
pub struct UpdateDialogManager {
    state: Mutex<UpdateDialogState>,
}

impl UpdateDialogManager {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(UpdateDialogState {
                payload: None,
                response: None,
            }),
        }
    }

    pub fn begin_confirmation(&self, payload: UpdateDialogPayload) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        let mut state = self.lock();
        if let Some(previous) = state.response.take() {
            let _ = previous.send(false);
        }
        state.payload = Some(payload);
        state.response = Some(sender);
        receiver
    }

    fn set_view(&self, payload: UpdateDialogPayload) {
        let mut state = self.lock();
        if state.response.is_none() {
            state.payload = Some(payload);
        }
    }

    pub fn payload(&self) -> Option<UpdateDialogPayload> {
        self.lock().payload.clone()
    }

    pub fn respond(&self, accepted: bool) -> bool {
        let sender = self.lock().response.take();
        sender.is_some_and(|sender| sender.send(accepted).is_ok())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, UpdateDialogState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Default for UpdateDialogManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 使用固定尺寸的本地窗口展示更新内容，正文独立滚动，避免系统弹窗无限增高。
pub(super) async fn confirm_install(app: &AppHandle, version: &str, notes: Option<&str>) -> bool {
    let manager = app.state::<Arc<UpdateDialogManager>>().inner().clone();
    let payload = payload(version, notes, true);
    let receiver = manager.begin_confirmation(payload.clone());
    if let Err(error) = show_window(app, &payload) {
        manager.respond(false);
        show_error(app, "无法显示更新内容", &error);
    }
    receiver.await.unwrap_or(false)
}

/// 从顶部菜单重新查看已经发现的版本，不启动第二个下载任务。
pub fn show_release_notes(app: &AppHandle, version: &str, notes: Option<&str>) {
    let manager = app.state::<Arc<UpdateDialogManager>>();
    let payload = payload(version, notes, false);
    manager.set_view(payload.clone());
    if let Some(current) = manager.payload() {
        let _ = show_window(app, &current);
    }
}

#[cfg(debug_assertions)]
pub fn show_preview(app: &AppHandle) {
    let manager = app.state::<Arc<UpdateDialogManager>>();
    let payload = payload(
        "0.1.3",
        Some(
            "## 本版更新\n\n- 下载中断后自动从磁盘断点续传\n- 显示下载速度、剩余时间和续传次数\n- 新增顶部原生菜单和发布后公开烟测\n\n更新内容较长时，只滚动中间区域，窗口高度保持不变。",
        ),
        true,
    );
    manager.set_view(payload.clone());
    let _ = show_window(app, &payload);
}

pub fn dismiss(app: &AppHandle) {
    if let Some(manager) = app.try_state::<Arc<UpdateDialogManager>>() {
        manager.respond(false);
    }
    if let Some(window) = app.get_webview_window(UPDATE_DIALOG_LABEL) {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn update_dialog_payload(
    window: WebviewWindow,
    manager: State<'_, Arc<UpdateDialogManager>>,
) -> Result<UpdateDialogPayload, String> {
    require_update_window(&window)?;
    manager
        .payload()
        .ok_or_else(|| "当前没有可显示的更新内容".to_string())
}

#[tauri::command]
pub fn respond_update_dialog(
    window: WebviewWindow,
    manager: State<'_, Arc<UpdateDialogManager>>,
    accepted: bool,
) -> Result<(), String> {
    require_update_window(&window)?;
    window.hide().map_err(|error| error.to_string())?;
    manager.respond(accepted);
    Ok(())
}

fn require_update_window(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == UPDATE_DIALOG_LABEL {
        Ok(())
    } else {
        Err("只有本地更新窗口可以执行该操作".to_string())
    }
}

fn payload(version: &str, notes: Option<&str>, confirmation: bool) -> UpdateDialogPayload {
    UpdateDialogPayload {
        version: version.to_string(),
        notes: format_release_notes(notes),
        confirmation,
    }
}

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
        result.push_str("\n\n……内容过长，已截断");
    }
    result
}

fn show_window(app: &AppHandle, payload: &UpdateDialogPayload) -> Result<(), String> {
    let window = match app.get_webview_window(UPDATE_DIALOG_LABEL) {
        Some(window) => window,
        None => WebviewWindowBuilder::new(app, UPDATE_DIALOG_LABEL, {
            #[cfg(dev)]
            let dev_url = app.config().build.dev_url.as_ref();
            #[cfg(not(dev))]
            let dev_url = None;
            resolve_update_dialog_url(dev_url)
        })
        .title(format!("DSH Desktop {} 更新", payload.version))
        .inner_size(UPDATE_DIALOG_WIDTH, UPDATE_DIALOG_HEIGHT)
        .min_inner_size(UPDATE_DIALOG_WIDTH, UPDATE_DIALOG_HEIGHT)
        .max_inner_size(UPDATE_DIALOG_WIDTH, UPDATE_DIALOG_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .center()
        .build()
        .map_err(|error| error.to_string())?,
    };
    window
        .set_title(&format!("DSH Desktop {} 更新", payload.version))
        .map_err(|error| error.to_string())?;
    window
        .emit("updater-dialog://payload", payload.clone())
        .map_err(|error| error.to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

fn resolve_update_dialog_url(dev_url: Option<&Url>) -> WebviewUrl {
    match dev_url {
        Some(dev_url) => {
            let mut url = dev_url.clone();
            url.set_path("/update.html");
            url.set_query(None);
            url.set_fragment(None);
            WebviewUrl::External(url)
        }
        None => WebviewUrl::App("update.html".into()),
    }
}

/// 更新窗口只允许导航到桌面壳同源的update.html，不继承主窗口的DSH运行时来源。
pub fn is_update_dialog_navigation_allowed(target: &Url, shell_url: Option<&Url>) -> bool {
    let Some(shell_url) = shell_url else {
        return false;
    };
    let Ok(expected) = shell_url.join("update.html") else {
        return false;
    };
    target.scheme() == expected.scheme()
        && target.host_str() == expected.host_str()
        && target.port_or_known_default() == expected.port_or_known_default()
        && target.path() == expected.path()
        && target.query().is_none()
        && target.fragment().is_none()
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

#[cfg(test)]
#[path = "dialog_tests.rs"]
mod tests;
