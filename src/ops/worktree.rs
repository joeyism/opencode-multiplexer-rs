use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreePlan {
    pub target_dir: PathBuf,
    pub args: Vec<String>,
}

pub fn worktree_target_dir(repo_dir: &Path, branch_name: &str) -> PathBuf {
    repo_dir.join(".worktrees").join(branch_name)
}

pub fn build_worktree_plan(
    repo_dir: &Path,
    branch_name: &str,
    branch_exists: bool,
    base_branch: &str,
) -> WorktreePlan {
    let target_dir = worktree_target_dir(repo_dir, branch_name);
    let args = if branch_exists {
        vec![
            "worktree".into(),
            "add".into(),
            target_dir.display().to_string(),
            branch_name.into(),
        ]
    } else {
        vec![
            "worktree".into(),
            "add".into(),
            "-b".into(),
            branch_name.into(),
            target_dir.display().to_string(),
            base_branch.into(),
        ]
    };
    WorktreePlan { target_dir, args }
}

pub fn create_worktree(repo_dir: &Path, branch_name: &str) -> anyhow::Result<PathBuf> {
    let target_dir = worktree_target_dir(repo_dir, branch_name);
    if target_dir.exists() {
        return Ok(target_dir);
    }
    fs::create_dir_all(repo_dir.join(".worktrees"))?;
    let branch_exists = git_ref_exists(repo_dir, branch_name)?;
    let base_branch = resolve_base_branch(repo_dir)?;
    let plan = build_worktree_plan(repo_dir, branch_name, branch_exists, &base_branch);
    let status = Command::new("git")
        .args(&plan.args)
        .current_dir(repo_dir)
        .status()?;
    if !status.success() {
        anyhow::bail!("git worktree add failed")
    }
    Ok(plan.target_dir)
}

fn git_ref_exists(repo_dir: &Path, branch_name: &str) -> anyhow::Result<bool> {
    Ok(Command::new("git")
        .args(["rev-parse", "--verify", branch_name])
        .current_dir(repo_dir)
        .status()?
        .success())
}

fn resolve_base_branch(repo_dir: &Path) -> anyhow::Result<String> {
    let origin_head = Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .current_dir(repo_dir)
        .output()?;
    if origin_head.status.success() {
        let value = String::from_utf8_lossy(&origin_head.stdout)
            .trim()
            .to_string();
        if let Some(branch) = value.rsplit('/').next() {
            if !branch.is_empty() {
                return Ok(branch.to_string());
            }
        }
    }

    let master = Command::new("git")
        .args(["rev-parse", "--verify", "master"])
        .current_dir(repo_dir)
        .status()?;
    if master.success() {
        return Ok("master".into());
    }
    Ok("main".into())
}
