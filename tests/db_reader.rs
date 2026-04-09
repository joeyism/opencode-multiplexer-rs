use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use ocmux_rs::{app::sessions::SessionStatus, data::db::reader::DbReader};
use rusqlite::{params, Connection};

#[test]
fn reads_projects_and_most_recent_session() {
    let db_path = temp_db_path("projects");
    let conn = init_db(&db_path);

    conn.execute(
        "INSERT INTO project (id, worktree, name, time_created, time_updated) VALUES (?1, ?2, 'repo', 1, 2)",
        params!["proj_1", "/tmp/repo"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, project_id, parent_id, title, directory, permission, time_created, time_updated, time_archived) VALUES (?1, ?2, NULL, 'title', '/tmp/repo', '{}', 1, 10, NULL)",
        params!["sess_1", "proj_1"],
    )
    .unwrap();

    let reader = DbReader::open(&db_path).unwrap();
    let projects = reader.get_projects().unwrap();
    let session = reader
        .get_most_recent_session("proj_1", 0)
        .unwrap()
        .unwrap();

    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].worktree, PathBuf::from("/tmp/repo"));
    assert_eq!(session.id, "sess_1");

    fs::remove_file(db_path).ok();
}

#[test]
fn session_status_prefers_needs_input_over_other_states() {
    let db_path = temp_db_path("status");
    let conn = init_db(&db_path);

    conn.execute(
        "INSERT INTO project (id, worktree, name, time_created, time_updated) VALUES ('proj_1', '/tmp/repo', 'repo', 1, 2)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, project_id, parent_id, title, directory, permission, time_created, time_updated, time_archived) VALUES ('sess_1', 'proj_1', NULL, 'title', '/tmp/repo', '{}', 1, 10, NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data, time_created) VALUES ('msg_1', 'sess_1', '{\"role\":\"assistant\",\"time\":{\"completed\":false}}', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO part (id, session_id, message_id, data, time_created) VALUES ('part_1', 'sess_1', 'msg_1', '{\"type\":\"tool\",\"tool\":\"question\",\"state\":{\"status\":\"running\"}}', 1)",
        [],
    )
    .unwrap();

    let reader = DbReader::open(&db_path).unwrap();

    assert_eq!(
        reader.get_session_status("sess_1").unwrap(),
        SessionStatus::NeedsInput
    );

    fs::remove_file(db_path).ok();
}

#[test]
fn reads_model_and_last_message_preview() {
    let db_path = temp_db_path("preview");
    let conn = init_db(&db_path);

    conn.execute(
        "INSERT INTO project (id, worktree, name, time_created, time_updated) VALUES ('proj_1', '/tmp/repo', 'repo', 1, 2)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, project_id, parent_id, title, directory, permission, time_created, time_updated, time_archived) VALUES ('sess_1', 'proj_1', NULL, 'title', '/tmp/repo', '{}', 1, 10, NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO message (id, session_id, data, time_created) VALUES ('msg_1', 'sess_1', '{\"role\":\"assistant\",\"modelID\":\"gpt-5\",\"time\":{\"completed\":true}}', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO part (id, session_id, message_id, data, time_created) VALUES ('part_1', 'sess_1', 'msg_1', '{\"type\":\"text\",\"text\":\"hello world\"}', 1)",
        [],
    )
    .unwrap();

    let reader = DbReader::open(&db_path).unwrap();

    assert_eq!(
        reader.get_session_model("sess_1").unwrap().as_deref(),
        Some("gpt-5")
    );
    let preview = reader.get_last_message_preview("sess_1").unwrap().unwrap();
    assert_eq!(preview.text, "hello world");
    assert_eq!(preview.role, "assistant");

    fs::remove_file(db_path).ok();
}

#[test]
fn reads_child_sessions() {
    let db_path = temp_db_path("children");
    let conn = init_db(&db_path);

    conn.execute(
        "INSERT INTO project (id, worktree, name, time_created, time_updated) VALUES ('proj_1', '/tmp/repo', 'repo', 1, 2)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, project_id, parent_id, title, directory, permission, time_created, time_updated, time_archived) VALUES ('parent', 'proj_1', NULL, 'parent title', '/tmp/repo', '{}', 1, 10, NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session (id, project_id, parent_id, title, directory, permission, time_created, time_updated, time_archived) VALUES ('child', 'proj_1', 'parent', 'child title', '/tmp/repo/child', '{}', 2, 20, NULL)",
        [],
    )
    .unwrap();

    let reader = DbReader::open(&db_path).unwrap();
    let children = reader.get_child_sessions("parent", 10, 0).unwrap();

    assert_eq!(children.len(), 1);
    assert_eq!(children[0].id, "child");
    assert!(reader.has_child_sessions("parent").unwrap());

    fs::remove_file(db_path).ok();
}

fn temp_db_path(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("ocmux-rs-{label}-{nanos}.db"))
}

fn init_db(path: &PathBuf) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE project (
            id TEXT PRIMARY KEY,
            worktree TEXT NOT NULL,
            name TEXT,
            time_created INTEGER,
            time_updated INTEGER
        );
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            project_id TEXT NOT NULL,
            parent_id TEXT,
            title TEXT,
            directory TEXT,
            permission TEXT,
            time_created INTEGER,
            time_updated INTEGER,
            time_archived INTEGER
        );
        CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            data TEXT NOT NULL,
            time_created INTEGER
        );
        CREATE TABLE part (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            message_id TEXT NOT NULL,
            data TEXT NOT NULL,
            time_created INTEGER
        );
        "#,
    )
    .unwrap();
    conn
}
