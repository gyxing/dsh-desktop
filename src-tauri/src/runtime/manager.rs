use std::sync::{Arc, Mutex, MutexGuard};

use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_shell::{process::CommandChild, ShellExt};
use url::Url;

use super::{
    diagnostics::{DiagnosticBuffer, DiagnosticSource},
    events,
    paths::RuntimePaths,
    process_tree::ProcessTreeGuard,
    status::{RuntimeErrorCode, RuntimeStatus},
};

const MAX_DIAGNOSTIC_ENTRIES: usize = 200;
const MAX_DIAGNOSTIC_BYTES: usize = 256 * 1024;

struct RuntimeState {
    generation: u64,
    status: RuntimeStatus,
    child: Option<CommandChild>,
    process_tree: Option<ProcessTreeGuard>,
    shell_url: Option<Url>,
    runtime_url: Option<Url>,
    diagnostics: DiagnosticBuffer,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            generation: 0,
            status: RuntimeStatus::starting(),
            child: None,
            process_tree: None,
            shell_url: None,
            runtime_url: None,
            diagnostics: DiagnosticBuffer::new(MAX_DIAGNOSTIC_ENTRIES, MAX_DIAGNOSTIC_BYTES),
        }
    }

    fn transition_to_probing(&mut self, generation: u64, url: &Url) -> Option<RuntimeStatus> {
        if self.generation != generation || !matches!(self.status, RuntimeStatus::Starting { .. }) {
            return None;
        }
        self.runtime_url = Some(url.clone());
        self.status = RuntimeStatus::probing();
        Some(self.status.clone())
    }

    fn transition_to_loading(&mut self, generation: u64, url: &Url) -> Option<RuntimeStatus> {
        if self.generation != generation
            || !matches!(self.status, RuntimeStatus::Probing { .. })
            || self.runtime_url.as_ref() != Some(url)
        {
            return None;
        }
        self.status = RuntimeStatus::loading();
        Some(self.status.clone())
    }

    fn transition_to_ready(&mut self, generation: u64, page_url: &Url) -> Option<RuntimeStatus> {
        let runtime_url = self.runtime_url.as_ref()?;
        if self.generation != generation
            || !matches!(self.status, RuntimeStatus::Loading { .. })
            || !same_origin(runtime_url, page_url)
        {
            return None;
        }
        self.status = RuntimeStatus::Ready {
            message: "DeepSeek Harness 已就绪".to_string(),
            url: page_url.to_string(),
        };
        Some(self.status.clone())
    }
}

/// 统一管理内置 Node Sidecar 的启动、状态和退出回收。
pub struct RuntimeManager {
    state: Mutex<RuntimeState>,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(RuntimeState::new()),
        }
    }

    /// 返回可直接传递给前端的最新运行时状态。
    pub fn status(&self) -> RuntimeStatus {
        self.lock().status.clone()
    }

    /// 返回已在写入时脱敏的当前进程诊断快照。
    pub fn diagnostics(&self) -> String {
        self.lock().diagnostics.snapshot()
    }

    pub fn navigation_urls(&self) -> (Option<Url>, Option<Url>) {
        let state = self.lock();
        (state.shell_url.clone(), state.runtime_url.clone())
    }

    pub fn record_system_diagnostic(&self, message: &str) {
        let mut state = self.lock();
        let generation = state.generation;
        state
            .diagnostics
            .push(generation, DiagnosticSource::System, message);
    }

    pub(super) fn record_diagnostic(
        &self,
        generation: u64,
        source: DiagnosticSource,
        message: &str,
    ) {
        self.lock().diagnostics.push(generation, source, message);
    }

    /// 终止当前 Sidecar；应用退出时同步调用，避免遗留后台进程。
    pub fn stop(&self) {
        let (child, process_tree) = {
            let mut state = self.lock();
            state.generation = state.generation.wrapping_add(1);
            state.runtime_url = None;
            (state.child.take(), state.process_tree.take())
        };
        terminate_resources(child, process_tree);
    }

    /// 启动固定版本 DSH；不设置 DSH_HOME，也不读写其用户配置。
    pub fn start(self: &Arc<Self>, app: &AppHandle) -> Result<(), String> {
        let shell_url = resolve_shell_url(app);
        let (generation, old_child, old_tree, status) = {
            let mut state = self.lock();
            state.generation = state.generation.wrapping_add(1);
            let generation = state.generation;
            state.diagnostics.push(
                generation,
                DiagnosticSource::System,
                "开始启动 DeepSeek Harness",
            );
            state.shell_url = state.shell_url.take().or(shell_url);
            state.runtime_url = None;
            state.status = RuntimeStatus::starting();
            (
                state.generation,
                state.child.take(),
                state.process_tree.take(),
                state.status.clone(),
            )
        };

        terminate_resources(old_child, old_tree);
        emit_status(app, status);

        if let Err((code, error)) = self.launch(app, generation) {
            self.fail(
                app,
                generation,
                code,
                format!("启动 DeepSeek Harness 失败：{error}"),
                false,
            );
            return Err(error);
        }

        Ok(())
    }

    fn launch(
        self: &Arc<Self>,
        app: &AppHandle,
        generation: u64,
    ) -> Result<(), (RuntimeErrorCode, String)> {
        let paths = RuntimePaths::resolve(app)
            .map_err(|error| (RuntimeErrorCode::RuntimeMissing, error))?;
        let (receiver, child) = app
            .shell()
            .sidecar("node")
            .map_err(|error| {
                (
                    RuntimeErrorCode::SpawnFailed,
                    format!("无法创建 Node Sidecar：{error}"),
                )
            })?
            .arg(paths.dsh_entry)
            .args(["web", "--host", "127.0.0.1", "--port", "0"])
            .current_dir(paths.working_directory)
            .spawn()
            .map_err(|error| {
                (
                    RuntimeErrorCode::SpawnFailed,
                    format!("无法执行 Node Sidecar：{error}"),
                )
            })?;

        let process_tree = match ProcessTreeGuard::attach(child.pid()) {
            Ok(process_tree) => process_tree,
            Err(error) => {
                let _ = child.kill();
                return Err((
                    RuntimeErrorCode::ProcessTreeFailed,
                    format!("无法管理 Sidecar 进程树：{error}"),
                ));
            }
        };

        let mut state = self.lock();
        if state.generation != generation {
            drop(state);
            terminate_resources(Some(child), Some(process_tree));
            return Ok(());
        }
        state.child = Some(child);
        state.process_tree = Some(process_tree);
        drop(state);

        events::observe(self.clone(), app.clone(), generation, receiver);
        Ok(())
    }

    pub(super) fn mark_probing(&self, app: &AppHandle, generation: u64, url: &Url) -> bool {
        let status = {
            let mut state = self.lock();
            let Some(status) = state.transition_to_probing(generation, url) else {
                return false;
            };
            status
        };
        emit_status(app, status);
        true
    }

    pub(super) fn mark_loading(&self, app: &AppHandle, generation: u64, url: &Url) -> bool {
        let status = {
            let mut state = self.lock();
            let Some(status) = state.transition_to_loading(generation, url) else {
                return false;
            };
            status
        };
        emit_status(app, status);
        true
    }

    /// 仅允许当前加载批次的同源页面完成事件进入 ready。
    pub fn mark_page_ready(&self, app: &AppHandle, page_url: &Url) -> bool {
        let status = {
            let mut state = self.lock();
            let generation = state.generation;
            let Some(status) = state.transition_to_ready(generation, page_url) else {
                return false;
            };
            state.diagnostics.push(
                generation,
                DiagnosticSource::System,
                "DeepSeek Harness 页面加载完成",
            );
            status
        };
        emit_status(app, status);
        true
    }

    /// 将当前批次标记为失败，并同步回收其完整进程树。
    pub(super) fn fail(
        &self,
        app: &AppHandle,
        generation: u64,
        code: RuntimeErrorCode,
        message: String,
        only_if_starting: bool,
    ) -> bool {
        let (status, child, process_tree, shell_url) = {
            let mut state = self.lock();
            if state.generation != generation || (only_if_starting && !state.status.is_starting()) {
                return false;
            }
            state.diagnostics.push(
                generation,
                DiagnosticSource::System,
                &format!("运行时失败：{code:?}；{message}"),
            );
            let restore_shell = matches!(
                state.status,
                RuntimeStatus::Loading { .. } | RuntimeStatus::Ready { .. }
            );
            state.status = RuntimeStatus::failed(code, message);
            state.runtime_url = None;
            (
                state.status.clone(),
                state.child.take(),
                state.process_tree.take(),
                restore_shell.then(|| state.shell_url.clone()).flatten(),
            )
        };

        terminate_resources(child, process_tree);
        navigate_if_present(app, shell_url);
        emit_status(app, status);
        true
    }

    /// 处理 Sidecar 自行退出，并在需要时恢复本地启动页。
    pub(super) fn terminated(&self, app: &AppHandle, generation: u64, exit_code: Option<i32>) {
        let code = exit_code
            .map(|value| value.to_string())
            .unwrap_or_else(|| "未知".to_string());
        let (status, child, process_tree, shell_url) = {
            let mut state = self.lock();
            if state.generation != generation {
                return;
            }
            if !state.status.is_starting() && !state.status.is_ready() {
                return;
            }
            state.diagnostics.push(
                generation,
                DiagnosticSource::System,
                &format!("DeepSeek Harness 进程退出，代码 {code}"),
            );
            let was_ready = state.status.is_ready();
            let restore_shell = matches!(
                state.status,
                RuntimeStatus::Loading { .. } | RuntimeStatus::Ready { .. }
            );
            state.status = if was_ready {
                RuntimeStatus::Exited {
                    code: RuntimeErrorCode::ProcessExited,
                    message: format!("DeepSeek Harness 已退出（代码 {code}）"),
                }
            } else {
                RuntimeStatus::Failed {
                    code: RuntimeErrorCode::ProcessExited,
                    message: format!("DeepSeek Harness 启动失败（代码 {code}）"),
                }
            };
            state.runtime_url = None;
            (
                state.status.clone(),
                state.child.take(),
                state.process_tree.take(),
                restore_shell.then(|| state.shell_url.clone()).flatten(),
            )
        };

        terminate_resources(child, process_tree);
        navigate_if_present(app, shell_url);
        emit_status(app, status);
    }

    fn lock(&self) -> MutexGuard<'_, RuntimeState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn terminate_resources(child: Option<CommandChild>, process_tree: Option<ProcessTreeGuard>) {
    if let Some(child) = child {
        let _ = child.kill();
    }
    drop(process_tree);
}

fn emit_status(app: &AppHandle, status: RuntimeStatus) {
    crate::desktop::tray::update_runtime_status(app, &status);
    let _ = app.emit("runtime://status", status);
}

fn navigate_if_present(app: &AppHandle, url: Option<Url>) {
    if let (Some(window), Some(url)) = (app.get_webview_window("main"), url) {
        let _ = window.navigate(url);
    }
}

pub(crate) fn resolve_shell_url<R: Runtime>(_app: &AppHandle<R>) -> Option<Url> {
    #[cfg(dev)]
    if let Some(url) = _app.config().build.dev_url.as_ref() {
        return Some(url.clone());
    }

    // Windows 发布包使用 WRY 的本地协议兼容地址加载内置启动页。
    #[cfg(windows)]
    let value = "http://tauri.localhost/";
    #[cfg(not(windows))]
    let value = "tauri://localhost/";

    Url::parse(value).ok()
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::RuntimeState;
    use crate::runtime::status::RuntimeStatus;

    fn local_url(port: u16, path: &str) -> Url {
        Url::parse(&format!("http://127.0.0.1:{port}{path}")).expect("测试地址应有效")
    }

    #[test]
    fn current_generation_advances_through_probe_load_and_ready() {
        let mut state = RuntimeState::new();
        state.generation = 5;
        let runtime_url = local_url(45123, "/");

        assert!(state.transition_to_probing(5, &runtime_url).is_some());
        assert!(matches!(state.status, RuntimeStatus::Probing { .. }));
        assert!(state.transition_to_loading(5, &runtime_url).is_some());
        assert!(matches!(state.status, RuntimeStatus::Loading { .. }));
        assert!(state
            .transition_to_ready(5, &local_url(45123, "/conversation"))
            .is_some());
        assert!(matches!(state.status, RuntimeStatus::Ready { .. }));
    }

    #[test]
    fn old_generation_cannot_advance_the_current_runtime() {
        let mut state = RuntimeState::new();
        state.generation = 8;

        assert!(state
            .transition_to_probing(7, &local_url(45124, "/"))
            .is_none());
        assert!(matches!(state.status, RuntimeStatus::Starting { .. }));
    }

    #[test]
    fn page_from_another_origin_cannot_mark_runtime_ready() {
        let mut state = RuntimeState::new();
        state.generation = 9;
        let runtime_url = local_url(45125, "/");
        state.transition_to_probing(9, &runtime_url);
        state.transition_to_loading(9, &runtime_url);

        assert!(state
            .transition_to_ready(9, &local_url(45126, "/"))
            .is_none());
        assert!(matches!(state.status, RuntimeStatus::Loading { .. }));
    }
}
