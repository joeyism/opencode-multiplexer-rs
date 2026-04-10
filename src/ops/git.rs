use std::path::{Path, PathBuf};
use std::process::Command;

pub fn commit_and_push_files(
    cwd: &Path,
    created: &[String],
    modified: &[String],
    deleted: &[String],
    message: &str,
) -> anyhow::Result<String> {
    let mut to_add = Vec::new();
    to_add.extend_from_slice(created);
    to_add.extend_from_slice(modified);
    to_add.extend_from_slice(deleted);
    
    if to_add.is_empty() {
        anyhow::bail!("no files to commit");
    }

    // git add <files>
    let mut add = Command::new("git");
    add.arg("add").args(&to_add).current_dir(cwd);
    let add_out = add.output()?;

    // git commit -m <msg>
    let mut commit = Command::new("git");
    commit.args(["commit", "-m", message]).current_dir(cwd);
    let commit_out = commit.output()?;

    // git push origin HEAD
    let mut push = Command::new("git");
    push.args(["push", "origin", "HEAD"]).current_dir(cwd);
    let push_out = push.output()?;

    // Combine all output
    let mut output = String::new();
    for (label, out) in [("add", &add_out), ("commit", &commit_out), ("push", &push_out)] {
        output.push_str(&format!("--- git {} ---\n", label));
        output.push_str(&String::from_utf8_lossy(&out.stdout));
        output.push_str(&String::from_utf8_lossy(&out.stderr));
        output.push('\n');
    }
    Ok(output)
}

pub fn get_file_statuses(
    cwd: &Path,
    session_files: &[String],
) -> anyhow::Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let session_files = repo_relative_session_files(cwd, session_files)?;
    let mut created = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();

    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .current_dir(cwd)
        .output()?;

    if !output.status.success() {
        return Ok((created, modified, deleted));
    }

    let status_output = String::from_utf8_lossy(&output.stdout);
    for line in status_output.lines() {
        if line.len() < 4 {
            continue;
        }
        let code = &line[0..2];
        let file_path = &line[3..];
        
        if code.contains('D') {
            deleted.push(file_path.to_string());
            continue;
        }

        let mut matched = false;
        for sf in &session_files {
            if sf.ends_with(file_path) || file_path.ends_with(sf) {
                matched = true;
                break;
            }
        }
        if !matched {
            continue;
        }

        if code.contains('?') || code.contains('A') {
            created.push(file_path.to_string());
        } else if code.contains('M') {
            modified.push(file_path.to_string());
        }
    }

    Ok((created, modified, deleted))
}


fn repo_root(cwd: &Path) -> anyhow::Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()?;
    if !output.status.success() {
        anyhow::bail!("not inside a git repository");
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string()))
}

pub fn repo_relative_session_files(cwd: &Path, session_files: &[String]) -> anyhow::Result<Vec<String>> {
    let root = repo_root(cwd)?;
    let root_str = root.display().to_string();
    
    let mut files = Vec::new();
    for file in session_files {
        if file.starts_with(&root_str) {
            let rel = file.strip_prefix(&root_str).unwrap().trim_start_matches('/');
            files.push(rel.to_string());
        } else if !Path::new(file).is_absolute() {
            files.push(file.clone());
        } else {
            // Absolute path but doesn't strictly string-match root.
            // On macOS, /var symlinks to /private/var. Try fs::canonicalize.
            if let Ok(canon_file) = std::fs::canonicalize(file) {
                if let Ok(canon_root) = std::fs::canonicalize(&root) {
                    if let Ok(rel) = canon_file.strip_prefix(&canon_root) {
                        files.push(rel.display().to_string());
                        continue;
                    }
                }
            }
        }
    }
    files.sort();
    files.dedup();
    files.retain(|f| !f.starts_with(".opencode/") && !f.starts_with(".opencode\\"));
    Ok(files)
}
