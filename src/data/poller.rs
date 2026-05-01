use std::{
    collections::HashMap,
    path::PathBuf,
    sync::mpsc::{self, Sender},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::app::sessions::SessionStatus;
use crate::data::{
    db::reader::DbReader,
    discovery::{
        cwd::cwd_for_pid,
        find_best_project,
        ps::{scan_processes, scan_serve_processes},
    },
};
use crate::registry::{load_managed_sessions, load_serve_registry};
use chrono::DateTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscoverySource {
    TuiExplicit,
    TuiHeuristic,
    Serve,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSessionInfo {
    pub session_id: String,
    pub cwd: PathBuf,
    pub title: String,
    pub status: SessionStatus,
    pub process_pid: Option<u32>,
    pub model: Option<String>,
    pub preview: Option<String>,
    pub time_updated: Option<i64>,
    pub has_children: bool,
    pub children: Vec<ChildSessionInfo>,
    pub serve_port: Option<u16>,
    pub source: DiscoverySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildSessionInfo {
    pub session_id: String,
    pub cwd: PathBuf,
    pub title: String,
    pub status: SessionStatus,
    pub time_updated: Option<i64>,
    pub has_children: bool,
    pub children: Vec<ChildSessionInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeSessionInfo {
    pub is_top_level: bool,
    pub is_managed: bool,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PollSnapshot {
    pub sessions: Vec<DiscoveredSessionInfo>,
}

pub fn should_include_serve_session(session: &ServeSessionInfo) -> bool {
    session.is_top_level
        && (session.is_managed
            || matches!(
                session.status,
                SessionStatus::Working | SessionStatus::NeedsInput
            ))
}

pub struct PollerHandle {
    stop_tx: Sender<()>,
    join_handle: thread::JoinHandle<()>,
}

impl PollerHandle {
    pub fn stop(self) {
        let _ = self.stop_tx.send(());
        let _ = self.join_handle.join();
    }
}

pub fn start_poller(poll_tx: Sender<PollSnapshot>) -> PollerHandle {
    let (stop_tx, stop_rx) = mpsc::channel();
    let join_handle = thread::spawn(move || {
        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }

            if let Ok(snapshot) = poll_once() {
                let _ = poll_tx.send(snapshot);
            }

            thread::sleep(Duration::from_secs(1));
        }
    });

    PollerHandle {
        stop_tx,
        join_handle,
    }
}

pub fn poll_once() -> anyhow::Result<PollSnapshot> {
    let reader = DbReader::open_default()?;
    let projects = reader.get_projects()?;
    let processes = scan_processes()?;
    let serve_processes = scan_serve_processes().unwrap_or_default();
    let managed_sessions = load_managed_sessions().unwrap_or_default();
    let mut sessions = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut offsets: HashMap<String, usize> = HashMap::new();
    let managed_tui_pids: std::collections::HashSet<u32> = load_serve_registry()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| e.tui_pid)
        .collect();

    let serve_sessions_by_port: HashMap<u16, Vec<String>> = std::thread::scope(|s| {
        let mut handles = Vec::new();
        for serve_process in &serve_processes {
            let port = serve_process.port;
            handles.push((
                port,
                s.spawn(move || fetch_recent_serve_session_ids(port).unwrap_or_default()),
            ));
        }
        let mut results = HashMap::new();
        for (port, handle) in handles {
            results.insert(port, handle.join().unwrap_or_default());
        }
        results
    });

    for serve_process in serve_processes {
        for serve_session_id in serve_sessions_by_port
            .get(&serve_process.port)
            .cloned()
            .unwrap_or_default()
        {
            if seen.contains(&serve_session_id) {
                continue;
            }
            let Some(session) = reader.get_session_by_id(&serve_session_id)? else {
                continue;
            };
            if !reader.is_top_level_session(&serve_session_id)? {
                continue;
            }
            let status = reader.get_session_status(&serve_session_id)?;
            if !should_include_serve_session(&ServeSessionInfo {
                is_top_level: true,
                is_managed: managed_sessions.contains(&serve_session_id),
                status,
            }) {
                continue;
            }
            seen.insert(serve_session_id.clone());
            let cwd = if session.directory.as_os_str().is_empty() {
                if let Some(proj) = projects
                    .iter()
                    .find(|project| project.id == session.project_id)
                {
                    proj.worktree.clone()
                } else {
                    continue; // Skip if no valid directory or project worktree exists
                }
            } else {
                session.directory.clone()
            };
            sessions.push(DiscoveredSessionInfo {
                session_id: serve_session_id.clone(),
                cwd,
                title: session.title.clone(),
                status,
                process_pid: Some(serve_process.pid),
                model: reader.get_session_model(&serve_session_id)?,
                preview: reader
                    .get_last_message_preview(&serve_session_id)?
                    .map(|preview| preview.text),
                time_updated: Some(session.time_updated),
                has_children: reader.has_child_sessions(&serve_session_id)?,
                children: collect_children(&reader, &serve_session_id, 2)?,
                serve_port: Some(serve_process.port),
                source: DiscoverySource::Serve,
            });
        }
    }

    for process in processes {
        if process.session_id.is_none() && managed_tui_pids.contains(&process.pid) {
            continue;
        }
        let Some(cwd) = cwd_for_pid(process.pid)? else {
            continue;
        };
        let Some(project) = find_best_project(&cwd, &projects) else {
            continue;
        };

        let (session, source) = if let Some(session_id) = process.session_id.as_deref() {
            (
                reader.get_session_by_id(session_id)?,
                DiscoverySource::TuiExplicit,
            )
        } else {
            let offset = offsets.entry(project.id.clone()).or_insert(0);
            let session = reader.get_most_recent_session(&project.id, *offset)?;
            *offset += 1;
            (session, DiscoverySource::TuiHeuristic)
        };

        let Some(session) = session else {
            continue;
        };
        if !seen.insert(session.id.clone()) {
            continue;
        }

        sessions.push(DiscoveredSessionInfo {
            session_id: session.id.clone(),
            cwd: if session.directory.as_os_str().is_empty() {
                cwd.clone()
            } else {
                session.directory.clone()
            },
            title: if session.title.is_empty() {
                project
                    .worktree
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| cwd.display().to_string())
            } else {
                session.title.clone()
            },
            status: reader.get_session_status(&session.id)?,
            process_pid: Some(process.pid),
            model: reader.get_session_model(&session.id)?,
            preview: reader
                .get_last_message_preview(&session.id)?
                .map(|preview| preview.text),
            time_updated: Some(session.time_updated),
            has_children: reader.has_child_sessions(&session.id)?,
            children: collect_children(&reader, &session.id, 2)?,
            serve_port: None,
            source,
        });
    }

    for managed_id in managed_sessions {
        if !seen.insert(managed_id.clone()) {
            continue;
        }
        let Some(session) = reader.get_session_by_id(&managed_id)? else {
            continue;
        };
        let cwd = if session.directory.as_os_str().is_empty() {
            if let Some(proj) = projects
                .iter()
                .find(|project| project.id == session.project_id)
            {
                proj.worktree.clone()
            } else {
                continue;
            }
        } else {
            session.directory.clone()
        };
        sessions.push(DiscoveredSessionInfo {
            session_id: managed_id.clone(),
            cwd,
            title: session.title.clone(),
            status: reader.get_session_status(&managed_id)?,
            process_pid: None,
            model: reader.get_session_model(&managed_id)?,
            preview: reader
                .get_last_message_preview(&managed_id)?
                .map(|preview| preview.text),
            time_updated: Some(session.time_updated),
            has_children: reader.has_child_sessions(&managed_id)?,
            children: collect_children(&reader, &managed_id, 2)?,
            serve_port: None,
            source: DiscoverySource::TuiExplicit,
        });
    }

    Ok(PollSnapshot { sessions })
}

fn collect_children(
    reader: &DbReader,
    parent_id: &str,
    max_depth: usize,
) -> anyhow::Result<Vec<ChildSessionInfo>> {
    if max_depth == 0 {
        return Ok(vec![]);
    }
    let children = reader.get_child_sessions(parent_id, 10, 0)?;
    children
        .into_iter()
        .map(|child| {
            let status = reader.get_session_status(&child.id)?;
            let has_children = reader.has_child_sessions(&child.id)?;
            let nested = if has_children {
                collect_children(reader, &child.id, max_depth - 1)?
            } else {
                vec![]
            };
            Ok(ChildSessionInfo {
                session_id: child.id,
                cwd: child.directory.clone(),
                title: child.title,
                status,
                time_updated: Some(child.time_updated),
                has_children,
                children: nested,
            })
        })
        .collect()
}

fn fetch_recent_serve_session_ids(port: u16) -> anyhow::Result<Vec<String>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(200))
        .build()?;
    let response = client
        .get(format!("http://localhost:{port}/session"))
        .send()?;
    if !response.status().is_success() {
        return Ok(vec![]);
    }
    let json: serde_json::Value = response.json()?;
    let cutoff = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_secs()
        .saturating_sub(24 * 60 * 60);
    let sessions = json
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.to_string();
            let updated = entry.pointer("/time/updated")?;
            let updated_epoch = if let Some(value) = updated.as_u64() {
                value
            } else if let Some(value) = updated.as_str() {
                value
                    .parse::<u64>()
                    .ok()
                    .or_else(|| chrono_like_epoch(value))?
            } else {
                return None;
            };
            (updated_epoch > cutoff).then_some(id)
        })
        .collect();
    Ok(sessions)
}

fn chrono_like_epoch(value: &str) -> Option<u64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.timestamp().max(0) as u64)
}

/// Hydrate a single session from the DB into a `DiscoveredSessionInfo`.
/// Returns `Ok(None)` if the session doesn't exist or has no directory.
#[cfg(test)]
fn hydrate_session(
    reader: &DbReader,
    session_id: &str,
    process_pid: Option<u32>,
    serve_port: Option<u16>,
    source: DiscoverySource,
) -> anyhow::Result<Option<DiscoveredSessionInfo>> {
    let Some(session) = reader.get_session_by_id(session_id)? else {
        return Ok(None);
    };
    if session.directory.as_os_str().is_empty() {
        return Ok(None);
    }
    let status = reader.get_session_status(session_id)?;
    Ok(Some(DiscoveredSessionInfo {
        session_id: session_id.to_string(),
        cwd: session.directory.clone(),
        title: session.title.clone(),
        status,
        process_pid,
        model: reader.get_session_model(session_id)?,
        preview: reader.get_last_message_preview(session_id)?.map(|p| p.text),
        time_updated: Some(session.time_updated),
        has_children: reader.has_child_sessions(session_id)?,
        children: collect_children(reader, session_id, 2)?,
        serve_port,
        source,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::sessions::SessionStatus;
    use rusqlite::Connection;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[test]
    fn hydrate_session_builds_info_from_db() {
        let db_path = temp_db_path("hydrate");
        let conn = init_db(&db_path);
        conn.execute(
            "INSERT INTO project VALUES ('proj1', '/tmp/proj', 'proj', 100, 200)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session VALUES ('sess1', 'proj1', NULL, 'My Title', '/tmp/proj', NULL, 100, 200, NULL)",
            [],
        )
        .unwrap();
        conn.execute(
            r#"INSERT INTO message VALUES ('msg1', 'sess1', '{"role":"assistant","time":{"completed":200}}', 200)"#,
            [],
        )
        .unwrap();

        let reader = DbReader::open(&db_path).unwrap();
        let info = hydrate_session(
            &reader,
            "sess1",
            Some(123),
            Some(4200),
            DiscoverySource::Serve,
        )
        .unwrap()
        .unwrap();
        assert_eq!(info.session_id, "sess1");
        assert_eq!(info.title, "My Title");
        assert_eq!(info.status, SessionStatus::Idle);
        assert_eq!(info.process_pid, Some(123));
        assert_eq!(info.serve_port, Some(4200));
        assert_eq!(info.source, DiscoverySource::Serve);
        assert_eq!(info.cwd, PathBuf::from("/tmp/proj"));
    }

    #[test]
    fn hydrate_session_returns_none_for_missing_session() {
        let db_path = temp_db_path("hydrate-miss");
        let _conn = init_db(&db_path);
        let reader = DbReader::open(&db_path).unwrap();
        let result =
            hydrate_session(&reader, "nonexistent", None, None, DiscoverySource::Serve).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn hydrate_session_returns_none_for_empty_directory() {
        let db_path = temp_db_path("hydrate-nodir");
        let conn = init_db(&db_path);
        conn.execute(
            "INSERT INTO project VALUES ('proj1', '/tmp/proj', 'proj', 100, 200)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session VALUES ('sess1', 'proj1', NULL, 'Title', '', NULL, 100, 200, NULL)",
            [],
        )
        .unwrap();

        let reader = DbReader::open(&db_path).unwrap();
        let result = hydrate_session(&reader, "sess1", None, None, DiscoverySource::Serve).unwrap();
        assert!(result.is_none());
    }
}
