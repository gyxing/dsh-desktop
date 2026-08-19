use std::{
    ffi::OsStr,
    fs::{self, OpenOptions, Permissions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use tauri::{AppHandle, Manager};

use crate::{
    desktop::terminal::{runtime_versions, TerminalError},
    runtime::paths::RuntimePaths,
};

const NODE_SHIM: &str = "#!/bin/sh\nexec \"$DSH_DESKTOP_NODE\" \"$@\"\n";
const DSH_SHIM: &str = "#!/bin/sh\nexec \"$DSH_DESKTOP_NODE\" \"$DSH_DESKTOP_DSH_ENTRY\" \"$@\"\n";
const PNPM_SHIM: &str =
    "#!/bin/sh\nexec \"$DSH_DESKTOP_NODE\" \"$DSH_DESKTOP_PNPM_ENTRY\" \"$@\"\n";

/// 准备Unix命令shim和启动脚本，只影响应用私有目录。
pub fn prepare_launcher(app: &AppHandle) -> Result<PathBuf, TerminalError> {
    let runtime = RuntimePaths::resolve(app).map_err(TerminalError::Runtime)?;
    let versions = runtime_versions()?;
    let terminal_directory = app
        .path()
        .app_local_data_dir()
        .map_err(|error| TerminalError::Runtime(error.to_string()))?
        .join("terminal");
    let shim_directory = terminal_directory.join("bin");
    ensure_directory(&shim_directory)?;
    for (name, contents) in [("node", NODE_SHIM), ("dsh", DSH_SHIM), ("pnpm", PNPM_SHIM)] {
        write_executable(&shim_directory.join(name), contents)?;
    }

    let launcher = terminal_directory.join("launch-dsh.sh");
    let script = format!(
        r#"#!/bin/sh
export DSH_DESKTOP_NODE={node}
export DSH_DESKTOP_DSH_ENTRY={dsh}
export DSH_DESKTOP_PNPM_ENTRY={pnpm}
export PATH={shim}:"$PATH"
cd {working}
printf '%s\n' 'DSH Desktop {desktop_version} terminal'
printf '%s\n' 'Node: {node_version}' 'DSH: {dsh_version}' 'pnpm: {pnpm_version}'
exec "${{SHELL:-/bin/sh}}" -l
"#,
        node = shell_quote(runtime.node_executable.as_os_str()),
        dsh = shell_quote(runtime.dsh_entry.as_os_str()),
        pnpm = shell_quote(runtime.pnpm_entry.as_os_str()),
        shim = shell_quote(shim_directory.as_os_str()),
        working = shell_quote(runtime.working_directory.as_os_str()),
        desktop_version = env!("CARGO_PKG_VERSION"),
        node_version = versions.node,
        dsh_version = versions.dsh,
        pnpm_version = versions.pnpm,
    );
    write_executable(&launcher, &script)?;
    Ok(launcher)
}
fn ensure_directory(directory: &Path) -> Result<(), TerminalError> {
    fs::create_dir_all(directory)?;
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(TerminalError::Io(std::io::Error::other(
            "终端shim路径不是普通目录",
        )));
    }
    fs::set_permissions(directory, Permissions::from_mode(0o700))?;
    Ok(())
}

fn write_executable(path: &Path, contents: &str) -> Result<(), TerminalError> {
    if fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        fs::set_permissions(path, Permissions::from_mode(0o700))?;
        return Ok(());
    }
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| std::io::Error::other("终端脚本文件名无效"))?;
    let temporary = path.with_file_name(format!(".{filename}.{}.tmp", std::process::id()));
    let _ = fs::remove_file(&temporary);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);
    fs::set_permissions(&temporary, Permissions::from_mode(0o700))?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

fn shell_quote(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}
