use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::process::Command;

pub fn cwd_for_pid(pid: u32) -> anyhow::Result<Option<PathBuf>> {
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("lsof")
            .args(["-p", &pid.to_string()])
            .output()?;
        if !output.status.success() {
            return Ok(None);
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let cwd = stdout
            .lines()
            .find(|line| line.contains(" cwd "))
            .map(|line| {
                line.split_whitespace()
                    .skip(8)
                    .collect::<Vec<_>>()
                    .join(" ")
            });
        Ok(cwd.filter(|value| !value.is_empty()).map(PathBuf::from))
    }

    #[cfg(not(target_os = "macos"))]
    {
        let path = std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
        Ok(path)
    }
}
