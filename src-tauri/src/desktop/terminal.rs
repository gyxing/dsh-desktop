use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use serde::Deserialize;
use tauri::{AppHandle, Manager};
use thiserror::Error;

use crate::runtime::paths::RuntimePaths;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;

const NODE_SHIM: &str = "@echo off\r\n\"%DSH_DESKTOP_NODE%\" %*\r\nexit /b %errorlevel%\r\n";
const DSH_SHIM: &str = "@echo off\r\n\"%DSH_DESKTOP_NODE%\" \"%DSH_DESKTOP_DSH_ENTRY%\" %*\r\nexit /b %errorlevel%\r\n";
const PNPM_SHIM: &str = "@echo off\r\n\"%DSH_DESKTOP_NODE%\" \"%DSH_DESKTOP_PNPM_ENTRY%\" %*\r\nexit /b %errorlevel%\r\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerShellKind {
    Pwsh,
    WindowsPowerShell,
}

#[derive(Debug)]
pub struct PowerShellExecutable {
    pub path: PathBuf,
    pub kind: PowerShellKind,
}

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("未找到可用的 PowerShell 7 或 Windows PowerShell 5.1")]
    PowerShellUnavailable,
    #[cfg(not(windows))]
    #[error("未找到可用的系统终端：{0}")]
    SystemTerminalUnavailable(String),
    #[error("无法读取桌面运行时：{0}")]
    Runtime(String),
    #[error("无法读取固定版本信息：{0}")]
    Versions(#[from] serde_json::Error),
    #[error("无法构造 DSH 终端 PATH：{0}")]
    Path(#[from] std::env::JoinPathsError),
    #[error("无法准备 DSH 终端：{0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeLock {
    pnpm_version: String,
    node: LockedVersion,
    dsh: LockedVersion,
}

#[derive(Debug, Deserialize)]
struct LockedVersion {
    version: String,
}

#[derive(Debug)]
pub struct RuntimeVersions {
    pub node: String,
    pub dsh: String,
    pub pnpm: String,
}

pub fn runtime_versions() -> Result<RuntimeVersions, TerminalError> {
    let lock: RuntimeLock =
        serde_json::from_str(include_str!("../../../runtime/runtime-lock.json"))?;
    Ok(RuntimeVersions {
        node: lock.node.version,
        dsh: lock.dsh.version,
        pnpm: lock.pnpm_version,
    })
}

pub fn select_powershell<F>(mut resolve: F) -> Result<PowerShellExecutable, TerminalError>
where
    F: FnMut(&str) -> Option<PathBuf>,
{
    if let Some(path) = resolve("pwsh.exe") {
        return Ok(PowerShellExecutable {
            path,
            kind: PowerShellKind::Pwsh,
        });
    }
    if let Some(path) = resolve("powershell.exe") {
        return Ok(PowerShellExecutable {
            path,
            kind: PowerShellKind::WindowsPowerShell,
        });
    }
    Err(TerminalError::PowerShellUnavailable)
}

/// 只生成批处理 shim，避免 PowerShell 执行策略优先命中同名 `.ps1`。
pub fn write_command_shims(directory: &Path) -> Result<(), TerminalError> {
    fs::create_dir_all(directory)?;
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(TerminalError::Io(std::io::Error::other(
            "终端 shim 路径不是普通目录",
        )));
    }

    for (name, contents) in [
        ("node.cmd", NODE_SHIM),
        ("dsh.cmd", DSH_SHIM),
        ("pnpm.cmd", PNPM_SHIM),
    ] {
        write_if_changed(&directory.join(name), contents)?;
    }
    Ok(())
}

fn write_if_changed(path: &Path, contents: &str) -> Result<(), std::io::Error> {
    if fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }

    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| std::io::Error::other("shim 文件名无效"))?;
    let temporary = path.with_file_name(format!(".{filename}.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&temporary);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)
}

pub fn powershell_welcome_script(kind: PowerShellKind) -> String {
    let compatibility_notice = match kind {
        PowerShellKind::Pwsh => "PowerShell 7",
        PowerShellKind::WindowsPowerShell => "Windows PowerShell 5.1 兼容模式",
    };
    format!(
        r#"$dshDesktopShimDir = $env:DSH_DESKTOP_SHIM_DIR
$dshDesktopPath = @($env:Path -split ';' | Where-Object {{ -not [string]::Equals($_, $dshDesktopShimDir, [StringComparison]::OrdinalIgnoreCase) }})
$env:Path = (@($dshDesktopShimDir) + $dshDesktopPath) -join ';'
Set-Location -LiteralPath $env:DSH_DESKTOP_WORKING_DIRECTORY
Write-Host ('DSH Desktop {{0}} PowerShell' -f $env:DSH_DESKTOP_VERSION)
Write-Host 'Shell: {compatibility_notice}'
Write-Host ('Node: {{0}}' -f $env:DSH_DESKTOP_NODE_VERSION)
Write-Host ('DSH: {{0}}' -f $env:DSH_DESKTOP_DSH_VERSION)
Write-Host ('pnpm: {{0}}' -f $env:DSH_DESKTOP_PNPM_VERSION)
Write-Host ('Working directory: {{0}}' -f $env:DSH_DESKTOP_WORKING_DIRECTORY)
Write-Host 'Commands: dsh --dump-config | dsh plugin --help | pnpm --version'
"#,
    )
}

/// 按当前平台打开独立DSH终端，不修改系统或用户配置。
pub fn open_dsh_terminal(app: &AppHandle) -> Result<(), TerminalError> {
    crate::platform::open_external_terminal(app)
}
/// 打开独立 PowerShell；只给该子进程注入随包命令，不改变系统或用户配置。
pub fn open_dsh_powershell(app: &AppHandle) -> Result<(), TerminalError> {
    let runtime = RuntimePaths::resolve(app).map_err(TerminalError::Runtime)?;
    let versions = runtime_versions()?;
    let shim_directory = app
        .path()
        .app_local_data_dir()
        .map_err(|error| TerminalError::Runtime(error.to_string()))?
        .join("terminal")
        .join("bin");
    write_command_shims(&shim_directory)?;

    let powershell = select_powershell(resolve_powershell_executable)?;
    let existing_path = env::var_os("PATH").unwrap_or_default();
    let terminal_path = env::join_paths(
        std::iter::once(shim_directory.clone()).chain(env::split_paths(&existing_path)),
    )?;
    let script = powershell_welcome_script(powershell.kind);
    let mut command = Command::new(&powershell.path);
    command
        .args(["-NoLogo", "-NoExit", "-NoProfile", "-Command", &script])
        .current_dir(&runtime.working_directory)
        .env("PATH", terminal_path)
        .env("DSH_DESKTOP_SHIM_DIR", &shim_directory)
        .env("DSH_DESKTOP_WORKING_DIRECTORY", &runtime.working_directory)
        .env("DSH_DESKTOP_NODE", &runtime.node_executable)
        .env("DSH_DESKTOP_DSH_ENTRY", &runtime.dsh_entry)
        .env("DSH_DESKTOP_PNPM_ENTRY", &runtime.pnpm_entry)
        .env("DSH_DESKTOP_VERSION", env!("CARGO_PKG_VERSION"))
        .env("DSH_DESKTOP_NODE_VERSION", versions.node)
        .env("DSH_DESKTOP_DSH_VERSION", versions.dsh)
        .env("DSH_DESKTOP_PNPM_VERSION", versions.pnpm);
    #[cfg(windows)]
    command.creation_flags(CREATE_NEW_CONSOLE);
    command.spawn()?;
    Ok(())
}

fn resolve_powershell_executable(name: &str) -> Option<PathBuf> {
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let known_path = match name {
        "pwsh.exe" => env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .map(|path| path.join("PowerShell").join("7").join(name)),
        "powershell.exe" => env::var_os("SystemRoot").map(PathBuf::from).map(|path| {
            path.join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join(name)
        }),
        _ => None,
    }?;
    known_path.is_file().then_some(known_path)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::{
        powershell_welcome_script, runtime_versions, select_powershell, write_command_shims,
        PowerShellKind,
    };

    fn test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dsh-desktop-terminal-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn terminal_prefers_pwsh_and_falls_back_to_windows_powershell() {
        let pwsh = PathBuf::from(r"C:\Program Files\PowerShell\7\pwsh.exe");
        let legacy = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");

        let preferred = select_powershell(|name| match name {
            "pwsh.exe" => Some(pwsh.clone()),
            "powershell.exe" => Some(legacy.clone()),
            _ => None,
        })
        .expect("应找到 PowerShell 7");
        assert_eq!(preferred.kind, PowerShellKind::Pwsh);
        assert_eq!(preferred.path, pwsh);

        let fallback = select_powershell(|name| match name {
            "powershell.exe" => Some(legacy.clone()),
            _ => None,
        })
        .expect("应回退 Windows PowerShell");
        assert_eq!(fallback.kind, PowerShellKind::WindowsPowerShell);
        assert_eq!(fallback.path, legacy);
    }

    #[test]
    fn terminal_reports_when_no_powershell_is_available() {
        let error = select_powershell(|_| None).expect_err("缺少 PowerShell 时必须失败");

        assert!(error.to_string().contains("PowerShell"));
    }

    #[test]
    fn terminal_writes_only_cmd_shims_that_use_process_environment_paths() {
        let directory = test_directory("shims");
        let _ = fs::remove_dir_all(&directory);

        write_command_shims(&directory).expect("应能创建私有命令 shim");

        let names = fs::read_dir(&directory)
            .expect("应能读取 shim 目录")
            .map(|entry| {
                entry
                    .expect("目录项应有效")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 3);
        for name in ["node.cmd", "dsh.cmd", "pnpm.cmd"] {
            assert!(names.iter().any(|candidate| candidate == name));
        }
        assert!(fs::read_to_string(directory.join("node.cmd"))
            .expect("node shim 应可读")
            .contains("%DSH_DESKTOP_NODE%"));
        assert!(fs::read_to_string(directory.join("dsh.cmd"))
            .expect("dsh shim 应可读")
            .contains("%DSH_DESKTOP_DSH_ENTRY%"));
        assert!(fs::read_to_string(directory.join("pnpm.cmd"))
            .expect("pnpm shim 应可读")
            .contains("%DSH_DESKTOP_PNPM_ENTRY%"));

        fs::remove_dir_all(directory).expect("应清理测试目录");
    }

    #[test]
    fn windows_powershell_welcome_explicitly_reports_compatibility_mode() {
        let script = powershell_welcome_script(PowerShellKind::WindowsPowerShell);

        assert!(script.contains("Windows PowerShell 5.1 兼容模式"));
        assert!(script.contains("DSH_DESKTOP_SHIM_DIR"));
        assert!(script.contains("DSH_DESKTOP_WORKING_DIRECTORY"));
    }

    #[test]
    fn terminal_versions_come_from_the_approved_runtime_lock() {
        let versions = runtime_versions().expect("运行时锁应能解析");

        assert_eq!(versions.node, "24.19.0");
        assert_eq!(versions.dsh, "0.1.0-rc.7");
        assert_eq!(versions.pnpm, "11.22.0");
    }
}
