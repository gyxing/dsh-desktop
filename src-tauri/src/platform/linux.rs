use std::{env, path::PathBuf, process::Command};

use tauri::AppHandle;

use super::unix_terminal;
use crate::desktop::terminal::TerminalError;

pub const NODE_EXECUTABLE_NAME: &str = "node";

pub fn open_external_terminal(app: &AppHandle) -> Result<(), TerminalError> {
    let launcher = unix_terminal::prepare_launcher(app)?;
    let candidates: [(&str, &[&str]); 4] = [
        ("x-terminal-emulator", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("konsole", &["-e"]),
        ("xfce4-terminal", &["-e"]),
    ];
    for (name, arguments) in candidates {
        if let Some(executable) = find_in_path(name) {
            Command::new(executable)
                .args(arguments)
                .arg(&launcher)
                .spawn()?;
            return Ok(());
        }
    }
    Err(TerminalError::SystemTerminalUnavailable(
        "x-terminal-emulator、GNOME Terminal、Konsole、Xfce Terminal".to_string(),
    ))
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}
