use crate::data::db::models::DbProject;
use std::path::Path;

pub mod cwd;
pub mod ps;

pub fn find_best_project<'a>(cwd: &Path, projects: &'a [DbProject]) -> Option<&'a DbProject> {
    projects
        .iter()
        .filter(|project| path_matches(cwd, &project.worktree))
        .max_by_key(|project| project.worktree.as_os_str().len())
}

fn path_matches(cwd: &Path, worktree: &Path) -> bool {
    cwd == worktree || cwd.starts_with(worktree) || worktree.starts_with(cwd)
}
