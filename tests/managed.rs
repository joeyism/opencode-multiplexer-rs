use std::{collections::HashSet, path::PathBuf};

use opencode_multiplexer::{
    app::sessions::{SessionList, SessionOrigin, SessionStatus},
    data::poller::{DiscoveredSessionInfo, PollSnapshot},
    ops::opencode::{build_managed_session_command, build_replica_command, display_title_for_cwd},
    terminal::{manager::PtyManager, pty::PtySession},
    ui::sidebar::flatten_sidebar_entries,
};
use portable_pty::CommandBuilder;

#[test]
fn flatten_sidebar_entries_hides_and_shows_children_based_on_expansion() {
    let mut manager = PtyManager::default();
    manager.apply_poll_snapshot(PollSnapshot {
        sessions: vec![DiscoveredSessionInfo {
            session_id: "parent".into(),
            cwd: PathBuf::from("/tmp/parent"),
            title: "parent".into(),
            status: SessionStatus::Idle,
            process_pid: Some(1),
            model: None,
            preview: None,
            time_updated: None,
            has_children: true,
            children: vec![opencode_multiplexer::data::poller::ChildSessionInfo {
                session_id: "child".into(),
                cwd: PathBuf::from("/tmp/parent"),
                title: "child".into(),
                status: SessionStatus::NeedsInput,
                time_updated: None,
                has_children: false,
                children: vec![],
            }],
        }],
    });

    let entries = manager.sidebar_entries();
    let collapsed = flatten_sidebar_entries(&entries, &HashSet::new());
    assert_eq!(collapsed.len(), 1);

    let expanded = flatten_sidebar_entries(&entries, &HashSet::from([String::from("parent")]));
    assert_eq!(expanded.len(), 2);
    assert_eq!(expanded[1].depth, 1);
    assert_eq!(expanded[1].status, SessionStatus::NeedsInput);
}

#[test]
fn first_session_becomes_active_and_selected() {
    let mut sessions = SessionList::default();

    let first = sessions.push(
        PathBuf::from("/tmp/project-a"),
        "project-a".into(),
        SessionStatus::Idle,
        None,
        SessionOrigin::Managed,
        None,
        None,
        None,
        None,
        None,
        false,
        vec![],
    );
    let second = sessions.push(
        PathBuf::from("/tmp/project-b"),
        "project-b".into(),
        SessionStatus::Idle,
        None,
        SessionOrigin::Managed,
        None,
        None,
        None,
        None,
        None,
        false,
        vec![],
    );

    assert_eq!(sessions.active_id(), Some(first));
    assert_eq!(sessions.selected_id(), Some(first));
    assert_ne!(Some(second), sessions.active_id());
}

#[test]
fn selecting_next_and_activating_switches_active_session() {
    let mut sessions = SessionList::default();

    sessions.push(
        PathBuf::from("/tmp/project-a"),
        "project-a".into(),
        SessionStatus::Idle,
        None,
        SessionOrigin::Managed,
        None,
        None,
        None,
        None,
        None,
        false,
        vec![],
    );
    let second = sessions.push(
        PathBuf::from("/tmp/project-b"),
        "project-b".into(),
        SessionStatus::Working,
        None,
        SessionOrigin::Managed,
        None,
        None,
        None,
        None,
        None,
        false,
        vec![],
    );

    sessions.select_next();
    sessions.activate_selected();

    assert_eq!(sessions.active_id(), Some(second));
    assert_eq!(sessions.selected_id(), Some(second));
}

#[test]
fn confirming_kill_removes_selected_and_promotes_neighbor() {
    let mut sessions = SessionList::default();

    let first = sessions.push(
        PathBuf::from("/tmp/project-a"),
        "project-a".into(),
        SessionStatus::Idle,
        None,
        SessionOrigin::Managed,
        None,
        None,
        None,
        None,
        None,
        false,
        vec![],
    );
    let second = sessions.push(
        PathBuf::from("/tmp/project-b"),
        "project-b".into(),
        SessionStatus::Working,
        None,
        SessionOrigin::Managed,
        None,
        None,
        None,
        None,
        None,
        false,
        vec![],
    );

    sessions.select_next();
    sessions.request_kill_selected();
    let killed = sessions.confirm_kill();

    assert_eq!(killed, Some(second));
    assert_eq!(sessions.active_id(), Some(first));
    assert_eq!(sessions.selected_id(), Some(first));
    assert_eq!(sessions.len(), 1);
}

#[test]
fn managed_session_command_uses_opencode_binary_in_target_directory() {
    let cwd = PathBuf::from("/tmp/example-repo");
    let command = build_managed_session_command(&cwd);

    assert_eq!(command.get_argv()[0].to_string_lossy(), "opencode");
    assert_eq!(
        command.get_cwd().map(|p| p.to_string_lossy().to_string()),
        Some(cwd.display().to_string())
    );
}

#[test]
fn replica_command_uses_session_flag() {
    let cwd = PathBuf::from("/tmp/example-repo");
    let command = build_replica_command(&cwd, "sess_123");

    assert_eq!(command.get_argv()[0].to_string_lossy(), "opencode");
    assert_eq!(command.get_argv()[1].to_string_lossy(), "-s");
    assert_eq!(command.get_argv()[2].to_string_lossy(), "sess_123");
}

#[test]
fn manager_can_attach_arbitrary_session() {
    let mut manager = PtyManager::default();
    let result = manager.attach_arbitrary_session(
        "sess_xyz".into(),
        PathBuf::from("/tmp/xyz"),
        "Arbitrary".into(),
        SessionStatus::Idle,
        Some(1234567890),
        24,
        80,
    );

    if let Err(e) = &result {
        let err_str = e.to_string();
        if err_str.contains("No such file or directory")
            || err_str.contains("not found")
            || err_str.contains("No viable candidates found in PATH")
            || err_str.contains("The system cannot find the file specified")
        {
            return;
        }
    }
    result.unwrap();

    let _active = manager.active_session().unwrap();
    let summary = manager.selected_summary().unwrap();

    assert_eq!(summary.session_id.as_deref(), Some("sess_xyz"));
    assert_eq!(summary.title, "Arbitrary");
    assert_eq!(manager.len(), 1);
}

#[test]
fn cwd_title_uses_directory_name() {
    assert_eq!(
        display_title_for_cwd(PathBuf::from("/tmp/example-repo").as_path()),
        "example-repo"
    );
}

#[test]
fn pty_manager_kill_selected_updates_active_session() {
    let mut manager = PtyManager::default();
    let first = manager.register_placeholder(
        PathBuf::from("/tmp/project-a"),
        "project-a".into(),
        SessionStatus::Idle,
        None,
        SessionOrigin::Managed,
        None,
        None,
        None,
        None,
        None,
        false,
        vec![],
    );
    let second = manager.register_placeholder(
        PathBuf::from("/tmp/project-b"),
        "project-b".into(),
        SessionStatus::Working,
        None,
        SessionOrigin::Managed,
        None,
        None,
        None,
        None,
        None,
        false,
        vec![],
    );

    manager.select_next();
    manager.activate_selected();
    let killed = manager.kill_selected_placeholder();

    assert_eq!(killed, Some(second));
    assert_eq!(manager.active_id(), Some(first));
    assert_eq!(manager.selected_id(), Some(first));
}

#[test]
fn applying_poll_snapshot_adds_and_updates_discovered_sessions() {
    let mut manager = PtyManager::default();

    manager.apply_poll_snapshot(PollSnapshot {
        sessions: vec![DiscoveredSessionInfo {
            session_id: "sess_discovered".into(),
            cwd: PathBuf::from("/tmp/discovered"),
            title: "discovered".into(),
            status: SessionStatus::NeedsInput,
            process_pid: Some(42),
            model: Some("gpt-5".into()),
            preview: Some("need answer".into()),
            time_updated: None,
            has_children: true,
            children: vec![],
        }],
    });

    let summary = manager.selected_summary().unwrap();
    assert_eq!(summary.session_id.as_deref(), Some("sess_discovered"));
    assert_eq!(summary.origin, SessionOrigin::Discovered);
    assert_eq!(summary.status, SessionStatus::NeedsInput);
    assert_eq!(summary.model.as_deref(), Some("gpt-5"));
    assert!(summary.has_children);
}

#[test]
fn applying_poll_snapshot_removes_stale_discovered_placeholders() {
    let mut manager = PtyManager::default();
    manager.apply_poll_snapshot(PollSnapshot {
        sessions: vec![DiscoveredSessionInfo {
            session_id: "sess_old".into(),
            cwd: PathBuf::from("/tmp/old"),
            title: "old".into(),
            status: SessionStatus::Idle,
            process_pid: Some(11),
            model: None,
            preview: None,
            time_updated: None,
            has_children: false,
            children: vec![],
        }],
    });

    manager.apply_poll_snapshot(PollSnapshot { sessions: vec![] });

    assert!(manager.is_empty());
}

#[test]
fn sidebar_entries_include_child_sessions() {
    let mut manager = PtyManager::default();
    manager.apply_poll_snapshot(PollSnapshot {
        sessions: vec![DiscoveredSessionInfo {
            session_id: "parent".into(),
            cwd: PathBuf::from("/tmp/parent"),
            title: "parent".into(),
            status: SessionStatus::Working,
            process_pid: Some(7),
            model: None,
            preview: None,
            time_updated: None,
            has_children: true,
            children: vec![opencode_multiplexer::data::poller::ChildSessionInfo {
                session_id: "child".into(),
                cwd: PathBuf::from("/tmp/parent"),
                title: "child".into(),
                status: SessionStatus::NeedsInput,
                time_updated: None,
                has_children: false,
                children: vec![],
            }],
        }],
    });

    let entries = manager.sidebar_entries();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].has_children);
    assert_eq!(entries[0].children.len(), 1);
    assert_eq!(entries[0].children[0].status, SessionStatus::NeedsInput);
}

#[test]
fn sidebar_entries_sort_top_level_sessions_by_recent_update_first() {
    let mut manager = PtyManager::default();

    let older = manager.register_placeholder(
        PathBuf::from("/tmp/project-older"),
        "older".into(),
        SessionStatus::Idle,
        Some("sess_old".into()),
        SessionOrigin::Discovered,
        None,
        None,
        None,
        None,
        Some(100),
        false,
        vec![],
    );
    let newer = manager.register_placeholder(
        PathBuf::from("/tmp/project-newer"),
        "newer".into(),
        SessionStatus::Idle,
        Some("sess_new".into()),
        SessionOrigin::Discovered,
        None,
        None,
        None,
        None,
        Some(200),
        false,
        vec![],
    );

    let entries = manager.sidebar_entries();

    assert_eq!(entries[0].top_level_id, newer);
    assert_eq!(entries[1].top_level_id, older);
}

#[test]
fn reap_exited_ptys_clears_dead_slot_keeps_entry() {
    let mut manager = PtyManager::default();

    // Register a placeholder session
    let id = manager.register_placeholder(
        PathBuf::from("/tmp/test"),
        "test".into(),
        SessionStatus::Working,
        None,
        SessionOrigin::Managed,
        None,
        None,
        None,
        None,
        None,
        false,
        vec![],
    );

    // Spawn a short-lived process that exits immediately
    #[cfg(unix)]
    let (shell, arg) = (std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()), "-c");
    #[cfg(windows)]
    let (shell, arg) = ("cmd.exe".to_string(), "/c");

    let mut cmd = CommandBuilder::new(shell);
    cmd.args([arg, "exit 0"]);
    let pty = PtySession::spawn_test_command(cmd, 24, 80).expect("spawn test command");
    manager.insert_pty_for_session(id, pty);

    // Activate so active_session() returns something initially
    manager.select_top_level(id);
    manager.activate_selected();
    assert!(
        manager.active_session().is_some(),
        "PTY should be active before exit"
    );

    // Wait briefly for the child to exit
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Reap should detect the dead child
    let exited = manager.reap_exited_ptys();
    assert!(exited.contains(&id), "should report exited session id");

    // PTY slot cleared but sidebar entry preserved
    assert!(
        manager.active_session().is_none(),
        "PTY should be cleared after reap"
    );
    assert_eq!(manager.len(), 1, "sidebar entry should be preserved");

    // Second call returns empty — nothing left to reap
    let exited_again = manager.reap_exited_ptys();
    assert!(exited_again.is_empty(), "second reap should find nothing");
}

#[test]
fn find_by_process_pid_matches_serve_pid() {
    let mut sessions = SessionList::default();

    let id = sessions.push(
        PathBuf::from("/tmp/project"),
        "project".into(),
        SessionStatus::Idle,
        Some("sess_1".into()),
        SessionOrigin::Managed,
        Some(200),
        Some(100),
        None,
        None,
        None,
        false,
        vec![],
    );

    assert_eq!(sessions.find_by_process_pid(200), Some(id));
    assert_eq!(sessions.find_by_process_pid(100), Some(id));
    assert_eq!(sessions.find_by_process_pid(999), None);
}

#[test]
fn apply_poll_snapshot_updates_via_serve_pid() {
    let mut manager = PtyManager::default();

    let _id = manager.register_placeholder(
        PathBuf::from("/tmp/project"),
        "project".into(),
        SessionStatus::Idle,
        None,
        SessionOrigin::Managed,
        Some(200),
        Some(100),
        None,
        None,
        None,
        false,
        vec![],
    );

    manager.apply_poll_snapshot(PollSnapshot {
        sessions: vec![DiscoveredSessionInfo {
            session_id: "sess_correct".into(),
            cwd: PathBuf::from("/tmp/project"),
            title: "Correct Title".into(),
            status: SessionStatus::Working,
            process_pid: Some(100), // serve PID
            model: None,
            preview: None,
            time_updated: None,
            has_children: false,
            children: vec![],
        }],
    });

    let summary = manager.selected_summary().unwrap();
    assert_eq!(summary.session_id.as_deref(), Some("sess_correct"));
    assert_eq!(summary.title, "Correct Title");
    assert_eq!(summary.status, SessionStatus::Working);
    assert_eq!(
        summary.process_pid,
        Some(200),
        "process_pid should remain the TUI PID"
    );
    assert_eq!(summary.serve_pid, Some(100));
}
