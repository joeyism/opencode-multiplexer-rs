use std::path::PathBuf;

use opencode_multiplexer::data::db::models::DbProject;
use opencode_multiplexer::data::discovery::{
    find_best_project,
    ps::{ParsedProcess, ParsedServeProcess, parse_process_line, parse_serve_process_line},
};

#[test]
fn parses_bare_opencode_process() {
    let parsed = parse_process_line("12345 opencode").unwrap();

    assert_eq!(
        parsed,
        ParsedProcess {
            pid: 12345,
            session_id: None,
        }
    );
}

#[test]
fn parses_opencode_process_with_session_flag() {
    let parsed = parse_process_line("12345 opencode -s sess_abc123").unwrap();

    assert_eq!(parsed.pid, 12345);
    assert_eq!(parsed.session_id.as_deref(), Some("sess_abc123"));
}

#[test]
fn parses_node_wrapped_opencode_process() {
    let parsed =
        parse_process_line("12345 node /opt/homebrew/bin/opencode -s sess_wrapped").unwrap();

    assert_eq!(parsed.pid, 12345);
    assert_eq!(parsed.session_id.as_deref(), Some("sess_wrapped"));
}

#[test]
fn parses_standalone_dot_opencode_process() {
    let parsed =
        parse_process_line("35434 /Users/joey/.nvm/versions/node/v23.10.0/lib/node_modules/opencode-ai/bin/.opencode -s ses_123").unwrap();

    assert_eq!(parsed.pid, 35434);
    assert_eq!(parsed.session_id.as_deref(), Some("ses_123"));
}

#[test]
fn parses_bare_dot_opencode_process() {
    let parsed = parse_process_line("35434 .opencode").unwrap();

    assert_eq!(parsed.pid, 35434);
    assert_eq!(parsed.session_id, None);
}

#[test]
fn prefers_longest_matching_project_worktree() {
    let projects = vec![
        DbProject {
            id: "root".into(),
            worktree: PathBuf::from("/Users/joey/Programming"),
        },
        DbProject {
            id: "nested".into(),
            worktree: PathBuf::from("/Users/joey/Programming/client"),
        },
    ];

    let matched = find_best_project(
        PathBuf::from("/Users/joey/Programming/client/app").as_path(),
        &projects,
    )
    .unwrap();

    assert_eq!(matched.id, "nested");
}

#[test]
fn parses_opencode_serve_process_with_port() {
    let parsed = parse_serve_process_line("12345 opencode serve --port 4096").unwrap();

    assert_eq!(
        parsed,
        ParsedServeProcess {
            pid: 12345,
            port: 4096,
        }
    );
}

#[test]
fn regular_process_parser_ignores_serve_processes() {
    assert!(parse_process_line("12345 opencode serve --port 4096").is_none());
}
