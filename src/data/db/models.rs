use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbProject {
    pub id: String,
    pub worktree: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSession {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub directory: PathBuf,
    pub time_updated: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPreview {
    pub text: String,
    pub role: String,
}
