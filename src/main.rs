use std::{
    error::Error,
    time::{Duration, Instant},
};

use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ocmux_rs::{
    app::{
        conversation::ConversationViewState, focus::AppFocus, reducer::reduce, state::AppState,
        Action,
    },
    config::load_config,
    data::{db::reader::DbReader, poller::start_poller},
    ops::worktree::create_worktree,
    ops::{
        fzf::{pick_directory, pick_session},
        opencode::display_title_for_cwd,
    },
    registry::save_managed_sessions,
    terminal::manager::PtyManager,
    ui::{
        conversation, root,
        sidebar::{flatten_sidebar_entries, SidebarRowKind},
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::path::PathBuf;

const FOOTER_HEIGHT: u16 = 2;
const COLLAPSED_SIDEBAR_WIDTH: u16 = 12;

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableBracketedPaste,
        crossterm::event::EnableFocusChange
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableBracketedPaste,
        crossterm::event::DisableFocusChange
    )?;
    terminal.show_cursor()?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<(), Box<dyn Error>> {
    let config = load_config().unwrap_or_default();
    let _ = ocmux_rs::registry::cleanup_stale_serve_entries();
    let mut state = AppState::default();
    let mut manager = PtyManager::default();
    let mut footer_message: Option<String> = None;
    let mut conversation = ConversationViewState::default();
    let (poll_tx, poll_rx) = std::sync::mpsc::channel();
    let poller = start_poller(poll_tx);

    let mut prev_selected_kind: Option<SidebarRowKind> = None;
    let result = (|| -> Result<(), Box<dyn Error>> {
        loop {
            while let Ok(snapshot) = poll_rx.try_recv() {
                manager.apply_poll_snapshot(snapshot);
            }

            let active_before = manager.active_id();
            let exited = manager.reap_exited_ptys();
            if active_before.is_some_and(|id| exited.contains(&id)) {
                state.focus = AppFocus::Sidebar;
                footer_message = Some("session exited".into());
            }

            manager.drain_all_output();
            let entries = manager.sidebar_entries();
            let rows = flatten_sidebar_entries(&entries, &state.expanded_session_ids);
            if !rows.is_empty() {
                if let Some(prev_kind) = prev_selected_kind.as_ref() {
                    if let Some(new_index) = rows.iter().position(|r| &r.kind == prev_kind) {
                        state.selected_sidebar_row = new_index;
                    }
                }
                if state.selected_sidebar_row >= rows.len() {
                    state.selected_sidebar_row = rows.len() - 1;
                }
            }
            let sidebar_width = if state.sidebar_collapsed {
                COLLAPSED_SIDEBAR_WIDTH
            } else {
                config.sidebar_width
            };

            let content_width = terminal.size()?.width.saturating_sub(sidebar_width);
            let viewport_height = terminal
                .size()
                .map(|s| s.height.saturating_sub(FOOTER_HEIGHT + 1))
                .unwrap_or(24) as usize;

            if state.focus == AppFocus::Conversation && conversation.should_poll(Instant::now()) {
                if let Some(session_id) = conversation.session_id().map(String::from) {
                    conversation.mark_polled(Instant::now());
                    match DbReader::open_default().and_then(|r| r.get_conversation(&session_id)) {
                        Ok(messages) => {
                            let doc = conversation::build_document(&messages, content_width);
                            conversation.replace_document(doc, viewport_height);
                            conversation.clear_error();
                        }
                        Err(e) => {
                            conversation.set_error(e.to_string());
                        }
                    }
                }
            }

            terminal.draw(|frame| {
                root::render(
                    frame,
                    state.focus,
                    state.selected_sidebar_row,
                    &rows,
                    &manager,
                    footer_message.as_deref(),
                    &config.keybindings,
                    state.show_help,
                    &state.show_files,
                    sidebar_width,
                    state.sidebar_collapsed,
                    state.app_focused,
                    &conversation,
                )
            })?;

            if !event::poll(Duration::from_millis(16))? {
                continue;
            }

            match event::read()? {
                Event::Key(key)
                    if key.code == KeyCode::Char(config.keybindings.help)
                        && !matches!(state.focus, AppFocus::Terminal) =>
                {
                    reduce(&mut state, Action::ToggleHelp)
                }
                Event::Key(key)
                    if state.show_help && matches!(key.code, KeyCode::Esc | KeyCode::Char(_)) =>
                {
                    state.show_help = false;
                }
                Event::Key(key)
                    if !state.show_files.is_empty()
                        && matches!(key.code, KeyCode::Esc | KeyCode::Char(_)) =>
                {
                    state.show_files.clear();
                }
                Event::Key(key) if is_focus_toggle(key) => reduce(&mut state, Action::ToggleFocus),
                Event::Key(key)
                    if matches!(state.focus, AppFocus::Sidebar)
                        && manager.pending_kill().is_some() =>
                {
                    match key.code {
                        KeyCode::Char('y') => {
                            let _ = manager.kill_selected()?;
                            save_managed_sessions(manager.managed_session_ids())?;
                            footer_message = Some("killed session".into());
                        }
                        KeyCode::Char('n') | KeyCode::Esc => {
                            manager.cancel_kill();
                            footer_message = None;
                        }
                        _ => {}
                    }
                }
                Event::Key(key) if matches!(state.focus, AppFocus::Sidebar) => match key.code {
                    KeyCode::Char(c) if c == config.keybindings.quit => break,
                    KeyCode::Char(c) if c == config.keybindings.down => {
                        reduce(&mut state, Action::SelectNextRow)
                    }
                    KeyCode::Down => reduce(&mut state, Action::SelectNextRow),
                    KeyCode::Char(c) if c == config.keybindings.up => {
                        reduce(&mut state, Action::SelectPrevRow)
                    }
                    KeyCode::Up => reduce(&mut state, Action::SelectPrevRow),
                    KeyCode::Enter => {
                        let (pty_rows, pty_cols) =
                            pane_size(terminal.size()?.into(), config.sidebar_width);
                        if let Some(row) = rows.get(state.selected_sidebar_row) {
                            match &row.kind {
                                SidebarRowKind::TopLevel { top_level_id, .. } => {
                                    manager.select_top_level(*top_level_id);
                                    match manager.activate_or_attach_selected(pty_rows, pty_cols) {
                                        Ok(_) => {
                                            save_managed_sessions(manager.managed_session_ids())?;
                                            state.focus = AppFocus::Terminal;
                                            footer_message = None
                                        }
                                        Err(error) => {
                                            footer_message = Some(format!("attach failed: {error}"))
                                        }
                                    }
                                }
                                SidebarRowKind::Child { .. } => {
                                    footer_message = Some(
                                        "child rows are selectable; attach not wired yet".into(),
                                    );
                                }
                            }
                        }
                    }
                    KeyCode::Char(c) if c == config.keybindings.view => {
                        if let Some(row) = rows.get(state.selected_sidebar_row) {
                            match &row.kind {
                                SidebarRowKind::TopLevel {
                                    session_id: Some(sid),
                                    ..
                                } => {
                                    let title = row.title.clone();
                                    conversation.open(sid.clone(), title, AppFocus::Sidebar);
                                    reduce(&mut state, Action::SetFocus(AppFocus::Conversation));
                                    footer_message = None;
                                }
                                SidebarRowKind::Child { session_id } => {
                                    let title = row.title.clone();
                                    conversation.open(session_id.clone(), title, AppFocus::Sidebar);
                                    reduce(&mut state, Action::SetFocus(AppFocus::Conversation));
                                    footer_message = None;
                                }
                                _ => {
                                    footer_message = Some(
                                        "conversation view requires a session with a DB ID".into(),
                                    );
                                }
                            }
                        }
                    }
                    KeyCode::Char(c) if c == config.keybindings.files => {
                        if let Some(row) = rows.get(state.selected_sidebar_row) {
                            if let Some(sid) = row.session_id.as_deref() {
                                match DbReader::open_default()
                                    .and_then(|r| r.get_session_modified_files(sid))
                                {
                                    Ok(files) if files.is_empty() => {
                                        footer_message =
                                            Some("no files modified by this session".into());
                                    }
                                    Ok(files) => {
                                        state.show_files = files;
                                    }
                                    Err(e) => {
                                        footer_message = Some(format!("failed to read files: {e}"));
                                    }
                                }
                            } else {
                                footer_message = Some("no session ID for this row".into());
                            }
                        }
                    }
                    KeyCode::Char('/') => match pick_session_with_terminal(terminal) {
                        Ok(Some((session_id, cwd, title, status, time_updated))) => {
                            let (rows, cols) =
                                pane_size(terminal.size()?.into(), config.sidebar_width);
                            match manager.attach_arbitrary_session(
                                session_id,
                                cwd,
                                title.clone(),
                                status,
                                time_updated,
                                rows,
                                cols,
                            ) {
                                Ok(_) => {
                                    save_managed_sessions(manager.managed_session_ids())?;
                                    state.focus = AppFocus::Terminal;
                                    state.selected_sidebar_row = 0;
                                    footer_message = None;
                                }
                                Err(error) => {
                                    footer_message = Some(format!("attach failed: {error}"))
                                }
                            }
                        }
                        Ok(None) => footer_message = Some("search canceled".into()),
                        Err(error) => footer_message = Some(format!("search failed: {error}")),
                    },
                    KeyCode::Char(c) if c == config.keybindings.kill => {
                        if let Some(row) = rows.get(state.selected_sidebar_row) {
                            match row.kind {
                                SidebarRowKind::TopLevel { top_level_id, .. } => {
                                    manager.select_top_level(top_level_id);
                                    manager.request_kill_selected();
                                    if let Some(summary) = manager.selected_summary() {
                                        footer_message = Some(format!(
                                            "kill {}? y confirm / n cancel",
                                            summary.title
                                        ));
                                    }
                                }
                                SidebarRowKind::Child { .. } => {
                                    footer_message =
                                        Some("kill only supported on top-level sessions".into());
                                }
                            }
                        }
                    }
                    KeyCode::Char(c) if c == config.keybindings.spawn => {
                        match pick_directory_with_terminal(terminal) {
                            Ok(Some(cwd)) => {
                                let title = display_title_for_cwd(&cwd);
                                let (rows, cols) =
                                    pane_size(terminal.size()?.into(), config.sidebar_width);
                                match manager.spawn_managed(cwd, title.clone(), rows, cols) {
                                    Ok(_) => {
                                        save_managed_sessions(manager.managed_session_ids())?;
                                        state.focus = AppFocus::Terminal;
                                        state.selected_sidebar_row = 0;
                                        footer_message = Some(format!("spawned {title}"))
                                    }
                                    Err(error) => {
                                        footer_message = Some(format!("spawn failed: {error}"))
                                    }
                                }
                            }
                            Ok(None) => footer_message = Some("spawn canceled".into()),
                            Err(error) => footer_message = Some(format!("picker failed: {error}")),
                        }
                    }
                    KeyCode::Tab => {
                        if let Some(row) = rows.get(state.selected_sidebar_row) {
                            if row.has_children {
                                if let Some(session_id) = row.session_id.clone() {
                                    reduce(&mut state, Action::ToggleExpandSelected(session_id));
                                }
                            }
                        }
                    }
                    KeyCode::Char('r') => {
                        let (pty_rows, pty_cols) =
                            pane_size(terminal.size()?.into(), sidebar_width);
                        match manager.refresh_active(pty_rows, pty_cols) {
                            Ok(true) => footer_message = Some("refreshed session".into()),
                            Ok(false) => {
                                footer_message = Some("no active session to refresh".into())
                            }
                            Err(error) => footer_message = Some(format!("refresh failed: {error}")),
                        }
                    }

                    KeyCode::Char('c') => {
                        if let Some(row) = rows.get(state.selected_sidebar_row) {
                            match &row.kind {
                                SidebarRowKind::TopLevel {
                                    session_id: Some(sid),
                                    ..
                                } => {
                                    if let Some(cwd) = resolve_session_cwd(row) {
                                        match commit_session_files(terminal, sid, &cwd) {
                                            Ok(Some(msg)) => footer_message = Some(msg),
                                            Ok(None) => {
                                                footer_message = Some("commit canceled".into())
                                            }
                                            Err(e) => {
                                                footer_message = Some(format!("commit failed: {e}"))
                                            }
                                        }
                                    } else {
                                        footer_message = Some("session directory not found".into());
                                    }
                                }
                                _ => {
                                    footer_message =
                                        Some("commit requires a top-level session with ID".into())
                                }
                            }
                        }
                    }
                    KeyCode::Char('!') => {
                        if let Some(row) = rows.get(state.selected_sidebar_row) {
                            match &row.kind {
                                SidebarRowKind::TopLevel { .. } => {
                                    if let Some(cwd) = resolve_session_cwd(row) {
                                        match drop_to_bash(terminal, &cwd) {
                                            Ok(_) => footer_message = None,
                                            Err(error) => {
                                                footer_message =
                                                    Some(format!("bash failed: {error}"))
                                            }
                                        }
                                    } else {
                                        footer_message = Some("session directory not found".into());
                                    }
                                }
                                SidebarRowKind::Child { .. } => {
                                    footer_message = Some("bash only on top-level sessions".into());
                                }
                            }
                        }
                    }
                    KeyCode::Char('s') => {
                        reduce(&mut state, Action::ToggleSidebarCollapse);
                    }
                    KeyCode::Char(c) if c == config.keybindings.worktree => {
                        match pick_directory_with_terminal(terminal) {
                            Ok(Some(repo_dir)) => {
                                match prompt_text_with_terminal(
                                    terminal,
                                    "Branch name (empty = repo root):",
                                )? {
                                    Some(branch) if !branch.trim().is_empty() => {
                                        match create_worktree(&repo_dir, branch.trim()) {
                                            Ok(worktree_dir) => {
                                                let title = display_title_for_cwd(&worktree_dir);
                                                let (rows, cols) = pane_size(
                                                    terminal.size()?.into(),
                                                    sidebar_width,
                                                );
                                                match manager.spawn_managed(
                                                    worktree_dir,
                                                    title.clone(),
                                                    rows,
                                                    cols,
                                                ) {
                                                    Ok(_) => {
                                                        save_managed_sessions(
                                                            manager.managed_session_ids(),
                                                        )?;
                                                        footer_message = Some(format!(
                                                            "spawned worktree {title}"
                                                        ));
                                                    }
                                                    Err(error) => {
                                                        footer_message =
                                                            Some(format!("spawn failed: {error}"))
                                                    }
                                                }
                                            }
                                            Err(error) => {
                                                footer_message =
                                                    Some(format!("worktree failed: {error}"))
                                            }
                                        }
                                    }
                                    Some(_) => {
                                        let title = display_title_for_cwd(&repo_dir);
                                        let (rows, cols) = pane_size(
                                            terminal.size()?.into(),
                                            config.sidebar_width,
                                        );
                                        match manager.spawn_managed(
                                            repo_dir,
                                            title.clone(),
                                            rows,
                                            cols,
                                        ) {
                                            Ok(_) => {
                                                save_managed_sessions(
                                                    manager.managed_session_ids(),
                                                )?;
                                                footer_message = Some(format!("spawned {title}"));
                                            }
                                            Err(error) => {
                                                footer_message =
                                                    Some(format!("spawn failed: {error}"))
                                            }
                                        }
                                    }
                                    None => footer_message = Some("worktree canceled".into()),
                                }
                            }
                            Ok(None) => footer_message = Some("worktree canceled".into()),
                            Err(error) => footer_message = Some(format!("picker failed: {error}")),
                        }
                    }
                    _ => {}
                },
                Event::Key(key) if matches!(state.focus, AppFocus::Conversation) => {
                    let vp = viewport_height;
                    match key.code {
                        KeyCode::Char('k') | KeyCode::Up => {
                            conversation.scroll_up(1);
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            conversation.scroll_down(1, vp);
                        }
                        KeyCode::Char('G') => {
                            conversation.scroll_to_end(vp);
                        }
                        KeyCode::Char('g') => {
                            conversation.scroll_to_top();
                        }
                        KeyCode::PageUp | KeyCode::Char('u')
                            if key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            conversation.scroll_up(vp.saturating_sub(1));
                        }
                        KeyCode::PageDown | KeyCode::Char('d')
                            if key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            conversation.scroll_down(vp.saturating_sub(1), vp);
                        }
                        KeyCode::Char(c) if c == config.keybindings.view => {
                            let return_focus = conversation.close();
                            state.last_main_focus = AppFocus::Terminal;
                            reduce(&mut state, Action::SetFocus(return_focus));
                            footer_message = None;
                        }
                        KeyCode::Char(c) if c == config.keybindings.quit => break,
                        KeyCode::Esc => {
                            let return_focus = conversation.close();
                            state.last_main_focus = AppFocus::Terminal;
                            reduce(&mut state, Action::SetFocus(return_focus));
                            footer_message = None;
                        }
                        _ => {}
                    }
                }
                Event::Key(key) if matches!(state.focus, AppFocus::Terminal) => {
                    if let Some(pty) = manager.active_session_mut() {
                        if let Err(error) = pty.send_key(key) {
                            footer_message = Some(format!("terminal write failed: {error}"));
                        }
                    }
                }
                Event::Paste(text) if matches!(state.focus, AppFocus::Terminal) => {
                    if let Some(pty) = manager.active_session_mut() {
                        if let Err(error) = pty.send_paste(&text) {
                            footer_message = Some(format!("paste failed: {error}"));
                        }
                    }
                }
                Event::Mouse(mouse) if matches!(mouse.kind, MouseEventKind::Down(_)) => {
                    if mouse.column < config.sidebar_width {
                        state.focus = AppFocus::Sidebar;
                        let row = mouse.row.saturating_sub(1) as usize;
                        while manager.selected_index() < row
                            && manager.selected_index() + 1 < manager.len()
                        {
                            manager.select_next();
                        }
                        while manager.selected_index() > row {
                            manager.select_prev();
                        }
                    }
                }
                Event::FocusGained => {
                    state.app_focused = true;
                }
                Event::FocusLost => {
                    state.app_focused = false;
                }
                Event::Resize(width, height) => {
                    let (pty_rows, pty_cols) = pane_size(
                        ratatui::layout::Rect::new(0, 0, width, height),
                        sidebar_width,
                    );
                    if let Err(error) = manager.resize_active(pty_rows, pty_cols) {
                        footer_message = Some(format!("resize failed: {error}"));
                    }
                    conversation.clamp_scroll(height.saturating_sub(FOOTER_HEIGHT + 1) as usize);
                }
                _ => {}
            }

            prev_selected_kind = rows.get(state.selected_sidebar_row).map(|r| r.kind.clone());
        }

        Ok(())
    })();

    manager.shutdown_local_ptys();
    poller.stop();

    result
}

fn pane_size(area: ratatui::layout::Rect, sidebar_width: u16) -> (u16, u16) {
    let rows = area.height.saturating_sub(FOOTER_HEIGHT).max(1);
    let cols = area.width.saturating_sub(sidebar_width).max(1);
    (rows, cols)
}

fn is_focus_toggle(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('4') && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn pick_directory_with_terminal(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<Option<std::path::PathBuf>, Box<dyn Error>> {
    leave_tui(terminal)?;
    let picked = pick_directory();
    enter_tui(terminal)?;

    Ok(picked?)
}

fn prompt_text_with_terminal(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    prompt: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    leave_tui(terminal)?;

    use std::io::Write;
    print!("\x1b[2J\x1b[H");
    print!("{} ", prompt);
    std::io::stdout().flush()?;

    let mut input = String::new();
    let result = std::io::stdin().read_line(&mut input);

    enter_tui(terminal)?;

    match result {
        Ok(0) => Ok(None),
        Ok(_) => {
            let trimmed = input.trim().to_string();
            Ok(Some(trimmed))
        }
        Err(_) => Ok(None),
    }
}

fn pick_session_with_terminal(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<
    Option<(
        String,
        PathBuf,
        String,
        ocmux_rs::app::sessions::SessionStatus,
        Option<i64>,
    )>,
    Box<dyn Error>,
> {
    leave_tui(terminal)?;

    let reader = DbReader::open_default()?;
    let all = reader.get_all_sessions()?;

    let mut display_lines = Vec::new();
    for session in &all {
        let time_str =
            ocmux_rs::ui::sidebar::relative_time_from_updated(Some(session.time_updated));
        let repo = session
            .worktree
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        let label = format!("{}/{}", repo, session.title);
        let display = format!("{}\t{}", label, time_str);
        display_lines.push((session.id.clone(), display));
    }

    let picked = pick_session(display_lines)?;

    enter_tui(terminal)?;

    if let Some(id) = picked {
        if let Some(session) = all.iter().find(|s| s.id == id) {
            let cwd = if session.directory.as_os_str().is_empty() {
                session.worktree.clone()
            } else {
                session.directory.clone()
            };
            let reader2 = DbReader::open_default()?;
            let status = reader2.get_session_status(&id)?;
            return Ok(Some((
                id,
                cwd,
                session.title.clone(),
                status,
                Some(session.time_updated),
            )));
        }
    }

    Ok(None)
}

fn drop_to_bash(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    cwd: &std::path::Path,
) -> Result<(), Box<dyn Error>> {
    leave_tui(terminal)?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".into());
    let _ = std::process::Command::new(&shell)
        .current_dir(cwd)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status();

    enter_tui(terminal)?;
    Ok(())
}

fn commit_session_files(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    session_id: &str,
    cwd: &std::path::Path,
) -> Result<Option<String>, Box<dyn Error>> {
    let reader = ocmux_rs::data::db::reader::DbReader::open_default()?;
    let files = reader.get_session_modified_files(session_id)?;
    if files.is_empty() {
        return Ok(Some("no files modified by this session".into()));
    }

    leave_tui(terminal)?;
    print!("\x1b[2J\x1b[H");

    // Get git status for session files
    let (created, modified, deleted) = ocmux_rs::ops::git::get_file_statuses(cwd, &files)?;

    println!("Files modified by this session:\n");

    if !created.is_empty() {
        println!("\x1b[32mCreated:\x1b[0m"); // Green
        for f in &created {
            println!("\x1b[32m  {}\x1b[0m", f);
        }
        println!();
    }

    if !modified.is_empty() {
        println!("\x1b[33mModified:\x1b[0m"); // Yellow
        for f in &modified {
            println!("\x1b[33m  {}\x1b[0m", f);
        }
        println!();
    }

    if !deleted.is_empty() {
        println!("\x1b[31mDeleted:\x1b[0m"); // Red
        for f in &deleted {
            println!("\x1b[31m  {}\x1b[0m", f);
        }
        println!();
    }

    if created.is_empty() && modified.is_empty() && deleted.is_empty() {
        println!("No uncommitted changes for session files.\n");
    }

    // Prompt for commit message
    use std::io::Write;
    print!("Commit message (empty to cancel): ");
    std::io::stdout().flush()?;
    let mut message = String::new();
    std::io::stdin().read_line(&mut message)?;
    let message = message.trim().to_string();

    let result = if message.is_empty() {
        None
    } else {
        // Run commit + push, show output
        let output = ocmux_rs::ops::git::commit_and_push_files(
            cwd, &created, &modified, &deleted, &message,
        )?;
        println!("\n{}", output);
        println!("Press Enter to continue...");
        let _ = std::io::stdin().read_line(&mut String::new());
        Some(format!("committed {} files", files.len()))
    };

    enter_tui(terminal)?;
    Ok(result)
}

fn resolve_session_cwd(row: &ocmux_rs::ui::sidebar::SidebarVisibleRow) -> Option<PathBuf> {
    if !row.cwd.as_os_str().is_empty() && row.cwd.is_dir() {
        return Some(row.cwd.clone());
    }
    if let Some(sid) = row.session_id.as_deref() {
        if let Ok(reader) = ocmux_rs::data::db::reader::DbReader::open_default() {
            if let Ok(Some(session)) = reader.get_session_by_id(sid) {
                if !session.directory.as_os_str().is_empty() && session.directory.is_dir() {
                    return Some(session.directory);
                }
                if let Ok(projects) = reader.get_projects() {
                    if let Some(proj) = projects.iter().find(|p| p.id == session.project_id) {
                        if proj.worktree.is_dir() {
                            return Some(proj.worktree.clone());
                        }
                    }
                }
            }
        }
    }
    None
}

fn leave_tui(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableBracketedPaste,
        crossterm::event::DisableFocusChange
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn enter_tui(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), Box<dyn Error>> {
    execute!(
        terminal.backend_mut(),
        EnterAlternateScreen,
        EnableBracketedPaste,
        crossterm::event::EnableFocusChange
    )?;
    enable_raw_mode()?;
    terminal.clear()?;
    Ok(())
}
