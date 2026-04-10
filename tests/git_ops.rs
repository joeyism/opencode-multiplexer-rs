use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use ocmux_rs::ops::git::{get_file_statuses, repo_relative_session_files};

fn temp_repo_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("ocmux-rs-repo-{}-{}", label, nanos))
}

fn git_init(dir: &PathBuf) {
    Command::new("git").arg("init").current_dir(dir).status().unwrap();
    Command::new("git").args(["config", "user.name", "Test"]).current_dir(dir).status().unwrap();
    Command::new("git").args(["config", "user.email", "test@example.com"]).current_dir(dir).status().unwrap();
}

#[test]
fn repo_relative_session_files_strips_absolute_paths_and_excludes_outside_files() {
    let repo = temp_repo_dir("relative");
    fs::create_dir_all(&repo).unwrap();
    let repo = fs::canonicalize(&repo).unwrap();
    git_init(&repo);

    let inside = repo.join("inside.txt");
    let outside = std::env::temp_dir().join("outside.txt");
    let relative = "already_rel.txt".to_string();

    let session_files = vec![
        inside.display().to_string(),
        outside.display().to_string(),
        relative.clone(),
        ".opencode/plans/123.md".to_string(),
    ];

    let result = repo_relative_session_files(&repo, &session_files).unwrap();

    assert_eq!(result.len(), 2);
    assert!(result.contains(&"inside.txt".to_string()));
    assert!(result.contains(&relative));
    assert!(!result.contains(&outside.display().to_string()));
    assert!(!result.contains(&".opencode/plans/123.md".to_string()));

    fs::remove_dir_all(repo).ok();
}

#[test]
fn get_file_statuses_categorizes_created_modified_deleted() {
    let repo = temp_repo_dir("status");
    fs::create_dir_all(&repo).unwrap();
    let repo = fs::canonicalize(&repo).unwrap();
    git_init(&repo);

    // Initial commit (baseline)
    fs::write(repo.join("to_modify.txt"), "v1").unwrap();
    fs::write(repo.join("to_delete.txt"), "v1").unwrap();
    Command::new("git").args(["add", "."]).current_dir(&repo).status().unwrap();
    Command::new("git").args(["commit", "-m", "init"]).current_dir(&repo).status().unwrap();

    // Create untracked, modify tracked, delete tracked
    fs::write(repo.join("created.txt"), "v1").unwrap();
    fs::write(repo.join("to_modify.txt"), "v2").unwrap();
    fs::remove_file(repo.join("to_delete.txt")).unwrap();

    // Session claims to have touched the created and modified files,
    // but knows nothing about the deleted file (bash rm scenario)
    let session_files = vec![
        "created.txt".to_string(),
        "to_modify.txt".to_string(),
    ];

    let (created, modified, deleted) = get_file_statuses(&repo, &session_files).unwrap();

    assert_eq!(created, vec!["created.txt"]);
    assert_eq!(modified, vec!["to_modify.txt"]);
    
    // The deleted file MUST be included even though it wasn't in session_files
    assert_eq!(deleted, vec!["to_delete.txt"]);

    fs::remove_dir_all(repo).ok();
}
