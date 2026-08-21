use tauri::WebviewUrl;
use url::Url;

use super::{
    is_update_dialog_navigation_allowed, resolve_update_dialog_url, UpdateDialogManager,
    UpdateDialogPayload,
};

#[tokio::test]
async fn confirmation_response_completes_the_waiting_update_task() {
    let manager = UpdateDialogManager::new();
    let receiver = manager.begin_confirmation(UpdateDialogPayload {
        version: "0.1.3".to_string(),
        notes: "本版更新".to_string(),
        confirmation: true,
    });

    assert!(manager.respond(true));
    assert!(receiver.await.expect("确认通道应返回结果"));
    assert!(!manager.respond(true), "重复响应必须保持幂等");
}

#[test]
fn development_dialog_uses_the_vite_server_update_page() {
    let dev_url = Url::parse("http://127.0.0.1:1420/").expect("测试地址应有效");

    let resolved = resolve_update_dialog_url(Some(&dev_url));

    assert!(matches!(
        resolved,
        WebviewUrl::External(url) if url.as_str() == "http://127.0.0.1:1420/update.html"
    ));
}

#[test]
fn update_dialog_navigation_allows_only_its_local_page() {
    let shell = Url::parse("http://127.0.0.1:1420/").expect("测试地址应有效");

    assert!(is_update_dialog_navigation_allowed(
        &Url::parse("http://127.0.0.1:1420/update.html").expect("测试地址应有效"),
        Some(&shell),
    ));
    assert!(!is_update_dialog_navigation_allowed(
        &Url::parse("http://127.0.0.1:1420/").expect("测试地址应有效"),
        Some(&shell),
    ));
    assert!(!is_update_dialog_navigation_allowed(
        &Url::parse("https://example.com/update.html").expect("测试地址应有效"),
        Some(&shell),
    ));
}
