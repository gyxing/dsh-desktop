use url::Url;

use super::{
    is_about_dialog_navigation_allowed, platform_label, resolve_build_id,
    resolve_build_timestamp_ms,
};

fn url(value: &str) -> Url {
    Url::parse(value).expect("测试地址应有效")
}

#[test]
fn platform_label_uses_public_cross_platform_names() {
    assert_eq!(platform_label("windows", "x86_64"), "Windows x64");
    assert_eq!(platform_label("macos", "aarch64"), "macOS ARM64");
    assert_eq!(platform_label("linux", "x86_64"), "Linux x64");
    assert_eq!(platform_label("freebsd", "riscv64"), "freebsd riscv64");
}

#[test]
fn about_dialog_only_allows_its_local_document() {
    let shell = url("http://tauri.localhost/");

    assert!(is_about_dialog_navigation_allowed(
        &url("http://tauri.localhost/about.html"),
        Some(&shell),
    ));
    for target in [
        "http://tauri.localhost/update.html",
        "http://tauri.localhost/about.html?source=external",
        "https://github.com/gyxing/dsh-desktop",
    ] {
        assert!(!is_about_dialog_navigation_allowed(
            &url(target),
            Some(&shell),
        ));
    }
}

#[test]
fn release_build_metadata_prefers_ci_values_and_has_local_fallbacks() {
    assert_eq!(
        resolve_build_timestamp_ms(Some("1787319300"), 42),
        1_787_319_300_000
    );
    assert_eq!(resolve_build_timestamp_ms(Some("invalid"), 42), 42);
    assert_eq!(
        resolve_build_id(Some("release-13"), Some("abcdef012345"), false),
        "release-13"
    );
    assert_eq!(
        resolve_build_id(None, Some("abcdef012345"), false),
        "abcdef0"
    );
    assert_eq!(resolve_build_id(None, None, true), "local-debug");
}
