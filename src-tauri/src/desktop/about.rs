use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use url::Url;

use super::{navigation, terminal::runtime_versions};

pub const ABOUT_DIALOG_LABEL: &str = "about-dialog";
const ABOUT_DIALOG_WIDTH: f64 = 600.0;
const ABOUT_DIALOG_HEIGHT: f64 = 380.0;
const PROJECT_URL: &str = "https://github.com/gyxing/dsh-desktop";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutDialogPayload {
    app_name: String,
    version: String,
    description: String,
    disclaimer: String,
    build_timestamp_ms: u64,
    build_id: String,
    platform: String,
    dsh_version: String,
    node_version: String,
    pnpm_version: String,
    website: String,
    author: String,
}

/// 打开单例 About 窗口，重复点击菜单时仅聚焦已有窗口。
pub fn show(app: &AppHandle) {
    if let Err(error) = show_window(app) {
        crate::platform::show_native_error(app, "无法显示关于信息", &error);
    }
}

#[tauri::command]
/// 仅向本地 About Webview 返回可公开的版本信息。
pub fn about_dialog_payload(window: WebviewWindow) -> Result<AboutDialogPayload, String> {
    require_about_window(&window)?;
    Ok(payload(window.app_handle()))
}

#[tauri::command]
/// 复制 About 窗口生成的公开版本摘要，并限制异常超长输入。
pub fn copy_about_info(window: WebviewWindow, text: String) -> Result<(), String> {
    require_about_window(&window)?;
    if text.len() > 4_096 {
        return Err("版本信息过长".to_string());
    }
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .map_err(|error| error.to_string())
}

#[tauri::command]
/// 使用系统默认浏览器打开固定项目主页。
pub fn open_about_website(window: WebviewWindow) -> Result<(), String> {
    require_about_window(&window)?;
    let target = Url::parse(PROJECT_URL).expect("固定项目地址应有效");
    navigation::open_external(window.app_handle(), &target)
}

#[tauri::command]
/// 关闭当前 About 窗口。
pub fn close_about_dialog(window: WebviewWindow) -> Result<(), String> {
    require_about_window(&window)?;
    window.close().map_err(|error| error.to_string())
}

fn resolve_build_timestamp_ms(explicit: Option<&str>, fallback: u64) -> u64 {
    explicit
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(|seconds| seconds.saturating_mul(1_000))
        .unwrap_or(fallback)
}

fn resolve_build_id(explicit: Option<&str>, github_sha: Option<&str>, debug_build: bool) -> String {
    if let Some(value) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return value.to_string();
    }
    if let Some(value) = github_sha.map(str::trim).filter(|value| !value.is_empty()) {
        return value.chars().take(7).collect();
    }
    if debug_build {
        "local-debug".to_string()
    } else {
        "local-release".to_string()
    }
}

fn payload(app: &AppHandle) -> AboutDialogPayload {
    let (node_version, dsh_version, pnpm_version) = runtime_versions()
        .map(|versions| (versions.node, versions.dsh, versions.pnpm))
        .unwrap_or_else(|_| {
            (
                "未知版本".to_string(),
                "未知版本".to_string(),
                "未知版本".to_string(),
            )
        });
    let executable_timestamp_ms = std::env::current_exe()
        .and_then(std::fs::metadata)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|timestamp| timestamp.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default();
    let build_timestamp_ms = resolve_build_timestamp_ms(
        option_env!("DSH_DESKTOP_BUILD_TIMESTAMP"),
        executable_timestamp_ms,
    );
    AboutDialogPayload {
        app_name: "DSH Desktop".to_string(),
        version: app.package_info().version.to_string(),
        description: "DeepSeek Harness 的第三方跨平台桌面封装".to_string(),
        disclaimer: "非 DeepSeek 官方产品".to_string(),
        build_timestamp_ms,
        build_id: resolve_build_id(
            option_env!("DSH_DESKTOP_BUILD_ID"),
            option_env!("GITHUB_SHA"),
            cfg!(debug_assertions),
        ),
        platform: platform_label(std::env::consts::OS, std::env::consts::ARCH),
        dsh_version,
        node_version,
        pnpm_version,
        website: PROJECT_URL.to_string(),
        author: env!("CARGO_PKG_AUTHORS").replace(':', ", "),
    }
}

fn show_window(app: &AppHandle) -> Result<(), String> {
    let window = match app.get_webview_window(ABOUT_DIALOG_LABEL) {
        Some(window) => window,
        None => WebviewWindowBuilder::new(app, ABOUT_DIALOG_LABEL, {
            #[cfg(dev)]
            let dev_url = app.config().build.dev_url.as_ref();
            #[cfg(not(dev))]
            let dev_url = None;
            resolve_about_dialog_url(dev_url)
        })
        .title("关于 DSH Desktop")
        .inner_size(ABOUT_DIALOG_WIDTH, ABOUT_DIALOG_HEIGHT)
        .min_inner_size(ABOUT_DIALOG_WIDTH, ABOUT_DIALOG_HEIGHT)
        .max_inner_size(ABOUT_DIALOG_WIDTH, ABOUT_DIALOG_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .center()
        .build()
        .map_err(|error| error.to_string())?,
    };
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())
}

fn resolve_about_dialog_url(dev_url: Option<&Url>) -> WebviewUrl {
    match dev_url {
        Some(dev_url) => {
            let mut url = dev_url.clone();
            url.set_path("/about.html");
            url.set_query(None);
            url.set_fragment(None);
            WebviewUrl::External(url)
        }
        None => WebviewUrl::App("about.html".into()),
    }
}

fn require_about_window(window: &WebviewWindow) -> Result<(), String> {
    (window.label() == ABOUT_DIALOG_LABEL)
        .then_some(())
        .ok_or_else(|| "只有本地关于窗口可以执行该操作".to_string())
}

/// 把 Rust 目标名称转换为面向用户的公开平台说明。
pub fn platform_label(operating_system: &str, architecture: &str) -> String {
    let operating_system = match operating_system {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        value => value,
    };
    let architecture = match architecture {
        "x86_64" => "x64",
        "aarch64" => "ARM64",
        value => value,
    };
    format!("{operating_system} {architecture}")
}

/// 关于窗口仅允许加载桌面壳内置页面，外链必须交由系统浏览器处理。
pub fn is_about_dialog_navigation_allowed(target: &Url, shell_url: Option<&Url>) -> bool {
    let Some(shell_url) = shell_url else {
        return false;
    };
    let Ok(expected) = shell_url.join("about.html") else {
        return false;
    };
    target.scheme() == expected.scheme()
        && target.host_str() == expected.host_str()
        && target.port_or_known_default() == expected.port_or_known_default()
        && target.path() == expected.path()
        && target.query().is_none()
        && target.fragment().is_none()
}

#[cfg(test)]
#[path = "about_tests.rs"]
mod tests;
