use std::path::Path;

use portable_pty::CommandBuilder;

pub fn build_managed_session_command(cwd: &Path) -> CommandBuilder {
    let mut command = CommandBuilder::new("opencode");
    command.cwd(cwd);
    command
}

pub fn build_replica_command(cwd: &Path, session_id: &str) -> CommandBuilder {
    let mut command = CommandBuilder::new("opencode");
    command.args(["-s", session_id]);
    command.cwd(cwd);
    command
}

pub fn display_title_for_cwd(cwd: &Path) -> String {
    cwd.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| cwd.display().to_string())
}

use std::process::{Command, Stdio};
use std::net::TcpListener;

pub fn find_available_port(start: u16) -> u16 {
    for port in start..start + 100 {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    start
}

pub fn spawn_serve_daemon(cwd: &Path, port: u16) -> anyhow::Result<u32> {
    let child = Command::new("opencode")
        .args(["serve", "--port", &port.to_string()])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(child.id())
}

pub fn wait_for_serve_ready(port: u16, timeout_secs: u64) -> bool {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    while start.elapsed() < timeout {
        if let Ok(resp) = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(500))
            .build()
            .and_then(|c| c.get(format!("http://localhost:{}/session", port)).send())
        {
            if resp.status().is_success() {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

pub fn get_latest_session_id_from_serve(port: u16) -> anyhow::Result<Option<String>> {
    let resp = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()?
        .get(format!("http://localhost:{}/session", port))
        .send()?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let json: serde_json::Value = resp.json()?;
    let sessions = json.as_array();
    if let Some(sessions) = sessions {
        if let Some(first) = sessions.first() {
            if let Some(id) = first.get("id").and_then(|v| v.as_str()) {
                return Ok(Some(id.to_string()));
            }
        }
    }
    Ok(None)
}
