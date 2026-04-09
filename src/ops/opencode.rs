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
