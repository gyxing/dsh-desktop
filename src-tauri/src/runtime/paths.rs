use std::{env, path::PathBuf};

use tauri::{path::BaseDirectory, AppHandle, Manager};

use crate::platform;

use super::profile_compatibility::{mirage_bundle_enabled, resolve_dsh_home};

const DSH_ENTRY: &str = "resources/dsh-runtime/node_modules/@deepseek-ai/dsh/lib/bin.js";
const PNPM_ENTRY: &str = "resources/dsh-runtime/node_modules/pnpm/bin/pnpm.cjs";
const COMPATIBILITY_PATCH: &str = "resources/dsh-desktop/cordis.patch.yml";

/// 保存 Sidecar 启动所需路径；只读检查 Profile Bundle，不改写用户配置。
pub struct RuntimePaths {
    pub node_executable: PathBuf,
    pub dsh_entry: PathBuf,
    pub pnpm_entry: PathBuf,
    pub compatibility_patch: Option<PathBuf>,
    pub working_directory: PathBuf,
}

impl RuntimePaths {
    /// 从 Tauri 资源目录和当前用户主目录解析运行路径。
    pub fn resolve(app: &AppHandle) -> Result<Self, String> {
        let dsh_entry = app
            .path()
            .resolve(DSH_ENTRY, BaseDirectory::Resource)
            .map_err(|error| format!("无法解析 DSH 运行时：{error}"))?;
        // Node 24 无法把 Windows 扩展路径作为入口脚本，安全时退回普通磁盘路径。
        let dsh_entry = dunce::simplified(&dsh_entry).to_path_buf();
        let pnpm_entry = app
            .path()
            .resolve(PNPM_ENTRY, BaseDirectory::Resource)
            .map_err(|error| format!("无法解析 pnpm 运行时：{error}"))?;
        let pnpm_entry = dunce::simplified(&pnpm_entry).to_path_buf();
        let working_directory = app
            .path()
            .home_dir()
            .map_err(|error| format!("无法读取用户主目录：{error}"))?;
        let configured_home = env::var_os("DSH_HOME");
        let dsh_home = resolve_dsh_home(
            &working_directory,
            &working_directory,
            configured_home.as_deref(),
        );
        let web_profile = dsh_home.join("profiles").join("web");
        let compatibility_patch = if mirage_bundle_enabled(&web_profile)? {
            let path = app
                .path()
                .resolve(COMPATIBILITY_PATCH, BaseDirectory::Resource)
                .map_err(|error| format!("无法解析 Desktop 兼容补丁：{error}"))?;
            Some(dunce::simplified(&path).to_path_buf())
        } else {
            None
        };
        let node_executable = platform::resolve_node_executable()?;

        if !dsh_entry.is_file() {
            return Err("DSH 运行时文件不存在，请重新安装应用".to_string());
        }
        if !pnpm_entry.is_file() {
            return Err("pnpm 运行时文件不存在，请重新安装应用".to_string());
        }
        if !node_executable.is_file() {
            return Err("Node Sidecar 文件不存在，请重新安装应用".to_string());
        }
        if compatibility_patch
            .as_ref()
            .is_some_and(|path| !path.is_file())
        {
            return Err("Desktop 兼容补丁不存在，请重新安装应用".to_string());
        }

        Ok(Self {
            node_executable,
            dsh_entry,
            pnpm_entry,
            compatibility_patch,
            working_directory,
        })
    }
}
