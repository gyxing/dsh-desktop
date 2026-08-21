use serde::{Deserialize, Serialize};
use tauri::{App, AppHandle, Emitter, Manager, Webview};
use url::Url;

#[cfg(any(windows, target_os = "linux", test))]
use tauri::WebviewUrl;
#[cfg(any(windows, target_os = "linux"))]
use tauri::{webview::WebviewBuilder, PhysicalPosition, PhysicalSize, Position, Rect, Size};

use crate::updater::{manager::UpdateManager, presentation::update_presentation};

pub const WINDOW_CHROME_LABEL: &str = "window-chrome";
#[cfg(any(windows, target_os = "linux", test))]
const CHROME_LOGICAL_HEIGHT: f64 = 36.0;

#[cfg(any(windows, target_os = "linux", test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChromeLayout {
    pub width: u32,
    pub chrome_height: u32,
    pub content_y: u32,
    pub content_height: u32,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowChromeAction {
    StartDragging,
    ToggleMaximize,
    Minimize,
    Close,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowChromeState {
    pub maximized: bool,
    pub title: String,
    pub update_text: Option<String>,
}

/// 把单行36px标题栏换算成物理像素，并为DSH内容保留剩余区域。
#[cfg(any(windows, target_os = "linux", test))]
pub fn calculate_chrome_layout(width: u32, height: u32, scale_factor: f64) -> ChromeLayout {
    let chrome_height = ((CHROME_LOGICAL_HEIGHT * scale_factor).round() as u32).min(height);
    ChromeLayout {
        width,
        chrome_height,
        content_y: chrome_height,
        content_height: height.saturating_sub(chrome_height),
    }
}

pub fn is_trusted_chrome_label(label: &str) -> bool {
    label == WINDOW_CHROME_LABEL
}

#[cfg(any(windows, target_os = "linux", test))]
pub fn resolve_chrome_url(dev_url: Option<&Url>) -> WebviewUrl {
    match dev_url {
        Some(dev_url) => {
            let mut url = dev_url.clone();
            url.set_path("/titlebar.html");
            url.set_query(None);
            url.set_fragment(None);
            WebviewUrl::External(url)
        }
        None => WebviewUrl::App("titlebar.html".into()),
    }
}

/// 标题栏只允许加载桌面壳同源的titlebar.html，不继承DSH内容来源。
pub fn is_chrome_navigation_allowed(target: &Url, shell_url: Option<&Url>) -> bool {
    let Some(shell_url) = shell_url else {
        return false;
    };
    let Ok(expected) = shell_url.join("titlebar.html") else {
        return false;
    };
    target.scheme() == expected.scheme()
        && target.host_str() == expected.host_str()
        && target.port_or_known_default() == expected.port_or_known_default()
        && target.path() == expected.path()
        && target.query().is_none()
        && target.fragment().is_none()
}

/// 创建独立本地标题栏，并把原main WebView下移到单行标题栏之后。
pub fn setup(app: &mut App) -> tauri::Result<()> {
    let main = app
        .get_webview_window("main")
        .ok_or(tauri::Error::WindowNotFound)?;

    #[cfg(any(windows, target_os = "linux"))]
    {
        main.set_decorations(false)?;
        let window = main.as_ref().window();
        let size = window.inner_size()?;
        let scale_factor = window.scale_factor()?;
        let layout = calculate_chrome_layout(size.width, size.height, scale_factor);
        apply_layout(app.handle(), layout)?;
        let url = {
            #[cfg(dev)]
            let dev_url = app.config().build.dev_url.as_ref();
            #[cfg(not(dev))]
            let dev_url = None;
            resolve_chrome_url(dev_url)
        };
        window.add_child(
            WebviewBuilder::new(WINDOW_CHROME_LABEL, url),
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(layout.width, layout.chrome_height),
        )?;
    }

    main.show()?;
    emit_state(app.handle());
    Ok(())
}

#[cfg(any(windows, target_os = "linux"))]
pub fn resize_to(app: &AppHandle, width: u32, height: u32) {
    if let Some(main) = app.get_webview("main") {
        let scale_factor = main.window().scale_factor().unwrap_or(1.0);
        let _ = apply_layout(app, calculate_chrome_layout(width, height, scale_factor));
    }
    emit_state(app);
}

#[cfg(target_os = "macos")]
pub fn resize_to(app: &AppHandle, _: u32, _: u32) {
    emit_state(app);
}

#[cfg(any(windows, target_os = "linux"))]
fn apply_layout(app: &AppHandle, layout: ChromeLayout) -> tauri::Result<()> {
    if let Some(main) = app.get_webview("main") {
        main.set_bounds(Rect {
            position: Position::Physical(PhysicalPosition::new(0, layout.content_y as i32)),
            size: Size::Physical(PhysicalSize::new(layout.width, layout.content_height)),
        })?;
    }
    if let Some(chrome) = app.get_webview(WINDOW_CHROME_LABEL) {
        chrome.set_bounds(Rect {
            position: Position::Physical(PhysicalPosition::new(0, 0)),
            size: Size::Physical(PhysicalSize::new(layout.width, layout.chrome_height)),
        })?;
    }
    Ok(())
}

#[tauri::command]
pub fn window_chrome_state(webview: Webview) -> Result<WindowChromeState, String> {
    require_chrome(&webview)?;
    build_state(webview.app_handle())
}

#[tauri::command]
pub fn window_chrome_action(webview: Webview, action: WindowChromeAction) -> Result<(), String> {
    require_chrome(&webview)?;
    let window = webview.window();
    match action {
        WindowChromeAction::StartDragging => window.start_dragging(),
        WindowChromeAction::ToggleMaximize => {
            if window.is_maximized().map_err(|error| error.to_string())? {
                window.unmaximize()
            } else {
                window.maximize()
            }
        }
        WindowChromeAction::Minimize => window.minimize(),
        // 关闭按钮沿用既定的隐藏到托盘行为，明确退出仍由菜单或托盘负责。
        WindowChromeAction::Close => window.hide(),
    }
    .map_err(|error| error.to_string())?;
    emit_state(webview.app_handle());
    Ok(())
}

pub fn emit_state(app: &AppHandle) {
    let Some(chrome) = app.get_webview(WINDOW_CHROME_LABEL) else {
        return;
    };
    if let Ok(state) = build_state(app) {
        let _ = chrome.emit("window-chrome://state", state);
    }
}

fn build_state(app: &AppHandle) -> Result<WindowChromeState, String> {
    let window = app
        .get_window("main")
        .ok_or_else(|| "主窗口不存在".to_string())?;
    let update = app
        .try_state::<std::sync::Arc<UpdateManager>>()
        .map(|manager| update_presentation(&manager.status()));
    Ok(WindowChromeState {
        maximized: window.is_maximized().map_err(|error| error.to_string())?,
        title: format!("DSH Desktop {}", app.package_info().version),
        update_text: update.and_then(|presentation| presentation.title_suffix),
    })
}

fn require_chrome(webview: &Webview) -> Result<(), String> {
    if is_trusted_chrome_label(webview.label()) {
        Ok(())
    } else {
        Err("只有本地标题栏可以执行窗口操作".to_string())
    }
}

#[cfg(test)]
#[path = "chrome_tests.rs"]
mod tests;
