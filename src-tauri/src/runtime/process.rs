use std::{ffi::OsString, path::Path, process::Stdio};

use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
};

use super::paths::RuntimePaths;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 生成 Desktop Sidecar 参数；最终补丁位于 Profile 层之后，只约束桌面运行时。
fn desktop_launch_arguments(compatibility_patch: Option<&Path>) -> Vec<OsString> {
    let mut arguments = vec![OsString::from("web")];
    if let Some(compatibility_patch) = compatibility_patch {
        arguments.extend([
            OsString::from("--patch"),
            compatibility_patch.as_os_str().to_owned(),
        ]);
    }
    arguments.extend([
        OsString::from("--host"),
        OsString::from("127.0.0.1"),
        OsString::from("--port"),
        OsString::from("0"),
    ]);
    arguments
}

/// Sidecar输出与退出事件，保持运行时状态机不依赖具体进程库。
#[derive(Debug)]
pub enum RuntimeEvent {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Error(String),
    Terminated { code: Option<i32> },
}

/// 保存Sidecar PID与终止通道；真实Child由异步观察任务独占。
pub struct ManagedChild {
    pid: u32,
    terminate: UnboundedSender<()>,
}

impl ManagedChild {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// 请求终止根进程；完整进程树由ProcessTreeGuard负责。
    pub fn kill(self) {
        let _ = self.terminate.send(());
    }
}

/// 使用打包Node启动DSH，并返回平台中立的事件接收器。
pub fn spawn(
    paths: &RuntimePaths,
) -> Result<(UnboundedReceiver<RuntimeEvent>, ManagedChild), String> {
    let mut command = Command::new(&paths.node_executable);
    command
        .arg(&paths.dsh_entry)
        .args(desktop_launch_arguments(
            paths.compatibility_patch.as_deref(),
        ))
        .current_dir(&paths.working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command
        .spawn()
        .map_err(|error| format!("无法执行Node Sidecar：{error}"))?;
    let pid = child
        .id()
        .ok_or_else(|| "Node Sidecar没有进程ID".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法读取Node Sidecar标准输出".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "无法读取Node Sidecar错误输出".to_string())?;
    let (event_sender, event_receiver) = mpsc::unbounded_channel();
    let (terminate_sender, mut terminate_receiver) = mpsc::unbounded_channel();

    tauri::async_runtime::spawn(read_lines(stdout, event_sender.clone(), false));
    tauri::async_runtime::spawn(read_lines(stderr, event_sender.clone(), true));
    tauri::async_runtime::spawn(async move {
        let status = tokio::select! {
            status = child.wait() => status,
            request = terminate_receiver.recv() => {
                if request.is_some() {
                    let _ = child.start_kill();
                }
                child.wait().await
            }
        };
        match status {
            Ok(status) => {
                let code = status.code();
                let _ = event_sender.send(RuntimeEvent::Terminated { code });
            }
            Err(error) => {
                let _ = event_sender.send(RuntimeEvent::Error(error.to_string()));
            }
        }
    });

    Ok((
        event_receiver,
        ManagedChild {
            pid,
            terminate: terminate_sender,
        },
    ))
}

async fn read_lines<R>(reader: R, sender: UnboundedSender<RuntimeEvent>, stderr: bool)
where
    R: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line).await {
            Ok(0) => break,
            Ok(_) => {
                while matches!(line.last(), Some(b'\n' | b'\r')) {
                    line.pop();
                }
                let event = if stderr {
                    RuntimeEvent::Stderr(line.clone())
                } else {
                    RuntimeEvent::Stdout(line.clone())
                };
                if sender.send(event).is_err() {
                    break;
                }
            }
            Err(error) => {
                let _ = sender.send(RuntimeEvent::Error(error.to_string()));
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::Path};

    use super::desktop_launch_arguments;

    #[test]
    fn desktop_compatibility_patch_is_applied_after_profile_layers() {
        let patch = Path::new("resources/dsh-desktop/cordis.patch.yml");

        assert_eq!(
            desktop_launch_arguments(Some(patch)),
            vec![
                OsString::from("web"),
                OsString::from("--patch"),
                patch.as_os_str().to_owned(),
                OsString::from("--host"),
                OsString::from("127.0.0.1"),
                OsString::from("--port"),
                OsString::from("0"),
            ]
        );
    }

    #[test]
    fn native_profile_launches_without_a_compatibility_patch() {
        assert_eq!(
            desktop_launch_arguments(None),
            vec![
                OsString::from("web"),
                OsString::from("--host"),
                OsString::from("127.0.0.1"),
                OsString::from("--port"),
                OsString::from("0"),
            ]
        );
    }
}
