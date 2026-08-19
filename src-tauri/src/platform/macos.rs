use std::process::Command;

use tauri::AppHandle;

use super::unix_terminal;
use crate::desktop::terminal::TerminalError;

pub const NODE_EXECUTABLE_NAME: &str = "node";

pub fn open_external_terminal(app: &AppHandle) -> Result<(), TerminalError> {
    let launcher = unix_terminal::prepare_launcher(app)?;
    Command::new("open")
        .args(["-a", "Terminal"])
        .arg(launcher)
        .spawn()?;
    Ok(())
}
