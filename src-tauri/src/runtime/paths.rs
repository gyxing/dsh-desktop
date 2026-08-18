use std::path::PathBuf;

use tauri::{path::BaseDirectory, AppHandle, Manager};

const DSH_ENTRY: &str = "resources/dsh-runtime/node_modules/@deepseek-ai/dsh/lib/bin.js";
const PNPM_ENTRY: &str = "resources/dsh-runtime/node_modules/pnpm/bin/pnpm.cjs";

/// 保存 Sidecar 启动所需路径，不承载或改写任何 DSH 用户配置。
pub struct RuntimePaths {
    pub node_executable: PathBuf,
    pub dsh_entry: PathBuf,
    pub pnpm_entry: PathBuf,
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
        let node_executable = std::env::current_exe()
            .map_err(|error| format!("无法解析桌面程序路径：{error}"))?
            .parent()
            .map(|directory| directory.join("node.exe"))
            .ok_or_else(|| "桌面程序路径没有父目录".to_string())?;

        if !dsh_entry.is_file() {
            return Err("DSH 运行时文件不存在，请重新安装应用".to_string());
        }
        if !pnpm_entry.is_file() {
            return Err("pnpm 运行时文件不存在，请重新安装应用".to_string());
        }
        if !node_executable.is_file() {
            return Err("Node Sidecar 文件不存在，请重新安装应用".to_string());
        }

        let working_directory = app
            .path()
            .home_dir()
            .map_err(|error| format!("无法读取用户主目录：{error}"))?;

        Ok(Self {
            node_executable,
            dsh_entry,
            pnpm_entry,
            working_directory,
        })
    }
}
