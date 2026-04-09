use std::{error::Error, time::Duration};

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ocmux_rs::{
    app::{focus::AppFocus, reducer::reduce, state::AppState, Action},
    config::load_config,
    data::poller::start_poller,
    ops::worktree::create_worktree,
    ops::{fzf::pick_directory, opencode::display_title_for_cwd},
    registry::save_managed_sessions,
    terminal::manager::PtyManager,
    ui::{
        root,
        sidebar::{flatten_sidebar_entries, SidebarRowKind},
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};

const FOOTER_HEIGHT: u16 = 1;
const COLLAPSED_SIDEBAR_WIDTH: u16 = 12;

fn main() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<(), Box<dyn Error>> {
    let config = load_config().unwrap_or_default();
    let mut state = AppState::default();
    let mut manager = PtyManager::default();
    let mut footer_message: Option<String> = None;
    let (poll_tx, poll_rx) = std::sync::mpsc::channel();
    let poller = start_poller(poll_tx);

    let result = (|| -> Result<(), Box<dyn Error>> {
        loop {
            while let Ok(snapshot) = poll_rx.try_recv() {
                manager.apply_poll_snapshot(snapshot);
            }
            manager.drain_all_output();
            let entries = manager.sidebar_entries();
            let rows = flatten_sidebar_entries(&entries, &state.expanded_session_ids);
            if !rows.is_empty() && state.selected_sidebar_row >= rows.len() {
                state.selected_sidebar_row = rows.len() - 1;
            }
            let sidebar_width = if state.sidebar_collapsed {
                COLLAPSED_SIDEBAR_WIDTH
            } else {
                config.sidebar_width
            };
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
                    sidebar_width,
                    state.sidebar_collapsed,
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
                Event::Key(key) if matches!(state.focus, AppFocus::Terminal) => {
                    if let Some(pty) = manager.active_session_mut() {
                        if let Err(error) = pty.send_key(key) {
                            footer_message = Some(format!("terminal write failed: {error}"));
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
                Event::Resize(width, height) => {
                    let (rows, cols) = pane_size(
                        ratatui::layout::Rect::new(0, 0, width, height),
                        sidebar_width,
                    );
                    if let Err(error) = manager.resize_active(rows, cols) {
                        footer_message = Some(format!("resize failed: {error}"));
                    }
                }
                _ => {}
            }
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
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    let picked = pick_directory();

    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    terminal.clear()?;

    Ok(picked?)
}

fn prompt_text_with_terminal(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    prompt: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    use std::io::Write;
    print!("{} ", prompt);
    std::io::stdout().flush()?;

    let mut input = String::new();
    let result = std::io::stdin().read_line(&mut input);

    execute!(terminal.backend_mut(), EnterAlternateScreen)?;
    enable_raw_mode()?;
    terminal.clear()?;

    match result {
        Ok(0) => Ok(None),
        Ok(_) => {
            let trimmed = input.trim().to_string();
            Ok(Some(trimmed))
        }
        Err(_) => Ok(None),
    }
}
