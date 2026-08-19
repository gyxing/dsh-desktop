use std::{sync::Arc, time::Duration};

use tauri::{AppHandle, Manager};
use tokio::sync::mpsc::UnboundedReceiver;

use super::{
    diagnostics::DiagnosticSource, health::probe_http, manager::RuntimeManager,
    process::RuntimeEvent, readiness::parse_readiness, status::RuntimeErrorCode,
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const HTTP_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// 监听 Sidecar 输出与退出事件，并为当前启动批次设置超时保护。
pub fn observe(
    manager: Arc<RuntimeManager>,
    app: AppHandle,
    generation: u64,
    mut receiver: UnboundedReceiver<RuntimeEvent>,
) {
    let event_manager = manager.clone();
    let event_app = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = receiver.recv().await {
            if !handle_event(&event_manager, &event_app, generation, event) {
                break;
            }
        }
    });

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_TIMEOUT).await;
        manager.fail(
            &app,
            generation,
            RuntimeErrorCode::StartupTimeout,
            "DeepSeek Harness 启动超时，请重新启动".to_string(),
            true,
        );
    });
}

fn handle_event(
    manager: &Arc<RuntimeManager>,
    app: &AppHandle,
    generation: u64,
    event: RuntimeEvent,
) -> bool {
    match event {
        RuntimeEvent::Stdout(line) => handle_stdout(manager, app, generation, &line),
        RuntimeEvent::Error(error) => {
            manager.fail(
                app,
                generation,
                RuntimeErrorCode::RuntimeCommunication,
                format!("DeepSeek Harness 进程通信失败：{error}"),
                false,
            );
            false
        }
        RuntimeEvent::Terminated { code } => {
            manager.terminated(app, generation, code);
            false
        }
        RuntimeEvent::Stderr(line) => {
            manager.record_diagnostic(
                generation,
                DiagnosticSource::Stderr,
                &String::from_utf8_lossy(&line),
            );
            true
        }
    }
}

fn handle_stdout(
    manager: &Arc<RuntimeManager>,
    app: &AppHandle,
    generation: u64,
    line: &[u8],
) -> bool {
    let output = String::from_utf8_lossy(line);
    let url = match parse_readiness(&output) {
        Ok(Some(url)) => url,
        Ok(None) => return true,
        Err(error) => {
            manager.fail(
                app,
                generation,
                RuntimeErrorCode::ReadinessInvalid,
                error.to_string(),
                true,
            );
            return false;
        }
    };

    if !manager.mark_probing(app, generation, &url) {
        return true;
    }

    let probe_manager = manager.clone();
    let probe_app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = probe_http(&url, HTTP_PROBE_TIMEOUT).await {
            probe_manager.fail(
                &probe_app,
                generation,
                RuntimeErrorCode::HttpUnreachable,
                format!("DeepSeek Harness 服务检查失败：{error}"),
                true,
            );
            return;
        }
        if !probe_manager.mark_loading(&probe_app, generation, &url) {
            return;
        }
        let navigation = probe_app
            .get_webview_window("main")
            .ok_or_else(|| "主窗口不存在".to_string())
            .and_then(|window| {
                window
                    .navigate(url)
                    .map_err(|error| format!("无法打开 DSH Web 界面：{error}"))
            });
        if let Err(error) = navigation {
            probe_manager.fail(
                &probe_app,
                generation,
                RuntimeErrorCode::PageLoadFailed,
                error,
                false,
            );
        }
    });

    true
}
