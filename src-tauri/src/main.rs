#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod desktop;
mod platform;
mod runtime;
mod updater;

#[cfg(windows)]
fn configure_webview2_gpu_compatibility() {
    const ARGUMENTS_ENV: &str = "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS";
    const DISABLE_GPU_ARGUMENT: &str = "--disable-gpu";

    let existing_arguments = std::env::var(ARGUMENTS_ENV).unwrap_or_default();
    let existing_arguments = existing_arguments.trim();
    let browser_arguments = if existing_arguments
        .split_whitespace()
        .any(|argument| argument == DISABLE_GPU_ARGUMENT)
    {
        existing_arguments.to_owned()
    } else if existing_arguments.is_empty() {
        DISABLE_GPU_ARGUMENT.to_owned()
    } else {
        format!("{existing_arguments} {DISABLE_GPU_ARGUMENT}")
    };

    // 保留调用方诊断参数，并确保主界面和所有子 WebView 使用同一套 GPU 兼容设置。
    std::env::set_var(ARGUMENTS_ENV, browser_arguments);
}

fn main() {
    #[cfg(windows)]
    configure_webview2_gpu_compatibility();

    app::run();
}
