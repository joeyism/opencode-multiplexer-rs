use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use rusqlite::{Connection, OpenFlags, params};
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlanTask {
    pub id: String,
    pub title: String,
    #[serde(default, rename = "dependsOn", alias = "depends_on")]
    pub depends_on: Vec<String>,
}

impl PlanTask {
    pub fn new(id: &str, title: &str, depends_on: Vec<&str>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            depends_on: depends_on.into_iter().map(str::to_string).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ParallelPlan {
    #[serde(default, rename = "planId", alias = "plan_id")]
    pub plan_id: String,
    #[serde(default)]
    pub title: String,
    pub tasks: Vec<PlanTask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskState {
    pub task_id: String,
    pub status: String,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

impl TaskState {
    pub fn new(task_id: &str, status: &str, session_id: Option<&str>, error: Option<&str>) -> Self {
        Self {
            task_id: task_id.into(),
            status: status.into(),
            session_id: session_id.map(str::to_string),
            error: error.map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParallelRun {
    pub run_id: String,
    pub plan_id: String,
    pub project_directory: PathBuf,
    pub status: String,
    pub tasks: Vec<TaskState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParallelSnapshot {
    pub plan: ParallelPlan,
    pub run: Option<ParallelRun>,
}

pub fn load_snapshot(
    project_directory: &Path,
    parent_session_id: &str,
) -> anyhow::Result<Option<ParallelSnapshot>> {
    let db_path = project_directory.join(".opencode/parallel-builds/runs.db");
    if !db_path.is_file() {
        return Ok(None);
    }

    let connection = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .with_context(|| format!("open parallel-builds database {}", db_path.display()))?;

    let mut runs = connection.prepare(
        "SELECT run_id, plan_id, project_directory, status
         FROM runs WHERE parent_session_id = ?1
         ORDER BY created_at DESC
         LIMIT 1",
    )?;
    let mut rows = runs.query(params![parent_session_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let run_id: String = row.get(0)?;
    let plan_id: String = row.get(1)?;
    let run_project = PathBuf::from(row.get::<_, String>(2)?);
    let run_status: String = row.get(3)?;

    let plan_root = if run_project.as_os_str().is_empty() {
        project_directory
    } else {
        &run_project
    };
    let plan_path = plan_root
        .join(".opencode/parallel-builds/plans")
        .join(&plan_id)
        .join("plan.json");
    let plan: ParallelPlan = serde_json::from_str(&fs::read_to_string(&plan_path)?)
        .with_context(|| format!("parse parallel-builds plan {}", plan_path.display()))?;

    let mut task_rows = connection
        .prepare("SELECT task_id, status, session_id, error FROM run_tasks WHERE run_id = ?1")?;
    let tasks = task_rows
        .query_map(params![run_id], |row| {
            Ok(TaskState {
                task_id: row.get(0)?,
                status: row.get(1)?,
                session_id: row.get(2)?,
                error: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(Some(ParallelSnapshot {
        plan,
        run: Some(ParallelRun {
            run_id,
            plan_id,
            project_directory: run_project,
            status: run_status,
            tasks,
        }),
    }))
}

#[allow(dead_code)]
fn task_state_map(states: &[TaskState]) -> HashMap<&str, &TaskState> {
    states
        .iter()
        .map(|state| (state.task_id.as_str(), state))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn plan_task_accepts_camel_case_dependency_field() {
        let plan: ParallelPlan = serde_json::from_str(
            r#"{"planId":"demo","tasks":[{"id":"b","title":"B","dependsOn":["a"]}]}"#,
        )
        .unwrap();
        assert_eq!(plan.plan_id, "demo");
        assert_eq!(plan.tasks[0].depends_on, vec!["a"]);
    }

    #[test]
    fn snapshot_uses_latest_run_by_created_at_and_joins_task_session() {
        let root = std::env::temp_dir().join(format!("ocmux-agents-{}", uuid::Uuid::new_v4()));
        let store = root.join(".opencode/parallel-builds");
        fs::create_dir_all(store.join("plans/demo")).unwrap();
        fs::create_dir_all(store.join("plans/newer")).unwrap();
        fs::write(
            store.join("plans/demo/plan.json"),
            r#"{"planId":"demo","title":"Demo","tasks":[{"id":"a","title":"A","dependsOn":[] }]}"#,
        )
        .unwrap();
        fs::write(
            store.join("plans/newer/plan.json"),
            r#"{"planId":"newer","title":"Newer","tasks":[{"id":"b","title":"B","dependsOn":[] }]}"#,
        )
        .unwrap();
        let db_path = store.join("runs.db");
        let connection = Connection::open(&db_path).unwrap();
        connection.execute_batch(
            "CREATE TABLE runs (run_id TEXT, plan_id TEXT, parent_session_id TEXT, project_directory TEXT, status TEXT, created_at TEXT);
             CREATE TABLE run_tasks (run_id TEXT, task_id TEXT, status TEXT, session_id TEXT, error TEXT);
             -- Older non-terminal run must not win over a newer terminal run.
             INSERT INTO runs VALUES ('old','demo','root','', 'running','2026-01-01');
             INSERT INTO runs VALUES ('new','newer','root','', 'cancelled','2026-01-02');
             INSERT INTO run_tasks VALUES ('old','a','running','stale',NULL);
             INSERT INTO run_tasks VALUES ('new','b','cancelled','child',NULL);",
        )
        .unwrap();
        drop(connection);

        let snapshot = load_snapshot(&root, "root").unwrap().unwrap();

        assert_eq!(snapshot.run.as_ref().unwrap().run_id, "new");
        assert_eq!(snapshot.plan.plan_id, "newer");
        assert_eq!(
            snapshot.run.as_ref().unwrap().tasks[0]
                .session_id
                .as_deref(),
            Some("child")
        );
        let _ = fs::remove_dir_all(root);
    }
}
