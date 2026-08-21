use tauri::WebviewUrl;
use url::Url;

use super::{
    calculate_chrome_layout, is_chrome_navigation_allowed, is_trusted_chrome_label,
    resolve_chrome_url,
};

#[test]
fn chrome_layout_reserves_one_scaled_row_for_the_titlebar() {
    let layout = calculate_chrome_layout(1180, 760, 1.25);

    assert_eq!(layout.chrome_height, 45);
    assert_eq!(layout.content_y, 45);
    assert_eq!(layout.content_height, 715);
    assert_eq!(layout.width, 1180);
}

#[test]
fn chrome_layout_never_underflows_a_tiny_window() {
    let layout = calculate_chrome_layout(320, 20, 1.0);

    assert_eq!(layout.chrome_height, 20);
    assert_eq!(layout.content_height, 0);
}

#[test]
fn only_the_local_chrome_webview_can_request_window_actions() {
    assert!(is_trusted_chrome_label("window-chrome"));
    assert!(!is_trusted_chrome_label("main"));
    assert!(!is_trusted_chrome_label("update-dialog"));
}

#[test]
fn development_chrome_uses_the_vite_titlebar_page() {
    let dev_url = Url::parse("http://127.0.0.1:1420/").expect("测试地址应有效");

    let resolved = resolve_chrome_url(Some(&dev_url));

    assert!(matches!(
        resolved,
        WebviewUrl::External(url) if url.as_str() == "http://127.0.0.1:1420/titlebar.html"
    ));
}

#[test]
fn chrome_navigation_allows_only_the_local_titlebar_page() {
    let shell = Url::parse("http://127.0.0.1:1420/").expect("测试地址应有效");

    assert!(is_chrome_navigation_allowed(
        &Url::parse("http://127.0.0.1:1420/titlebar.html").expect("测试地址应有效"),
        Some(&shell),
    ));
    assert!(!is_chrome_navigation_allowed(
        &Url::parse("http://127.0.0.1:1420/").expect("测试地址应有效"),
        Some(&shell),
    ));
    assert!(!is_chrome_navigation_allowed(
        &Url::parse("https://example.com/titlebar.html").expect("测试地址应有效"),
        Some(&shell),
    ));
}
