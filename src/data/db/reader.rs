use std::path::{Path, PathBuf};

use anyhow::Context;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

use crate::{
    app::sessions::SessionStatus,
    data::db::models::{DbProject, DbSession, DbSessionSummary, SessionPreview},
};

pub struct DbReader {
    conn: Connection,
}

impl DbReader {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .with_context(|| format!("failed to open sqlite db at {}", path.display()))?;
        Ok(Self { conn })
    }

    pub fn open_default() -> anyhow::Result<Self> {
        Self::open(&default_db_path()?)
    }


    pub fn get_all_sessions(&self) -> anyhow::Result<Vec<DbSessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.title, s.directory,
                    COALESCE(
                        (SELECT time_created FROM message WHERE session_id = s.id AND json_extract(data, '$.role') = 'user' ORDER BY time_created DESC LIMIT 1),
                        s.time_created
                    ) as last_interaction,
                    s.time_archived, p.worktree
             FROM session s
             JOIN project p ON p.id = s.project_id
             WHERE s.parent_id IS NULL
             ORDER BY last_interaction DESC
             LIMIT 500"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DbSessionSummary {
                id: row.get(0)?,
                title: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                directory: PathBuf::from(row.get::<_, Option<String>>(2)?.unwrap_or_default()),
                time_updated: row.get(3)?,
                archived: row.get::<_, Option<i64>>(4)?.is_some(),
                worktree: PathBuf::from(row.get::<_, String>(5)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn get_projects(&self) -> anyhow::Result<Vec<DbProject>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, worktree FROM project ORDER BY time_updated DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(DbProject {
                id: row.get(0)?,
                worktree: PathBuf::from(row.get::<_, String>(1)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_most_recent_session(
        &self,
        project_id: &str,
        offset: usize,
    ) -> anyhow::Result<Option<DbSession>> {
        self.conn
            .query_row(
                "SELECT id, project_id, title, directory,
                        COALESCE(
                            (SELECT time_created FROM message WHERE session_id = session.id AND json_extract(data, '$.role') = 'user' ORDER BY time_created DESC LIMIT 1),
                            time_created
                        ) as last_interaction
                 FROM session 
                 WHERE project_id = ?1 AND time_archived IS NULL AND parent_id IS NULL 
                 ORDER BY last_interaction DESC 
                 LIMIT 1 OFFSET ?2",
                params![project_id, offset as i64],
                |row| {
                    Ok(DbSession {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        title: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        directory: PathBuf::from(row.get::<_, Option<String>>(3)?.unwrap_or_default()),
                        time_updated: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_session_by_id(&self, session_id: &str) -> anyhow::Result<Option<DbSession>> {
        self.conn
            .query_row(
                "SELECT id, project_id, title, directory,
                        COALESCE(
                            (SELECT time_created FROM message WHERE session_id = session.id AND json_extract(data, '$.role') = 'user' ORDER BY time_created DESC LIMIT 1),
                            time_created
                        ) as last_interaction
                 FROM session WHERE id = ?1",
                [session_id],
                |row| {
                    Ok(DbSession {
                        id: row.get(0)?,
                        project_id: row.get(1)?,
                        title: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        directory: PathBuf::from(
                            row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        ),
                        time_updated: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_child_sessions(
        &self,
        parent_session_id: &str,
        limit: usize,
        offset: usize,
    ) -> anyhow::Result<Vec<DbSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, title, directory, time_updated FROM session WHERE parent_id = ?1 AND time_archived IS NULL ORDER BY time_created DESC LIMIT ?2 OFFSET ?3",
        )?;
        let rows = stmt.query_map(
            params![parent_session_id, limit as i64, offset as i64],
            |row| {
                Ok(DbSession {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    title: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    directory: PathBuf::from(row.get::<_, Option<String>>(3)?.unwrap_or_default()),
                    time_updated: row.get(4)?,
                })
            },
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn has_child_sessions(&self, session_id: &str) -> anyhow::Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM session WHERE parent_id = ?1 AND time_archived IS NULL LIMIT 1",
            [session_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn is_top_level_session(&self, session_id: &str) -> anyhow::Result<bool> {
        let result: Option<(Option<String>, Option<i64>)> = self
            .conn
            .query_row(
                "SELECT parent_id, time_archived FROM session WHERE id = ?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(matches!(result, Some((None, None))))
    }

    pub fn get_session_status(&self, session_id: &str) -> anyhow::Result<SessionStatus> {
        let latest_message_id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM message WHERE session_id = ?1 ORDER BY time_created DESC LIMIT 1",
                [session_id],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(message_id) = latest_message_id.as_deref() {
            let latest_running: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM part p WHERE p.session_id = ?1 AND json_extract(p.data, '$.type') = 'tool' AND json_extract(p.data, '$.tool') IN ('question', 'plan_exit') AND json_extract(p.data, '$.state.status') = 'running' AND p.message_id = ?2",
                params![session_id, message_id],
                |row| row.get(0),
            )?;
            if latest_running > 0 {
                return Ok(SessionStatus::NeedsInput);
            }
        }

        let child_running: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM part p JOIN message m ON m.id = p.message_id JOIN session s ON s.id = m.session_id WHERE s.parent_id = ?1 AND s.time_archived IS NULL AND json_extract(p.data, '$.type') = 'tool' AND json_extract(p.data, '$.tool') IN ('question', 'plan_exit') AND json_extract(p.data, '$.state.status') = 'running'",
            [session_id],
            |row| row.get(0),
        )?;
        if child_running > 0 {
            return Ok(SessionStatus::NeedsInput);
        }

        if let Some(message_id) = latest_message_id.as_deref() {
            let latest_error: i64 = self.conn.query_row(
                "SELECT COUNT(*) FROM part p WHERE p.session_id = ?1 AND json_extract(p.data, '$.type') = 'tool' AND json_extract(p.data, '$.state.status') = 'error' AND p.message_id = ?2",
                params![session_id, message_id],
                |row| row.get(0),
            )?;
            if latest_error > 0 {
                return Ok(SessionStatus::Error);
            }
        }

        let latest_message: Option<(Option<String>, Option<i64>)> = self.conn.query_row(
            "SELECT json_extract(data, '$.role') as role, json_extract(data, '$.time.completed') as completed FROM message WHERE session_id = ?1 ORDER BY time_created DESC LIMIT 1",
            [session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;

        match latest_message {
            Some((Some(role), _completed)) if role == "user" => Ok(SessionStatus::Working),
            Some((Some(role), completed)) if role == "assistant" && completed.is_none() => {
                Ok(SessionStatus::Working)
            }
            Some(_) => Ok(SessionStatus::Idle),
            None => Ok(SessionStatus::Idle),
        }
    }


    pub fn get_session_modified_files(&self, session_id: &str) -> anyhow::Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT file_path FROM (
                SELECT COALESCE(
                    json_extract(data, '$.state.input.filePath'),
                    json_extract(data, '$.state.input.path'),
                    json_extract(data, '$.state.metadata.filepath'),
                    json_extract(data, '$.state.metadata.filediff.file'),
                    json_extract(data, '$.input.filePath'),
                    json_extract(data, '$.input.path')
                ) AS file_path
                FROM part
                WHERE session_id = ?1
                  AND json_extract(data, '$.type') = 'tool'
                  AND json_extract(data, '$.tool') IN ('edit', 'write', 'apply_patch', 'github_create_or_update_file')
            ) WHERE file_path IS NOT NULL
            ORDER BY file_path"
        )?;
        let rows = stmt.query_map([session_id], |row| row.get(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<String>>>()?)
    }
    pub fn get_session_model(&self, session_id: &str) -> anyhow::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT json_extract(data, '$.modelID') as model_id FROM message WHERE session_id = ?1 AND json_extract(data, '$.role') = 'assistant' AND json_extract(data, '$.modelID') IS NOT NULL ORDER BY time_created DESC LIMIT 1",
                [session_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn get_last_message_preview(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<SessionPreview>> {
        self.conn
            .query_row(
                "SELECT json_extract(p.data, '$.text') as text, json_extract(m.data, '$.role') as role FROM part p JOIN message m ON p.message_id = m.id WHERE p.session_id = ?1 AND json_extract(p.data, '$.type') = 'text' AND json_extract(p.data, '$.text') IS NOT NULL AND json_extract(p.data, '$.text') != '' AND json_extract(p.data, '$.text') NOT LIKE '<%' ORDER BY m.time_created DESC, p.time_created DESC LIMIT 1",
                [session_id],
                |row| {
                    Ok(SessionPreview {
                        text: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                        role: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }
}

fn default_db_path() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/share/opencode/opencode.db"))
}
