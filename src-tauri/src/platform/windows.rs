use tauri::AppHandle;

use crate::desktop::terminal::{open_dsh_powershell, TerminalError};

pub const NODE_EXECUTABLE_NAME: &str = "node.exe";

pub fn open_external_terminal(app: &AppHandle) -> Result<(), TerminalError> {
    open_dsh_powershell(app)
}
