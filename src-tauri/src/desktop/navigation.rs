use url::Url;

use std::sync::Arc;

use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};
use tauri_plugin_shell::ShellExt;

use crate::runtime::manager::{resolve_shell_url, RuntimeManager};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationDecision {
    Allow,
    OpenExternal,
    Deny,
}

/// 把窗口内同源导航、系统外链和危险目标分开，避免本机其他服务被误认为 DSH。
pub fn decide_navigation(
    target: &Url,
    shell_url: Option<&Url>,
    runtime_url: Option<&Url>,
) -> NavigationDecision {
    if shell_url.is_some_and(|allowed| same_origin(allowed, target))
        || runtime_url.is_some_and(|allowed| same_origin(allowed, target))
    {
        return NavigationDecision::Allow;
    }

    match target.scheme() {
        "mailto" => NavigationDecision::OpenExternal,
        "http" | "https" if is_loopback_host(target.host_str()) => NavigationDecision::Deny,
        "http" | "https" => NavigationDecision::OpenExternal,
        _ => NavigationDecision::Deny,
    }
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(host, Some("127.0.0.1" | "localhost" | "::1"))
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::<R>::new("navigation-policy")
        .on_navigation(|webview, target| {
            if webview.label() == crate::desktop::about::ABOUT_DIALOG_LABEL {
                let shell_url = resolve_shell_url(webview.app_handle());
                return crate::desktop::about::is_about_dialog_navigation_allowed(
                    target,
                    shell_url.as_ref(),
                );
            }
            if webview.label() == crate::desktop::chrome::WINDOW_CHROME_LABEL {
                let shell_url = resolve_shell_url(webview.app_handle());
                return crate::desktop::chrome::is_chrome_navigation_allowed(
                    target,
                    shell_url.as_ref(),
                );
            }
            if webview.label() == crate::updater::dialog::UPDATE_DIALOG_LABEL {
                let shell_url = resolve_shell_url(webview.app_handle());
                return crate::updater::dialog::is_update_dialog_navigation_allowed(
                    target,
                    shell_url.as_ref(),
                );
            }
            if webview.label() != "main" {
                return false;
            }
            let app = webview.app_handle();
            let manager = app.state::<Arc<RuntimeManager>>();
            let (managed_shell_url, runtime_url) = manager.navigation_urls();
            let shell_url = managed_shell_url.or_else(|| resolve_shell_url(app));

            match decide_navigation(target, shell_url.as_ref(), runtime_url.as_ref()) {
                NavigationDecision::Allow => true,
                NavigationDecision::OpenExternal => {
                    if let Err(error) = open_external(app, target) {
                        manager.record_system_diagnostic(&format!(
                            "无法使用系统默认应用打开外部链接：{error}"
                        ));
                    }
                    false
                }
                NavigationDecision::Deny => {
                    manager.record_system_diagnostic(&format!(
                        "已拒绝窗口导航：scheme={} host={}",
                        target.scheme(),
                        target.host_str().unwrap_or("<none>")
                    ));
                    false
                }
            }
        })
        .build()
}

#[allow(deprecated)]
pub(crate) fn open_external<R: Runtime>(
    app: &tauri::AppHandle<R>,
    target: &Url,
) -> Result<(), String> {
    app.shell()
        .open(target.as_str(), None)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::{decide_navigation, NavigationDecision};

    fn url(value: &str) -> Url {
        Url::parse(value).expect("测试地址应有效")
    }

    #[test]
    fn navigation_allows_the_shell_and_current_runtime_origins() {
        let shell = url("http://tauri.localhost/");
        let runtime = url("http://127.0.0.1:45130/");

        assert_eq!(
            decide_navigation(
                &url("http://tauri.localhost/assets/app.js"),
                Some(&shell),
                None
            ),
            NavigationDecision::Allow
        );
        assert_eq!(
            decide_navigation(
                &url("http://127.0.0.1:45130/conversation"),
                Some(&shell),
                Some(&runtime)
            ),
            NavigationDecision::Allow
        );
    }

    #[test]
    fn navigation_opens_normal_web_and_mail_links_externally() {
        let shell = url("http://tauri.localhost/");

        assert_eq!(
            decide_navigation(&url("https://example.com/help"), Some(&shell), None),
            NavigationDecision::OpenExternal
        );
        assert_eq!(
            decide_navigation(&url("mailto:support@example.com"), Some(&shell), None),
            NavigationDecision::OpenExternal
        );
    }

    #[test]
    fn navigation_denies_other_loopback_ports_and_unsafe_schemes() {
        let shell = url("http://tauri.localhost/");
        let runtime = url("http://127.0.0.1:45131/");

        for target in [
            "http://127.0.0.1:45132/",
            "http://localhost:45131/",
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
        ] {
            assert_eq!(
                decide_navigation(&url(target), Some(&shell), Some(&runtime)),
                NavigationDecision::Deny,
                "应拒绝 {target}"
            );
        }
    }
}
