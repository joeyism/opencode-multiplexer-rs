use std::{collections::HashMap, path::PathBuf};

use crate::{
    app::sessions::{SessionList, SessionOrigin, SessionStatus, SessionSummary},
    data::poller::{ChildSessionInfo, PollSnapshot},
    ui::sidebar::{ChildSidebarEntry, SidebarEntry},
};

use super::pty::PtySession;

#[derive(Default)]
pub struct PtyManager {
    sessions: SessionList,
    ptys: HashMap<u64, Option<PtySession>>,
}

impl PtyManager {
    #[allow(clippy::too_many_arguments)]
    pub fn register_placeholder(
        &mut self,
        cwd: PathBuf,
        title: String,
        status: SessionStatus,
        session_id: Option<String>,
        origin: SessionOrigin,
        process_pid: Option<u32>,
        serve_pid: Option<u32>,
        model: Option<String>,
        preview: Option<String>,
        time_updated: Option<i64>,
        has_children: bool,
        children: Vec<crate::data::poller::ChildSessionInfo>,
    ) -> u64 {
        let id = self.sessions.push(
            cwd,
            title,
            status,
            session_id,
            origin,
            process_pid,
            serve_pid,
            model,
            preview,
            time_updated,
            has_children,
            children,
        );
        self.ptys.insert(id, None);
        id
    }

    pub fn spawn_managed(
        &mut self,
        cwd: PathBuf,
        title: String,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<u64> {
        use crate::ops::opencode::{find_available_port, spawn_serve_daemon, wait_for_serve_ready};
        use crate::registry::{register_serve_process, update_serve_registry_tui_pid};

        // Spawn serve daemon as persistent backend
        let port = find_available_port(4200);
        let serve_pid = spawn_serve_daemon(&cwd, port)?;
        register_serve_process(port, serve_pid, &cwd)?;

        // Wait for serve to be ready
        if !wait_for_serve_ready(port, 10) {
            anyhow::bail!("opencode serve did not start within 10s on port {}", port);
        }

        // Spawn TUI client as a disposable PTY (always fresh, no -s flag)
        let pty = PtySession::spawn_managed(&cwd, rows, cols)?;
        let session_id = None;
        let process_pid = pty.process_id();
        if let Some(pid) = process_pid {
            let _ = update_serve_registry_tui_pid(port, pid);
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let id = self.sessions.push(
            cwd,
            title,
            SessionStatus::Working,
            session_id,
            SessionOrigin::Managed,
            process_pid,
            Some(serve_pid),
            None,
            None,
            Some(now_ms),
            false,
            vec![],
        );
        self.ptys.insert(id, Some(pty));
        self.sessions.select_last();
        self.sessions.activate_selected();
        Ok(id)
    }

    pub fn activate_or_attach_selected(&mut self, rows: u16, cols: u16) -> anyhow::Result<()> {
        let Some(selected_id) = self.selected_id() else {
            return Ok(());
        };

        let needs_attach = self.ptys.get(&selected_id).is_some_and(|pty| pty.is_none());
        if needs_attach {
            let Some(summary) = self.selected_summary().cloned() else {
                return Ok(());
            };
            if let Some(session_id) = summary.session_id.as_deref() {
                let pty = PtySession::spawn_replica(&summary.cwd, session_id, rows, cols)?;
                if let Some(slot) = self.ptys.get_mut(&selected_id) {
                    *slot = Some(pty);
                }
            }
        }

        self.activate_selected();
        self.resize_active(rows, cols)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn attach_arbitrary_session(
        &mut self,
        session_id: String,
        cwd: PathBuf,
        title: String,
        status: SessionStatus,
        time_updated: Option<i64>,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<()> {
        let pty = PtySession::spawn_replica(&cwd, &session_id, rows, cols)?;
        let process_pid = pty.process_id();
        let id = self.sessions.push(
            cwd,
            title,
            status,
            Some(session_id),
            SessionOrigin::Managed,
            process_pid,
            None,
            None,
            None,
            time_updated,
            false,
            vec![],
        );
        self.ptys.insert(id, Some(pty));
        self.sessions.select_last();
        self.sessions.activate_selected();
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn active_id(&self) -> Option<u64> {
        self.sessions.active_id()
    }

    pub fn selected_id(&self) -> Option<u64> {
        self.sessions.selected_id()
    }

    pub fn pending_kill(&self) -> Option<u64> {
        self.sessions.pending_kill()
    }

    pub fn selected_index(&self) -> usize {
        self.sessions.selected_index()
    }

    pub fn selected_summary(&self) -> Option<&SessionSummary> {
        let selected = self.sessions.selected_id()?;
        self.sessions
            .items()
            .iter()
            .find(|session| session.id == selected)
    }

    pub fn select_next(&mut self) {
        self.sessions.select_next();
    }

    pub fn select_prev(&mut self) {
        self.sessions.select_prev();
    }

    pub fn select_top_level(&mut self, id: u64) {
        self.sessions.select_id(id);
    }

    pub fn activate_selected(&mut self) {
        self.sessions.activate_selected();
    }

    pub fn request_kill_selected(&mut self) {
        self.sessions.request_kill_selected();
    }

    pub fn cancel_kill(&mut self) {
        self.sessions.cancel_kill();
    }

    pub fn kill_selected(&mut self) -> anyhow::Result<Option<u64>> {
        let id = match self.sessions.pending_kill() {
            Some(id) => id,
            None => return Ok(None),
        };

        let keep_placeholder = self
            .sessions
            .items()
            .iter()
            .find(|session| session.id == id)
            .is_some_and(|session| session.origin == SessionOrigin::Discovered);

        if let Some(Some(pty)) = self.ptys.get_mut(&id) {
            let _ = pty.kill();
        }

        if keep_placeholder {
            self.sessions.cancel_kill();
            if let Some(pty) = self.ptys.get_mut(&id) {
                *pty = None;
            }
            return Ok(Some(id));
        }

        let killed = self.sessions.confirm_kill();
        if let Some(id) = killed {
            self.ptys.remove(&id);
        }
        Ok(killed)
    }

    pub fn kill_selected_placeholder(&mut self) -> Option<u64> {
        self.sessions.request_kill_selected();
        let killed = self.sessions.confirm_kill();
        if let Some(id) = killed {
            self.ptys.remove(&id);
        }
        killed
    }

    pub fn active_session_mut(&mut self) -> Option<&mut PtySession> {
        let id = self.sessions.active_id()?;
        self.ptys.get_mut(&id)?.as_mut()
    }

    pub fn active_session(&self) -> Option<&PtySession> {
        let id = self.sessions.active_id()?;
        self.ptys.get(&id)?.as_ref()
    }

    pub fn active_summary(&self) -> Option<&SessionSummary> {
        let active_id = self.sessions.active_id()?;
        self.sessions
            .items()
            .iter()
            .find(|session| session.id == active_id)
    }

    pub fn reap_exited_ptys(&mut self) -> Vec<u64> {
        let dead_ids: Vec<u64> = self
            .ptys
            .iter_mut()
            .filter_map(|(&id, slot)| {
                if let Some(pty) = slot.as_mut() {
                    if !pty.is_alive() { Some(id) } else { None }
                } else {
                    None
                }
            })
            .collect();

        for &id in &dead_ids {
            if let Some(slot) = self.ptys.get_mut(&id) {
                *slot = None;
            }
        }

        dead_ids
    }

    pub fn drain_all_output(&mut self) {
        for pty in self.ptys.values_mut().filter_map(Option::as_mut) {
            pty.drain_output();
        }
    }

    pub fn resize_active(&mut self, rows: u16, cols: u16) -> anyhow::Result<()> {
        if let Some(pty) = self.active_session_mut() {
            pty.resize(rows, cols)?;
        }
        Ok(())
    }

    pub fn sidebar_entries(&self) -> Vec<SidebarEntry> {
        let mut sessions = self.sessions.items().iter().collect::<Vec<_>>();
        sessions.sort_by(|a, b| match (a.time_updated, b.time_updated) {
            (Some(a), Some(b)) => b.cmp(&a),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });

        sessions
            .into_iter()
            .map(|session| SidebarEntry {
                top_level_id: session.id,
                session_id: session.session_id.clone(),
                cwd: session.cwd.clone(),
                title: session.title.clone(),
                status: session.status,
                active: self.sessions.active_id() == Some(session.id),
                origin: session.origin,
                time_updated: session.time_updated,
                has_children: session.has_children,
                children: session.children.iter().map(convert_child).collect(),
            })
            .collect()
    }

    pub fn apply_poll_snapshot(&mut self, snapshot: PollSnapshot) {
        let keep_ids = snapshot
            .sessions
            .iter()
            .map(|info| info.session_id.clone())
            .collect::<std::collections::HashSet<_>>();

        for discovered in snapshot.sessions {
            if let Some(id) = self.sessions.find_by_session_id(&discovered.session_id) {
                if let Some(summary) = self.sessions.get_mut(id) {
                    summary.cwd = discovered.cwd.clone();
                    summary.title = discovered.title.clone();
                    summary.status = discovered.status;
                    if summary.serve_pid != discovered.process_pid {
                        summary.process_pid = discovered.process_pid;
                    }
                    summary.model = discovered.model.clone();
                    summary.preview = discovered.preview.clone();
                    summary.time_updated = discovered.time_updated;
                    summary.has_children = discovered.has_children;
                    summary.children = discovered.children.clone();
                }
                continue;
            }

            if let Some(process_pid) = discovered.process_pid
                && let Some(id) = self.sessions.find_by_process_pid(process_pid)
            {
                if let Some(summary) = self.sessions.get_mut(id) {
                    summary.session_id = Some(discovered.session_id.clone());
                    summary.cwd = discovered.cwd.clone();
                    summary.title = discovered.title.clone();
                    summary.status = discovered.status;
                    if summary.serve_pid != discovered.process_pid {
                        summary.process_pid = discovered.process_pid;
                    }
                    summary.model = discovered.model.clone();
                    summary.preview = discovered.preview.clone();
                    summary.time_updated = discovered.time_updated;
                    summary.has_children = discovered.has_children;
                    summary.children = discovered.children.clone();
                }
                continue;
            }

            self.register_placeholder(
                discovered.cwd,
                discovered.title,
                discovered.status,
                Some(discovered.session_id),
                SessionOrigin::Discovered,
                discovered.process_pid,
                None,
                discovered.model,
                discovered.preview,
                discovered.time_updated,
                discovered.has_children,
                discovered.children,
            );
        }

        let stale_ids = self
            .sessions
            .items()
            .iter()
            .filter(|session| {
                session.origin == SessionOrigin::Discovered
                    && self.ptys.get(&session.id).is_none_or(|pty| pty.is_none())
                    && session
                        .session_id
                        .as_deref()
                        .is_none_or(|session_id| !keep_ids.contains(session_id))
            })
            .map(|session| session.id)
            .collect::<Vec<_>>();

        self.sessions
            .retain(|session| !stale_ids.contains(&session.id));
        for id in stale_ids {
            self.ptys.remove(&id);
        }
    }

    pub fn refresh_active(&mut self, rows: u16, cols: u16) -> anyhow::Result<bool> {
        let Some(active_id) = self.sessions.active_id() else {
            return Ok(false);
        };
        let summary = self
            .sessions
            .items()
            .iter()
            .find(|s| s.id == active_id)
            .cloned();
        let Some(summary) = summary else {
            return Ok(false);
        };
        let Some(session_id) = summary.session_id.as_deref() else {
            return Ok(false);
        };

        // Kill existing PTY
        if let Some(Some(pty)) = self.ptys.get_mut(&active_id) {
            let _ = pty.kill();
        }

        // Spawn fresh replica
        let pty = PtySession::spawn_replica(&summary.cwd, session_id, rows, cols)?;
        if let Some(slot) = self.ptys.get_mut(&active_id) {
            *slot = Some(pty);
        }

        Ok(true)
    }

    pub fn shutdown_local_ptys(&mut self) {
        // Only kill PTY clients (TUI viewers), NOT serve daemons.
        // Serve daemons persist in the background for session continuity.
        for pty in self.ptys.values_mut().filter_map(Option::as_mut) {
            let _ = pty.kill();
        }
    }

    pub fn managed_session_ids(&self) -> Vec<String> {
        self.sessions
            .items()
            .iter()
            .filter(|session| session.origin == SessionOrigin::Managed)
            .filter_map(|session| session.session_id.clone())
            .collect()
    }

    #[doc(hidden)]
    pub fn insert_pty_for_session(&mut self, id: u64, pty: PtySession) {
        self.ptys.insert(id, Some(pty));
    }
}

fn convert_child(child: &ChildSessionInfo) -> ChildSidebarEntry {
    ChildSidebarEntry {
        session_id: child.session_id.clone(),
        cwd: child.cwd.clone(),
        title: child.title.clone(),
        status: child.status,
        time_updated: child.time_updated,
        has_children: child.has_children,
        children: child.children.iter().map(convert_child).collect(),
    }
}
