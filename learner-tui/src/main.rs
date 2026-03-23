mod app;
mod theme;
mod ui;
mod widgets;
mod screens;
mod io_layer;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::collections::HashMap;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, AutoSolveMode, ConfluenceFocus, IssueFocus, Screen, SolveFocus, SolveProgress, SolveStatus, Tab};
use io_layer::oauth::{self, OAuthProvider, OAuthStatus, DeviceFlowState};
use screens::settings::SettingsAction;
use ui::PanelAreas;
use widgets::tab_bar::TabBarState;

/// Shared result channel for background OAuth flows
type OAuthResult = Arc<Mutex<Option<Result<HashMap<String, String>, String>>>>;

/// State for an active device code polling loop
struct DevicePollState {
    flow: DeviceFlowState,
    last_poll: std::time::Instant,
}

const DEFAULT_DB_PATH: &str = "./db/learnings.db";
const RESEARCH_DB_PATH: &str = "./db/research.db";
const TICK_MS: u64 = 200;
const REFRESH_INTERVAL: u64 = 25;

/// Resolve a DB path: CLI arg → known home-relative path → default fallback.
fn resolve_db_path(cli_arg: Option<String>, home_rel: &str, default: &str) -> String {
    if let Some(p) = cli_arg {
        if std::path::Path::new(&p).exists() { return p; }
    }
    if let Ok(home) = std::env::var("HOME") {
        let full = format!("{}/{}", home, home_rel);
        if std::path::Path::new(&full).exists() { return full; }
    }
    default.to_string()
}

fn main() -> io::Result<()> {
    let db_path = resolve_db_path(
        std::env::args().nth(1),
        ".openclaw/workspace-learner-agent/db/learnings.db",
        DEFAULT_DB_PATH,
    );

    let research_db_path = resolve_db_path(
        std::env::args().nth(2),
        ".openclaw/workspace-researcher-agent/db/research.db",
        RESEARCH_DB_PATH,
    );

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(db_path, research_db_path);
    let mut panel_areas = PanelAreas::default();
    let mut tab_bar_state = TabBarState::default();
    let mut tick_since_refresh: u64 = 0;

    // Background cleanup: auto-purge "No actionable" data every 5 minutes
    let (cleanup_tx, cleanup_rx) = std::sync::mpsc::channel::<(usize, usize, usize)>();
    {
        let db_path_clone = app.db_path.clone();
        let research_db_path_clone = app.research_db_path.clone();
        let tx = cleanup_tx;
        std::thread::spawn(move || {
            loop {
                let n_learn = io_layer::db::delete_no_actionable_learnings(&db_path_clone).unwrap_or(0);
                let (n_issues, n_sols) = io_layer::db::delete_no_actionable_research(&research_db_path_clone).unwrap_or((0, 0));
                let _ = tx.send((n_learn, n_issues, n_sols));
                std::thread::sleep(std::time::Duration::from_secs(300));
            }
        });
    }

    // OAuth background state
    let oauth_result: OAuthResult = Arc::new(Mutex::new(None));
    let mut device_poll: Option<DevicePollState> = None;

    terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;

    loop {
        if event::poll(Duration::from_millis(TICK_MS))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press { continue; }

                    // Overlay key handler — takes priority over everything
                    if app.overlay.is_some() {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => { app.overlay = None; }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if let Some(ref mut ov) = app.overlay { ov.scroll_up(1); }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if let Some(ref mut ov) = app.overlay { ov.scroll_down(3); }
                            }
                            KeyCode::PageUp => {
                                if let Some(ref mut ov) = app.overlay { ov.scroll_up(10); }
                            }
                            KeyCode::PageDown => {
                                if let Some(ref mut ov) = app.overlay { ov.scroll_down(10); }
                            }
                            _ => {}
                        }
                        terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        continue; // Skip all other key handling when overlay is open
                    }

                    // Welcome screen captures all input
                    if app.screen == Screen::Welcome {
                        match key.code {
                            KeyCode::Enter => {
                                app.screen = Screen::Main;
                                app.set_tab(Tab::Portal);
                                terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            }
                            KeyCode::Char('q') => break,
                            _ => {}
                        }
                        continue;
                    }

                    // Portal tab captures input for its focus chain
                    if app.current_tab == Tab::Portal {
                        let mut handled = true;
                        match key.code {
                            KeyCode::Tab => {
                                app.portal.advance_focus();
                                let visible_h = terminal.size().map(|s| s.height.saturating_sub(5)).unwrap_or(20);
                                app.portal.scroll_to_focus(visible_h);
                            }
                            KeyCode::BackTab => {
                                app.portal.retreat_focus();
                                let visible_h = terminal.size().map(|s| s.height.saturating_sub(5)).unwrap_or(20);
                                app.portal.scroll_to_focus(visible_h);
                            }
                            KeyCode::Enter => {
                                // If on a save button, save that section
                                if let Some(section) = app.portal.focused_save_section() {
                                    save_portal_section(&app, section);
                                    app.portal.status_message = Some(("Saved!".to_string(), true));
                                    app.portal.status_tick = app.tick_count;
                                }
                                // If on a dropdown, toggle it
                                if app.portal.has_focused_dropdown() {
                                    if let Some(dd) = app.portal.focused_dropdown_mut() {
                                        dd.toggle();
                                    }
                                }
                                // If on an OAuth button, start the flow
                                if let Some(provider) = app.portal.focused_oauth_provider() {
                                    match provider {
                                        OAuthProvider::Google | OAuthProvider::Dropbox => {
                                            // Localhost redirect flow in background thread
                                            app.portal.oauth_status = OAuthStatus::WaitingForBrowser;
                                            let result_clone = Arc::clone(&oauth_result);
                                            let provider_clone = provider.clone();
                                            // Use client_id from the corresponding env key, or prompt
                                            let client_id = get_client_id_for_provider(&app, &provider_clone);
                                            if client_id.is_empty() {
                                                app.portal.oauth_status = OAuthStatus::Error(
                                                    format!("Set {}_CLIENT_ID in .env first", provider_clone.env_prefix())
                                                );
                                            } else {
                                                let client_secret = get_client_secret_for_provider(&app, &provider_clone);
                                                std::thread::spawn(move || {
                                                    let secret_ref = if client_secret.is_empty() { None } else { Some(client_secret.as_str()) };
                                                    let result = oauth::localhost_redirect_flow(&provider_clone, &client_id, secret_ref);
                                                    if let Ok(mut guard) = result_clone.lock() {
                                                        *guard = Some(result);
                                                    }
                                                });
                                            }
                                        }
                                        OAuthProvider::Microsoft => {
                                            // Device code flow
                                            let client_id = get_client_id_for_provider(&app, &provider);
                                            if client_id.is_empty() {
                                                app.portal.oauth_status = OAuthStatus::Error(
                                                    "Set MICROSOFT_CLIENT_ID in .env first".to_string()
                                                );
                                            } else {
                                                match oauth::start_device_flow(&client_id) {
                                                    Ok(flow) => {
                                                        app.portal.oauth_status = OAuthStatus::WaitingForDevice {
                                                            url: flow.verification_uri.clone(),
                                                            code: flow.user_code.clone(),
                                                        };
                                                        // Try to open the browser to the verification URL
                                                        let _ = open::that(&flow.verification_uri);
                                                        device_poll = Some(DevicePollState {
                                                            flow,
                                                            last_poll: std::time::Instant::now(),
                                                        });
                                                    }
                                                    Err(e) => {
                                                        app.portal.oauth_status = OAuthStatus::Error(e);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            KeyCode::Esc => {
                                // Close dropdown if expanded, otherwise unfocus
                                if app.portal.has_focused_dropdown() {
                                    if let Some(dd) = app.portal.focused_dropdown_mut() {
                                        if dd.expanded { dd.expanded = false; }
                                    }
                                }
                            }
                            _ => { handled = false; }
                        }

                        if !handled {
                            // If a text input is focused, route character keys to it
                            if app.portal.has_focused_input() {
                                handled = true;
                                match key.code {
                                    KeyCode::Char(c) => {
                                        if let Some(input) = app.portal.focused_input_mut() {
                                            input.insert_char(c);
                                        }
                                    }
                                    KeyCode::Backspace => {
                                        if let Some(input) = app.portal.focused_input_mut() {
                                            input.delete_char_before();
                                        }
                                    }
                                    KeyCode::Delete => {
                                        if let Some(input) = app.portal.focused_input_mut() {
                                            input.delete_char_at();
                                        }
                                    }
                                    KeyCode::Left => {
                                        if let Some(input) = app.portal.focused_input_mut() {
                                            input.move_cursor_left();
                                        }
                                    }
                                    KeyCode::Right => {
                                        if let Some(input) = app.portal.focused_input_mut() {
                                            input.move_cursor_right();
                                        }
                                    }
                                    KeyCode::Home => {
                                        if let Some(input) = app.portal.focused_input_mut() {
                                            input.move_cursor_home();
                                        }
                                    }
                                    KeyCode::End => {
                                        if let Some(input) = app.portal.focused_input_mut() {
                                            input.move_cursor_end();
                                        }
                                    }
                                    _ => { handled = false; }
                                }
                            }

                            // If a dropdown is focused and expanded, handle arrow keys
                            if !handled && app.portal.has_focused_dropdown() {
                                let expanded = app.portal.focused_dropdown_mut().map(|d| d.expanded).unwrap_or(false);
                                if expanded {
                                    handled = true;
                                    match key.code {
                                        KeyCode::Up => {
                                            if let Some(dd) = app.portal.focused_dropdown_mut() { dd.select_prev(); }
                                        }
                                        KeyCode::Down => {
                                            if let Some(dd) = app.portal.focused_dropdown_mut() { dd.select_next(); }
                                        }
                                        _ => { handled = false; }
                                    }
                                }
                            }

                            // Portal scroll: Up/Down/PgUp/PgDn when no input or expanded dropdown
                            if !handled && !app.portal.has_focused_input()
                                && !app.portal.has_focused_dropdown()
                            {
                                let visible_h = terminal.size().map(|s| s.height.saturating_sub(5)).unwrap_or(20);
                                match key.code {
                                    KeyCode::Up => {
                                        app.portal.scroll_up(3);
                                        handled = true;
                                    }
                                    KeyCode::Down => {
                                        app.portal.scroll_down(3, visible_h);
                                        handled = true;
                                    }
                                    KeyCode::PageUp => {
                                        app.portal.scroll_up(10);
                                        handled = true;
                                    }
                                    KeyCode::PageDown => {
                                        app.portal.scroll_down(10, visible_h);
                                        handled = true;
                                    }
                                    _ => {}
                                }
                            }
                        }

                        if handled {
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }

                        // Guard: don't let 'q' or single-char keys quit when input is focused
                        if app.portal.has_focused_input() {
                            // Swallow any unhandled key presses to prevent quit
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }
                    }

                    // Issues tab: search mode captures all input first
                    if app.current_tab == Tab::Issues && app.issues_state.search.active {
                        let mut handled = true;
                        match key.code {
                            KeyCode::Esc => {
                                app.issues_state.search.deactivate();
                            }
                            KeyCode::Enter => {
                                // Jump to selected search result
                                if let Some(result_idx) = app.issues_state.search.selected_result_index() {
                                    // result_idx is the index in the original issues list
                                    // Find it in filtered_indices
                                    if let Some(pos) = app.issues_state.filtered_indices.iter().position(|&fi| fi == result_idx) {
                                        app.issues_state.selected_index = pos;
                                        app.issues_state.list_state.select(Some(pos));
                                        app.issues_state.detail_scroll = 0;
                                    }
                                }
                                app.issues_state.search.deactivate();
                            }
                            KeyCode::Up => {
                                app.issues_state.search.select_prev();
                            }
                            KeyCode::Down => {
                                app.issues_state.search.select_next();
                            }
                            KeyCode::Backspace => {
                                app.issues_state.search.delete_char_before();
                                let items: Vec<(usize, String)> = app.issues_state.issues.iter().enumerate()
                                    .map(|(i, issue)| (i, issue.title.clone()))
                                    .collect();
                                let item_refs: Vec<(usize, &str)> = items.iter().map(|(i, s)| (*i, s.as_str())).collect();
                                app.issues_state.search.update_results(&item_refs);
                            }
                            KeyCode::Char(c) => {
                                app.issues_state.search.insert_char(c);
                                let items: Vec<(usize, String)> = app.issues_state.issues.iter().enumerate()
                                    .map(|(i, issue)| (i, issue.title.clone()))
                                    .collect();
                                let item_refs: Vec<(usize, &str)> = items.iter().map(|(i, s)| (*i, s.as_str())).collect();
                                app.issues_state.search.update_results(&item_refs);
                            }
                            _ => { handled = false; }
                        }
                        if handled {
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }
                    }

                    // Issues tab captures input for focus navigation and filter dropdowns
                    if app.current_tab == Tab::Issues {
                        let mut handled = true;
                        match key.code {
                            KeyCode::Char('/') => {
                                app.issues_state.search.activate();
                            }
                            KeyCode::Left => {
                                // Move focus area left: Detail -> List -> Filters
                                match app.issues_state.focus {
                                    IssueFocus::Detail => app.issues_state.focus = IssueFocus::List,
                                    IssueFocus::List => app.issues_state.focus = IssueFocus::Filters,
                                    IssueFocus::Filters => {} // already leftmost
                                }
                            }
                            KeyCode::Right => {
                                // Move focus area right: Filters -> List -> Detail
                                match app.issues_state.focus {
                                    IssueFocus::Filters => {
                                        app.issues_state.collapse_all_dropdowns();
                                        app.issues_state.focus = IssueFocus::List;
                                    }
                                    IssueFocus::List => app.issues_state.focus = IssueFocus::Detail,
                                    IssueFocus::Detail => {} // already rightmost
                                }
                            }
                            KeyCode::Up => {
                                match app.issues_state.focus {
                                    IssueFocus::Filters => {
                                        if app.issues_state.any_dropdown_expanded() {
                                            app.issues_state.active_dropdown_mut().select_prev();
                                        } else {
                                            // Move between filter dropdowns
                                            if app.issues_state.active_filter > 0 {
                                                app.issues_state.active_filter -= 1;
                                            }
                                        }
                                    }
                                    IssueFocus::List => {
                                        app.issues_state.scroll_list(-1);
                                    }
                                    IssueFocus::Detail => {
                                        app.issues_state.detail_scroll = app.issues_state.detail_scroll.saturating_sub(1);
                                    }
                                }
                            }
                            KeyCode::Down => {
                                match app.issues_state.focus {
                                    IssueFocus::Filters => {
                                        if app.issues_state.any_dropdown_expanded() {
                                            app.issues_state.active_dropdown_mut().select_next();
                                        } else {
                                            if app.issues_state.active_filter < 2 {
                                                app.issues_state.active_filter += 1;
                                            }
                                        }
                                    }
                                    IssueFocus::List => {
                                        app.issues_state.scroll_list(1);
                                    }
                                    IssueFocus::Detail => {
                                        app.issues_state.detail_scroll += 1;
                                    }
                                }
                            }
                            KeyCode::Enter => {
                                match app.issues_state.focus {
                                    IssueFocus::Filters => {
                                        let dd = app.issues_state.active_dropdown_mut();
                                        if dd.expanded {
                                            dd.expanded = false;
                                            // Re-apply filters after selection
                                            app.issues_state.apply_filters();
                                        } else {
                                            dd.toggle();
                                        }
                                    }
                                    IssueFocus::List => {
                                        if !app.issues_state.search.active {
                                            let idx = app.issues_state.selected_index;
                                            if let Some(&real_idx) = app.issues_state.filtered_indices.get(idx) {
                                                if let Some(issue) = app.issues_state.issues.get(real_idx) {
                                                    let cluster = issue.cluster_name.as_deref().unwrap_or("—");
                                                    let content = format!(
                                                        "# {}\n\n**Category:** {} | **Severity:** {} | **Status:** {}\n**Cluster:** {} | **Solutions:** {} | Created: {}\n\n---\n\n{}",
                                                        issue.title, issue.category, issue.severity, issue.status,
                                                        cluster, issue.solution_count, issue.created_at, issue.description
                                                    );
                                                    let title = issue.title.clone();
                                                    open_mdr(&mut app, &title, &content);
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            KeyCode::Esc => {
                                if app.issues_state.any_dropdown_expanded() {
                                    app.issues_state.collapse_all_dropdowns();
                                }
                            }
                            _ => { handled = false; }
                        }

                        if handled {
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }
                    }

                    // Solutions tab: search mode captures all input first
                    if app.current_tab == Tab::Solutions && app.solutions_state.search.active {
                        let mut handled = true;
                        match key.code {
                            KeyCode::Esc => {
                                app.solutions_state.search.deactivate();
                            }
                            KeyCode::Enter => {
                                if let Some(result_idx) = app.solutions_state.search.selected_result_index() {
                                    app.solutions_state.selected_index = result_idx;
                                    app.solutions_state.list_state.select(Some(result_idx));
                                    app.solutions_state.detail_scroll = 0;
                                }
                                app.solutions_state.search.deactivate();
                            }
                            KeyCode::Up => {
                                app.solutions_state.search.select_prev();
                            }
                            KeyCode::Down => {
                                app.solutions_state.search.select_next();
                            }
                            KeyCode::Backspace => {
                                app.solutions_state.search.delete_char_before();
                                let items: Vec<(usize, String)> = app.solutions_state.solutions.iter().enumerate()
                                    .map(|(i, sol)| (i, format!("{} {}", sol.issue_title, sol.summary)))
                                    .collect();
                                let item_refs: Vec<(usize, &str)> = items.iter().map(|(i, s)| (*i, s.as_str())).collect();
                                app.solutions_state.search.update_results(&item_refs);
                            }
                            KeyCode::Char(c) => {
                                app.solutions_state.search.insert_char(c);
                                let items: Vec<(usize, String)> = app.solutions_state.solutions.iter().enumerate()
                                    .map(|(i, sol)| (i, format!("{} {}", sol.issue_title, sol.summary)))
                                    .collect();
                                let item_refs: Vec<(usize, &str)> = items.iter().map(|(i, s)| (*i, s.as_str())).collect();
                                app.solutions_state.search.update_results(&item_refs);
                            }
                            _ => { handled = false; }
                        }
                        if handled {
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }
                    }

                    // Solutions tab captures input for list navigation
                    if app.current_tab == Tab::Solutions {
                        let mut handled = true;
                        match key.code {
                            KeyCode::Char('/') => {
                                app.solutions_state.search.activate();
                            }
                            KeyCode::Up => {
                                app.solutions_state.scroll_list(-1);
                            }
                            KeyCode::Down => {
                                app.solutions_state.scroll_list(1);
                            }
                            KeyCode::Enter => {
                                if !app.solutions_state.search.active {
                                    if let Some(sol) = app.solutions_state.solutions.get(app.solutions_state.selected_index) {
                                        let content = format!(
                                            "# {}\n\n**Severity:** {} | **Status:** {} | **Confidence:** {}\n\n**Source:** {}\n\n---\n\n{}",
                                            sol.issue_title, sol.issue_severity, sol.issue_status,
                                            sol.confidence, sol.source_url, sol.summary
                                        );
                                        let title = sol.issue_title.clone();
                                        open_mdr(&mut app, &title, &content);
                                    }
                                }
                            }
                            _ => { handled = false; }
                        }

                        if handled {
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }
                    }

                    // Confluence tab captures input for panel navigation and solve
                    if app.current_tab == Tab::Confluence {
                        let mut handled = true;
                        match key.code {
                            KeyCode::Left => {
                                match app.confluence_state.focus {
                                    ConfluenceFocus::Unmet => app.confluence_state.focus = ConfluenceFocus::Met,
                                    ConfluenceFocus::Solved => app.confluence_state.focus = ConfluenceFocus::Met,
                                    ConfluenceFocus::Met => {} // already leftmost
                                }
                            }
                            KeyCode::Right => {
                                match app.confluence_state.focus {
                                    ConfluenceFocus::Met => app.confluence_state.focus = ConfluenceFocus::Unmet,
                                    ConfluenceFocus::Solved => app.confluence_state.focus = ConfluenceFocus::Unmet,
                                    ConfluenceFocus::Unmet => {} // already rightmost
                                }
                            }
                            KeyCode::Tab => {
                                // Cycle: Met -> Unmet -> Solved -> Met
                                app.confluence_state.focus = match app.confluence_state.focus {
                                    ConfluenceFocus::Met => ConfluenceFocus::Unmet,
                                    ConfluenceFocus::Unmet => ConfluenceFocus::Solved,
                                    ConfluenceFocus::Solved => ConfluenceFocus::Met,
                                };
                            }
                            KeyCode::BackTab => {
                                app.confluence_state.focus = match app.confluence_state.focus {
                                    ConfluenceFocus::Met => ConfluenceFocus::Solved,
                                    ConfluenceFocus::Unmet => ConfluenceFocus::Met,
                                    ConfluenceFocus::Solved => ConfluenceFocus::Unmet,
                                };
                            }
                            KeyCode::Up => {
                                match app.confluence_state.focus {
                                    ConfluenceFocus::Met => app.confluence_state.scroll_met(-1),
                                    ConfluenceFocus::Unmet => app.confluence_state.scroll_unmet(-1),
                                    ConfluenceFocus::Solved => app.confluence_state.scroll_solved(-1),
                                }
                            }
                            KeyCode::Down => {
                                match app.confluence_state.focus {
                                    ConfluenceFocus::Met => app.confluence_state.scroll_met(1),
                                    ConfluenceFocus::Unmet => app.confluence_state.scroll_unmet(1),
                                    ConfluenceFocus::Solved => app.confluence_state.scroll_solved(1),
                                }
                            }
                            KeyCode::Enter => {
                                // Trigger solve on selected met/unmet confluence
                                if matches!(app.confluence_state.focus, ConfluenceFocus::Met | ConfluenceFocus::Unmet)
                                    && matches!(app.confluence_state.solve_status, SolveStatus::Idle)
                                {
                                    app.confluence_state.trigger_solve(app.tick_count);
                                }
                            }
                            _ => { handled = false; }
                        }

                        if handled {
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }
                    }

                    // Solve tab: search mode captures all input first
                    if app.current_tab == Tab::Solve && app.solve_state.search.active {
                        let mut handled = true;
                        match key.code {
                            KeyCode::Esc => {
                                app.solve_state.search.deactivate();
                            }
                            KeyCode::Enter => {
                                if let Some(result_idx) = app.solve_state.search.selected_result_index() {
                                    // Search covers all items: AI first, then Human
                                    let ai_len = app.solve_state.ai_items.len();
                                    if result_idx < ai_len {
                                        app.solve_state.focus = SolveFocus::AiList;
                                        app.solve_state.ai_selected = result_idx;
                                        app.solve_state.ai_list_state.select(Some(result_idx));
                                    } else {
                                        let human_idx = result_idx - ai_len;
                                        app.solve_state.focus = SolveFocus::HumanList;
                                        app.solve_state.human_selected = human_idx;
                                        app.solve_state.human_list_state.select(Some(human_idx));
                                    }
                                }
                                app.solve_state.search.deactivate();
                            }
                            KeyCode::Up => {
                                app.solve_state.search.select_prev();
                            }
                            KeyCode::Down => {
                                app.solve_state.search.select_next();
                            }
                            KeyCode::Backspace => {
                                app.solve_state.search.delete_char_before();
                                let items: Vec<(usize, String)> = app.solve_state.ai_items.iter().enumerate()
                                    .map(|(i, item)| (i, item.name.clone()))
                                    .chain(app.solve_state.human_items.iter().enumerate()
                                        .map(|(i, item)| (i + app.solve_state.ai_items.len(), item.name.clone())))
                                    .collect();
                                let item_refs: Vec<(usize, &str)> = items.iter().map(|(i, s)| (*i, s.as_str())).collect();
                                app.solve_state.search.update_results(&item_refs);
                            }
                            KeyCode::Char(c) => {
                                app.solve_state.search.insert_char(c);
                                let items: Vec<(usize, String)> = app.solve_state.ai_items.iter().enumerate()
                                    .map(|(i, item)| (i, item.name.clone()))
                                    .chain(app.solve_state.human_items.iter().enumerate()
                                        .map(|(i, item)| (i + app.solve_state.ai_items.len(), item.name.clone())))
                                    .collect();
                                let item_refs: Vec<(usize, &str)> = items.iter().map(|(i, s)| (*i, s.as_str())).collect();
                                app.solve_state.search.update_results(&item_refs);
                            }
                            _ => { handled = false; }
                        }
                        if handled {
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }
                    }

                    // Solve tab captures input for two-column navigation and actions
                    if app.current_tab == Tab::Solve {
                        let mut handled = true;
                        match key.code {
                            KeyCode::Tab => {
                                // Cycle focus: AiList -> AiActions -> HumanList -> Solved -> AiList
                                app.solve_state.focus = match app.solve_state.focus {
                                    SolveFocus::AiList => SolveFocus::AiActions,
                                    SolveFocus::AiActions => SolveFocus::HumanList,
                                    SolveFocus::HumanList => SolveFocus::Solved,
                                    SolveFocus::Solved => SolveFocus::AiList,
                                };
                            }
                            KeyCode::BackTab => {
                                app.solve_state.focus = match app.solve_state.focus {
                                    SolveFocus::AiList => SolveFocus::Solved,
                                    SolveFocus::AiActions => SolveFocus::AiList,
                                    SolveFocus::HumanList => SolveFocus::AiActions,
                                    SolveFocus::Solved => SolveFocus::HumanList,
                                };
                            }
                            KeyCode::Left => {
                                match app.solve_state.focus {
                                    SolveFocus::HumanList => app.solve_state.focus = SolveFocus::AiList,
                                    SolveFocus::Solved => app.solve_state.focus = SolveFocus::AiList,
                                    SolveFocus::AiActions => {
                                        if app.solve_state.active_button > 0 {
                                            app.solve_state.active_button -= 1;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            KeyCode::Right => {
                                match app.solve_state.focus {
                                    SolveFocus::AiList => app.solve_state.focus = SolveFocus::HumanList,
                                    SolveFocus::Solved => app.solve_state.focus = SolveFocus::HumanList,
                                    SolveFocus::AiActions => {
                                        let max_btn = if app.solve_state.auto_solve_mode != AutoSolveMode::Off
                                            || app.solve_state.progress == SolveProgress::Solving { 5 } else { 4 };
                                        if app.solve_state.active_button < max_btn {
                                            app.solve_state.active_button += 1;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            KeyCode::Up => {
                                match app.solve_state.focus {
                                    SolveFocus::AiList => app.solve_state.scroll_ai(-1),
                                    SolveFocus::HumanList => app.solve_state.scroll_human(-1),
                                    SolveFocus::Solved => app.solve_state.scroll_solved(-1),
                                    SolveFocus::AiActions => {
                                        app.solve_state.focus = SolveFocus::AiList;
                                    }
                                }
                            }
                            KeyCode::Down => {
                                match app.solve_state.focus {
                                    SolveFocus::AiList => app.solve_state.scroll_ai(1),
                                    SolveFocus::HumanList => app.solve_state.scroll_human(1),
                                    SolveFocus::Solved => app.solve_state.scroll_solved(1),
                                    SolveFocus::AiActions => {} // already at bottom
                                }
                            }
                            KeyCode::Char(' ') => {
                                match app.solve_state.focus {
                                    SolveFocus::AiList => {
                                        app.solve_state.toggle_ai_check();
                                    }
                                    SolveFocus::HumanList => {
                                        // Set strikethrough tick before toggling
                                        if let Some(item) = app.solve_state.human_items.get(app.solve_state.human_selected) {
                                            if !item.strikethrough {
                                                let tick = app.tick_count;
                                                if let Some(item) = app.solve_state.human_items.get_mut(app.solve_state.human_selected) {
                                                    item.strikethrough = true;
                                                    item.strikethrough_tick = Some(tick);
                                                }
                                            } else {
                                                app.solve_state.toggle_human_check();
                                            }
                                        }
                                    }
                                    _ => { handled = false; }
                                }
                            }
                            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                                // Shift+Enter: open overlay for selected item
                                let item_info = match app.solve_state.focus {
                                    SolveFocus::AiList => {
                                        app.solve_state.ai_items.get(app.solve_state.ai_selected)
                                            .map(|i| (i.name.clone(), format!("# {}\n\n{}", i.name, i.summary)))
                                    }
                                    SolveFocus::HumanList => {
                                        app.solve_state.human_items.get(app.solve_state.human_selected)
                                            .map(|i| (i.name.clone(), format!("# {}\n\n{}", i.name, i.summary)))
                                    }
                                    _ => None,
                                };
                                if let Some((title, md)) = item_info {
                                    open_mdr(&mut app, &title, &md);
                                }
                            }
                            KeyCode::Enter => {
                                match app.solve_state.focus {
                                    SolveFocus::AiActions => {
                                        match app.solve_state.active_button {
                                            0 => {
                                                // Solve
                                                if app.solve_state.progress == SolveProgress::Idle {
                                                    let tick = app.tick_count;
                                                    let mesh_db = app.mesh_db_path();
                                                    let research_db = app.research_db_path.clone();
                                                    let env_map = io_layer::env_store::load(&app.env_path);
                                                    for item in app.solve_state.ai_items.iter_mut().filter(|i| i.checked && !i.dispatched) {
                                                        item.dispatched = true;
                                                        item.dispatch_tick = Some(tick);
                                                        let item_id = item.id;
                                                        let name = item.name.clone();
                                                        let summary = item.summary.clone();
                                                        let cluster_id = item.cluster_id;
                                                        let is_surface = item.surface;
                                                        let mdb = mesh_db.clone();
                                                        let rdb = research_db.clone();
                                                        let ev = env_map.clone();
                                                        std::thread::spawn(move || {
                                                            if let Some(mdb) = mdb {
                                                                let ctx = io_layer::db::fetch_cluster_context(&mdb, &rdb, cluster_id, &name, &summary);
                                                                let ea_peer = "12D3KooWJtKPNjyKXjLTSmccExB9saN6N8mHuuZEMPa7M7LrDsSk";
                                                                let msg = build_ea_message(item_id, &ctx, is_surface);
                                                                let ok = std::process::Command::new("aqua")
                                                                    .args(["--dir", "/home/typhoon/.aqua/main", "send", ea_peer, "--message", &msg, "--topic", "ea.task"])
                                                                    .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                                                                    .status().map(|s| s.success()).unwrap_or(false);
                                                                if !ok {
                                                                    let _ = std::process::Command::new("aqua")
                                                                        .args(["send", ea_peer, "--message", &msg, "--topic", "ea.task"])
                                                                        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                                                                        .status();
                                                                }
                                                                if !is_surface { cleanup_airtable_for_cluster(&name, &ev); }
                                                            }
                                                        });
                                                    }
                                                    app.solve_state.start_solve(app.tick_count);
                                                }
                                            }
                                            1 => {
                                                // Transfer to Human
                                                app.solve_state.transfer_to_human();
                                            }
                                            2 => {
                                                // Dissolve
                                                let tick = app.tick_count;
                                                app.solve_state.dissolve_checked(tick);
                                            }
                                            3 => {
                                                // Auto-Solve All toggle
                                                app.solve_state.auto_solve_mode = match app.solve_state.auto_solve_mode {
                                                    AutoSolveMode::All => AutoSolveMode::Off,
                                                    _ => AutoSolveMode::All,
                                                };
                                            }
                                            4 => {
                                                // Auto-Solve Selected toggle
                                                app.solve_state.auto_solve_mode = match app.solve_state.auto_solve_mode {
                                                    AutoSolveMode::Selected => AutoSolveMode::Off,
                                                    _ => AutoSolveMode::Selected,
                                                };
                                            }
                                            5 => { app.solve_state.stop_auto_solve(); }
                                            _ => {}
                                        }
                                    }
                                    SolveFocus::AiList => {
                                        if let Some(idx) = app.solve_state.ai_list_state.selected() {
                                            if let Some(item) = app.solve_state.ai_items.get(idx) {
                                                let mut content = format!("**Classification:** AI Solvable — automated action signals detected\n\n# {}\n\n## Strategy\n{}", item.name, item.summary);
                                                if !item.actions.is_empty() {
                                                    content.push_str("\n\n## Auto Actions\n");
                                                    for (i, a) in item.actions.iter().enumerate() {
                                                        content.push_str(&format!("{}. {}\n", i + 1, a));
                                                    }
                                                }
                                                let title = item.name.clone();
                                                open_mdr(&mut app, &title, &content);
                                            }
                                        }
                                    }
                                    SolveFocus::HumanList => {
                                        if let Some(idx) = app.solve_state.human_list_state.selected() {
                                            if let Some(item) = app.solve_state.human_items.get(idx) {
                                                let content = format!("**Classification:** Human Solvable — human involvement signals or no AI signals\n\n# {}\n\n## Strategy\n{}", item.name, item.summary);
                                                let title = item.name.clone();
                                                open_mdr(&mut app, &title, &content);
                                            }
                                        }
                                    }
                                    SolveFocus::Solved => {
                                        if let Some(item) = app.solve_state.solved_items.get(app.solve_state.solved_selected) {
                                            let content = format!("**Status:** Solved ({})\n\n# {}\n\n## Summary\n{}", item.solved_at, item.name, item.summary);
                                            let title = item.name.clone();
                                            open_mdr(&mut app, &title, &content);
                                        }
                                    }
                                    _ => { handled = false; }
                                }
                            }
                            KeyCode::Char('s') => {
                                // Quick solve shortcut
                                if app.solve_state.progress == SolveProgress::Idle {
                                    let tick = app.tick_count;
                                    let mesh_db = app.mesh_db_path();
                                    let research_db = app.research_db_path.clone();
                                    let env_map = io_layer::env_store::load(&app.env_path);
                                    for item in app.solve_state.ai_items.iter_mut().filter(|i| i.checked && !i.dispatched) {
                                        item.dispatched = true;
                                        item.dispatch_tick = Some(tick);
                                        let item_id = item.id;
                                        let name = item.name.clone();
                                        let summary = item.summary.clone();
                                        let cluster_id = item.cluster_id;
                                        let is_surface = item.surface;
                                        let mdb = mesh_db.clone();
                                        let rdb = research_db.clone();
                                        let ev = env_map.clone();
                                        std::thread::spawn(move || {
                                            if let Some(mdb) = mdb {
                                                let ctx = io_layer::db::fetch_cluster_context(&mdb, &rdb, cluster_id, &name, &summary);
                                                let ea_peer = "12D3KooWJtKPNjyKXjLTSmccExB9saN6N8mHuuZEMPa7M7LrDsSk";
                                                let msg = build_ea_message(item_id, &ctx, is_surface);
                                                let ok = std::process::Command::new("aqua")
                                                    .args(["--dir", "/home/typhoon/.aqua/main", "send", ea_peer, "--message", &msg, "--topic", "ea.task"])
                                                    .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                                                    .status().map(|s| s.success()).unwrap_or(false);
                                                if !ok {
                                                    let _ = std::process::Command::new("aqua")
                                                        .args(["send", ea_peer, "--message", &msg, "--topic", "ea.task"])
                                                        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                                                        .status();
                                                }
                                                if !is_surface { cleanup_airtable_for_cluster(&name, &ev); }
                                            }
                                        });
                                    }
                                    app.solve_state.start_solve(app.tick_count);
                                }
                            }
                            KeyCode::Char('t') => {
                                // Quick transfer shortcut
                                app.solve_state.transfer_to_human();
                            }
                            KeyCode::Char('d') => {
                                // Quick dissolve shortcut
                                let tick = app.tick_count;
                                app.solve_state.dissolve_checked(tick);
                            }
                            KeyCode::Char('X') | KeyCode::Char('x') => {
                                // X = emergency stop auto-solve
                                app.solve_state.stop_auto_solve();
                            }
                            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                // Ctrl+A: toggle Auto-Solve All mode
                                app.solve_state.auto_solve_mode = match app.solve_state.auto_solve_mode {
                                    AutoSolveMode::All => AutoSolveMode::Off,
                                    _ => AutoSolveMode::All,
                                };
                            }
                            KeyCode::Char('a') => {
                                // Select all AI items
                                let all_checked = app.solve_state.ai_items.iter().all(|i| i.checked);
                                for item in &mut app.solve_state.ai_items {
                                    item.checked = !all_checked;
                                }
                            }
                            KeyCode::Char('/') => {
                                app.solve_state.search.activate();
                            }
                            KeyCode::Char('m') | KeyCode::Char('M') => {
                                match app.solve_state.focus {
                                    SolveFocus::AiList => app.solve_state.move_selected_to_human(),
                                    SolveFocus::HumanList => app.solve_state.move_selected_to_ai(),
                                    _ => { handled = false; }
                                }
                            }
                            _ => { handled = false; }
                        }

                        if handled {
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }
                    }

                    // Settings tab captures input for sliders and buttons
                    if app.current_tab == Tab::Settings {
                        let mut handled = true;
                        match key.code {
                            KeyCode::Tab => {
                                app.settings.advance_focus();
                            }
                            KeyCode::BackTab => {
                                app.settings.retreat_focus();
                            }
                            KeyCode::Enter => {
                                if let Some(action) = app.settings.focused_action() {
                                    match action {
                                        SettingsAction::Refresh => {
                                            app.settings.refresh_sysinfo();
                                            app.settings.status_message = Some(("System info refreshed".to_string(), true));
                                            app.settings.status_tick = app.tick_count;
                                        }
                                        SettingsAction::SaveSysinfo => {
                                            match app.settings.save_sysinfo() {
                                                Ok(_) => {
                                                    app.settings.status_message = Some(("Sysinfo saved to file".to_string(), true));
                                                }
                                                Err(e) => {
                                                    app.settings.status_message = Some((format!("Save error: {}", e), false));
                                                }
                                            }
                                            app.settings.status_tick = app.tick_count;
                                        }
                                        SettingsAction::SaveAll => {
                                            match app.settings.save_params() {
                                                Ok(_) => {
                                                    app.settings.status_message = Some(("Pipeline params saved".to_string(), true));
                                                }
                                                Err(e) => {
                                                    app.settings.status_message = Some((format!("Save error: {}", e), false));
                                                }
                                            }
                                            app.settings.status_tick = app.tick_count;
                                        }
                                        SettingsAction::ResetDefaults => {
                                            app.settings.reset_defaults();
                                            app.settings.status_message = Some(("Reset to defaults".to_string(), true));
                                            app.settings.status_tick = app.tick_count;
                                        }
                                        SettingsAction::ToggleEdit => {
                                            if let Some(slider) = app.settings.focused_slider_mut() {
                                                if slider.editing_number {
                                                    slider.commit_edit();
                                                } else {
                                                    slider.start_editing();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            KeyCode::Esc => {
                                if let Some(slider) = app.settings.focused_slider_mut() {
                                    if slider.editing_number {
                                        slider.cancel_edit();
                                    }
                                }
                            }
                            KeyCode::Left => {
                                if app.settings.has_focused_slider() && !app.settings.has_focused_input() {
                                    if let Some(slider) = app.settings.focused_slider_mut() {
                                        slider.decrement();
                                    }
                                } else {
                                    handled = false;
                                }
                            }
                            KeyCode::Right => {
                                if app.settings.has_focused_slider() && !app.settings.has_focused_input() {
                                    if let Some(slider) = app.settings.focused_slider_mut() {
                                        slider.increment();
                                    }
                                } else {
                                    handled = false;
                                }
                            }
                            KeyCode::Char(c) if app.settings.has_focused_input() => {
                                if let Some(slider) = app.settings.focused_slider_mut() {
                                    slider.type_char(c);
                                }
                            }
                            KeyCode::Backspace if app.settings.has_focused_input() => {
                                if let Some(slider) = app.settings.focused_slider_mut() {
                                    slider.backspace();
                                }
                            }
                            _ => { handled = false; }
                        }

                        if handled {
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }

                        // Guard: don't let 'q' quit when editing a number
                        if app.settings.has_focused_input() {
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            continue;
                        }
                    }

                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                        KeyCode::Char('r') => {
                            app.refresh();
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        }
                        KeyCode::Char(c @ '1'..='8') => {
                            if let Some(tab) = Tab::from_index((c as usize) - ('1' as usize)) {
                                app.set_tab(tab);
                                terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                            }
                        }
                        KeyCode::Tab => {
                            app.next_tab();
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        }
                        KeyCode::BackTab => {
                            app.prev_tab();
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        }
                        KeyCode::Up => {
                            match app.current_tab {
                                Tab::Learnings => app.scroll_recent_learnings(-1),
                                Tab::Research => app.scroll_research_issues(-1),
                                _ => {} // stub tabs have no scrollable content yet
                            }
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        }
                        KeyCode::Down => {
                            match app.current_tab {
                                Tab::Learnings => app.scroll_recent_learnings(1),
                                Tab::Research => app.scroll_research_issues(1),
                                _ => {} // stub tabs have no scrollable content yet
                            }
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse_event) => {
                    // Overlay mouse scroll
                    if app.overlay.is_some() {
                        match mouse_event.kind {
                            MouseEventKind::ScrollUp => {
                                if let Some(ref mut ov) = app.overlay { ov.scroll_up(3); }
                            }
                            MouseEventKind::ScrollDown => {
                                if let Some(ref mut ov) = app.overlay { ov.scroll_down(3); }
                            }
                            _ => {}
                        }
                        terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        continue;
                    }

                    let mouse = mouse_event;
                    // Welcome screen mouse handling
                    if app.screen == Screen::Welcome {
                        if let MouseEventKind::Down(crossterm::event::MouseButton::Left) = mouse.kind {
                            let size = terminal.size()?;
                            let term_area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
                            if screens::welcome::hit_test_button(mouse.column, mouse.row, term_area) {
                                app.screen = Screen::Main;
                                app.set_tab(Tab::Portal);
                            }
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        }
                        continue;
                    }

                    match mouse.kind {
                        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                            let delta = if mouse.kind == MouseEventKind::ScrollUp { -3 } else { 3 };
                            let col = mouse.column;
                            let row = mouse.row;
                            match app.current_tab {
                                Tab::Learnings => {
                                    if is_in_rect(col, row, panel_areas.dropbox_runs) {
                                        app.scroll_dropbox_runs(delta);
                                    } else if is_in_rect(col, row, panel_areas.email_runs) {
                                        app.scroll_email_runs(delta);
                                    } else if is_in_rect(col, row, panel_areas.recent_learnings) {
                                        app.scroll_recent_learnings(delta);
                                    }
                                }
                                Tab::Research => {
                                    if is_in_rect(col, row, panel_areas.research_issues) {
                                        app.scroll_research_issues(delta);
                                    } else if is_in_rect(col, row, panel_areas.research_solutions) {
                                        app.scroll_research_solutions(delta);
                                    }
                                }
                                Tab::Issues => {
                                    // Scroll issue list regardless of mouse position
                                    app.issues_state.scroll_list(delta);
                                }
                                Tab::Solutions => {
                                    app.solutions_state.scroll_list(delta);
                                }
                                Tab::Confluence => {
                                    match app.confluence_state.focus {
                                        ConfluenceFocus::Met => app.confluence_state.scroll_met(delta),
                                        ConfluenceFocus::Unmet => app.confluence_state.scroll_unmet(delta),
                                        ConfluenceFocus::Solved => app.confluence_state.scroll_solved(delta),
                                    }
                                }
                                Tab::Solve => {
                                    let sr = app.solve_state.solved_rect;
                                    if col >= sr.x && col < sr.x + sr.width && row >= sr.y && row < sr.y + sr.height {
                                        // Mouse is over Solved panel — scroll it regardless of focus
                                        let len = app.solve_state.solved_items.len();
                                        if len > 0 {
                                            let cur = app.solve_state.solved_list_state.selected().unwrap_or(0);
                                            if delta < 0 {
                                                app.solve_state.solved_list_state.select(Some(cur.saturating_sub(1)));
                                            } else {
                                                app.solve_state.solved_list_state.select(Some((cur + 1).min(len - 1)));
                                            }
                                        }
                                    } else {
                                        match app.solve_state.focus {
                                            SolveFocus::AiList | SolveFocus::AiActions => app.solve_state.scroll_ai(delta),
                                            SolveFocus::HumanList => app.solve_state.scroll_human(delta),
                                            SolveFocus::Solved => app.solve_state.scroll_solved(delta),
                                        }
                                    }
                                }
                                _ => {} // stub tabs have no scrollable content yet
                            }
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        }
                        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                            if let Some(tab) = tab_bar_state.hit_test(mouse.column, mouse.row) {
                                app.set_tab(tab);
                            }
                            // Settings tab click handling
                            if app.current_tab == Tab::Settings {
                                let clicked = app.settings.handle_click(mouse.column, mouse.row);
                                if clicked {
                                    // If a button was clicked, execute its action
                                    if let Some(action) = app.settings.focused_action() {
                                        match action {
                                            SettingsAction::Refresh => {
                                                app.settings.refresh_sysinfo();
                                                app.settings.status_message = Some(("System info refreshed".to_string(), true));
                                                app.settings.status_tick = app.tick_count;
                                            }
                                            SettingsAction::SaveSysinfo => {
                                                match app.settings.save_sysinfo() {
                                                    Ok(_) => {
                                                        app.settings.status_message = Some(("Sysinfo saved".to_string(), true));
                                                    }
                                                    Err(e) => {
                                                        app.settings.status_message = Some((format!("Error: {}", e), false));
                                                    }
                                                }
                                                app.settings.status_tick = app.tick_count;
                                            }
                                            SettingsAction::SaveAll => {
                                                match app.settings.save_params() {
                                                    Ok(_) => {
                                                        app.settings.status_message = Some(("Pipeline params saved".to_string(), true));
                                                    }
                                                    Err(e) => {
                                                        app.settings.status_message = Some((format!("Error: {}", e), false));
                                                    }
                                                }
                                                app.settings.status_tick = app.tick_count;
                                            }
                                            SettingsAction::ResetDefaults => {
                                                app.settings.reset_defaults();
                                                app.settings.status_message = Some(("Reset to defaults".to_string(), true));
                                                app.settings.status_tick = app.tick_count;
                                            }
                                            SettingsAction::ToggleEdit => {
                                                // Click on slider bar sets value, don't toggle edit
                                            }
                                        }
                                    }
                                }
                            }
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        } else {
            app.tick();
            tick_since_refresh += 1;
            if tick_since_refresh >= REFRESH_INTERVAL {
                app.refresh();
                tick_since_refresh = 0;
            }

            // Dispatch any newly-queued solve items to EA agent (auto-solve path)
            {
                let tick = app.tick_count;
                let mesh_db = app.mesh_db_path();
                let research_db = app.research_db_path.clone();
                if let Some(mdb) = mesh_db {
                    // Rate-limit: 1 dispatch per tick to prevent thread storm on auto-solve
                    let mut dispatched_this_tick = false;
                    let mut timeout_ids: Vec<u64> = Vec::new();

                    for item in app.solve_state.ai_items.iter_mut() {
                        // Check timeout: dispatched items with no response after DISPATCH_TIMEOUT_TICKS → move to Human
                        if item.dispatched {
                            if let Some(dt) = item.dispatch_tick {
                                if tick.saturating_sub(dt) > DISPATCH_TIMEOUT_TICKS {
                                    timeout_ids.push(item.id);
                                }
                            }
                        }
                        if dispatched_this_tick { continue; }
                        if (item.solving || item.queued) && !item.dispatched {
                            item.dispatched = true;
                            item.dispatch_tick = Some(tick);
                            dispatched_this_tick = true;
                            let cluster_id = item.cluster_id;
                            let item_id = item.id;
                            let name = item.name.clone();
                            let summary = item.summary.clone();
                            let is_surface = item.surface;
                            let mdb2 = mdb.clone();
                            let rdb = research_db.clone();
                            let env_map = io_layer::env_store::load(&app.env_path);
                            std::thread::spawn(move || {
                                let ctx = io_layer::db::fetch_cluster_context(&mdb2, &rdb, cluster_id, &name, &summary);
                                let ea_peer = "12D3KooWJtKPNjyKXjLTSmccExB9saN6N8mHuuZEMPa7M7LrDsSk";
                                let msg = build_ea_message(item_id, &ctx, is_surface);
                                let ok = std::process::Command::new("aqua")
                                    .args(["--dir", "/home/typhoon/.aqua/main", "send", ea_peer, "--message", &msg, "--topic", "ea.task"])
                                    .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                                    .status().map(|s| s.success()).unwrap_or(false);
                                if !ok {
                                    let _ = std::process::Command::new("aqua")
                                        .args(["send", ea_peer, "--message", &msg, "--topic", "ea.task"])
                                        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
                                        .status();
                                }
                                if !is_surface {
                                    cleanup_airtable_for_cluster(&name, &env_map);
                                }
                            });
                        }
                    }

                    // Move timed-out items to Human
                    for id in timeout_ids {
                        if let Some(pos) = app.solve_state.ai_items.iter().position(|i| i.id == id) {
                            let mut item = app.solve_state.ai_items.remove(pos);
                            item.dispatched = false; item.solving = false; item.queued = false;
                            app.solve_state.human_items.push(item);
                        }
                    }
                }
            }

            // Poll EA response file for completed/failed/reclassified items
            poll_ea_responses(&mut app);

            // Drain cleanup results
            while let Ok((nl, ni, ns)) = cleanup_rx.try_recv() {
                app.cleanup_total.0 += nl;
                app.cleanup_total.1 += ni;
                app.cleanup_total.2 += ns;
                if nl + ni + ns > 0 { app.refresh(); }
            }

            // Check for completed OAuth localhost redirect flow
            if let Ok(mut guard) = oauth_result.try_lock() {
                if let Some(result) = guard.take() {
                    match result {
                        Ok(tokens) => {
                            // Save tokens to .env
                            let _ = io_layer::env_store::save(&app.env_path, &tokens);
                            app.portal.oauth_status = OAuthStatus::Success(tokens);
                            app.portal.status_message = Some(("OAuth tokens saved!".to_string(), true));
                            app.portal.status_tick = app.tick_count;
                        }
                        Err(e) => {
                            app.portal.oauth_status = OAuthStatus::Error(e);
                        }
                    }
                }
            }

            // Poll for device code flow completion (Microsoft)
            if let Some(ref poll_state) = device_poll {
                let elapsed = poll_state.last_poll.elapsed().as_secs();
                if elapsed >= poll_state.flow.interval {
                    match poll_state.flow.poll_for_token() {
                        Ok(Some(tokens)) => {
                            let _ = io_layer::env_store::save(&app.env_path, &tokens);
                            app.portal.oauth_status = OAuthStatus::Success(tokens);
                            app.portal.status_message = Some(("Microsoft OAuth complete!".to_string(), true));
                            app.portal.status_tick = app.tick_count;
                            device_poll = None;
                        }
                        Ok(None) => {
                            // Still waiting, update last_poll time
                        }
                        Err(e) => {
                            app.portal.oauth_status = OAuthStatus::Error(e);
                            device_poll = None;
                        }
                    }
                    // Update the last_poll timestamp
                    if let Some(ref mut ps) = device_poll {
                        ps.last_poll = std::time::Instant::now();
                    }
                }
            }

            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
        }

        if app.should_quit { break; }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}

fn is_in_rect(col: u16, row: u16, rect: ratatui::layout::Rect) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

/// Path where EA agent appends its resolve decisions as JSON lines.
const EA_RESPONSES_PATH: &str =
    "/home/typhoon/.openclaw/workspace-researcher-agent/db/ea-responses.jsonl";

/// Maximum ticks before a dispatched item is auto-moved to Human Solvable (~60s at 200ms/tick).
const DISPATCH_TIMEOUT_TICKS: u64 = 300;

/// Parse a single line from ea-responses.jsonl.
/// Expected format: {"item_id": N, "decision": "SOLVED|FAILED|AI_SOLVABLE|HUMAN_REQUIRED", "summary": "..."}
fn parse_ea_response(line: &str) -> Option<(u64, String, String)> {
    // item_id
    let id_start = line.find("\"item_id\":")?;
    let after_id = line[id_start + 10..].trim_start();
    let id_end = after_id.find(|c: char| !c.is_ascii_digit())?;
    let item_id: u64 = after_id[..id_end].parse().ok()?;

    // decision
    let dec_start = line.find("\"decision\":")?;
    let after_dec = line[dec_start + 11..].trim_start().trim_start_matches('"');
    let dec_end = after_dec.find('"')?;
    let decision = after_dec[..dec_end].to_string();

    // summary (optional)
    let summary = if let Some(sum_start) = line.find("\"summary\":") {
        let after_sum = line[sum_start + 10..].trim_start().trim_start_matches('"');
        if let Some(sum_end) = after_sum.find('"') {
            after_sum[..sum_end].to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    Some((item_id, decision, summary))
}

/// Poll ea-responses.jsonl for EA agent decisions and update solve state accordingly.
fn poll_ea_responses(app: &mut App) {
    let path = std::path::Path::new(EA_RESPONSES_PATH);
    if !path.exists() { return; }
    let content = match std::fs::read_to_string(path) { Ok(c) => c, Err(_) => return };
    if content.trim().is_empty() { return; }

    let mut processed_lines: Vec<String> = Vec::new();
    let mesh_db = app.mesh_db_path();
    let research_db = app.research_db_path.clone();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        if let Some((item_id, decision, summary)) = parse_ea_response(trimmed) {
            processed_lines.push(trimmed.to_string());
            match decision.as_str() {
                "AI_SOLVABLE" => {
                    // Surface item researched → re-list as Deep (surface=false) with new summary
                    if let Some(pos) = app.solve_state.ai_items.iter().position(|i| i.id == item_id && i.surface) {
                        app.solve_state.ai_items[pos].surface = false;
                        app.solve_state.ai_items[pos].dispatched = false;
                        app.solve_state.ai_items[pos].solving = false;
                        app.solve_state.ai_items[pos].queued = false;
                        app.solve_state.ai_items[pos].dispatch_tick = None;
                        if !summary.is_empty() {
                            app.solve_state.ai_items[pos].summary = summary;
                        }
                    }
                }
                "HUMAN_REQUIRED" | "FAILED" => {
                    // Move item to human list
                    if let Some(pos) = app.solve_state.ai_items.iter().position(|i| i.id == item_id) {
                        let mut item = app.solve_state.ai_items.remove(pos);
                        item.dispatched = false; item.solving = false; item.queued = false;
                        if !summary.is_empty() { item.summary = summary; }
                        app.solve_state.human_items.push(item);
                        app.solve_state.fix_ai_selection();
                        app.solve_state.fix_human_selection();
                    }
                }
                "SOLVED" => {
                    if let Some(pos) = app.solve_state.ai_items.iter().position(|i| i.id == item_id) {
                        let item = app.solve_state.ai_items.remove(pos);
                        let cluster_id = item.cluster_id;
                        let member_ids = item.member_ids_json.clone();
                        let item_summary = if !summary.is_empty() { summary } else { item.summary.clone() };
                        app.solve_state.solved_items.push(app::SolvedItem {
                            name: item.name.clone(),
                            method: "AI".to_string(),
                            solved_at: chrono::Local::now().format("%H:%M").to_string(),
                            summary: item_summary,
                        });
                        app.solve_state.fix_ai_selection();
                        app.solve_state.fix_solved_selection();
                        // Fire DB deletion in background
                        if let Some(mdb) = mesh_db.clone() {
                            let rdb = research_db.clone();
                            std::thread::spawn(move || {
                                let _ = io_layer::db::delete_solved_cluster(&mdb, &rdb, cluster_id, &member_ids);
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Rewrite file without processed lines
    if !processed_lines.is_empty() {
        let remaining: String = content.lines()
            .filter(|l| !processed_lines.contains(&l.trim().to_string()))
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(path, if remaining.is_empty() { String::new() } else { remaining + "\n" });
    }

    // If no dispatched items remain, transition to Idle
    let any_dispatched = app.solve_state.ai_items.iter().any(|i| i.dispatched);
    if !any_dispatched && app.solve_state.progress == SolveProgress::Solving {
        app.solve_state.progress = SolveProgress::Done;
    }
}

/// Build the EA task message for a solve item. Surface items get a deep research request;
/// Deep items (surface=false) get a direct solve instruction.
fn build_ea_message(item_id: u64, ctx: &io_layer::db::ClusterContext, is_surface: bool) -> String {
    let mut msg = if is_surface {
        format!(
            "DEEP RESEARCH TASK\n\nCluster: {}\nItem ID: {}\n\nCurrent surface strategy:\n{}\n\n\
             The action list is surface-only (notifications/logging). Research deeper: \
             pull full context from this cluster's issues and solutions, determine if AI \
             can concretely solve this (file edits, code changes, emails, config updates, etc.).\n\n\
             When done, append EXACTLY ONE line to {}:\n\
             If AI can solve it: {{\"item_id\": {}, \"decision\": \"AI_SOLVABLE\", \"summary\": \"<concrete steps>\"}}\n\
             If human required: {{\"item_id\": {}, \"decision\": \"HUMAN_REQUIRED\", \"summary\": \"<reason>\"}}\n",
            ctx.cluster_name, item_id, ctx.strategy_summary,
            EA_RESPONSES_PATH, item_id, item_id
        )
    } else {
        format!(
            "SOLVE TASK\n\nCluster: {}\nItem ID: {}\n\nStrategy:\n{}\n",
            ctx.cluster_name, item_id, ctx.strategy_summary
        )
    };

    if !ctx.issues.is_empty() {
        msg.push_str("\nIssues:\n");
        for (t, s) in &ctx.issues { msg.push_str(&format!("  - {} [{}]\n", t, s)); }
    }
    if !ctx.solutions.is_empty() {
        msg.push_str("\nSolutions to execute:\n");
        for (i, s) in &ctx.solutions { msg.push_str(&format!("  Issue: {}\n  Solution: {}\n\n", i, s)); }
    }

    if !is_surface {
        msg.push_str(&format!(
            "\nInstructions: Execute each solution concretely — edit files, draft emails into Drafts \
             folders, write documents, fix configs/code in-place. Do NOT log to Airtable. \
             Actually perform the work.\n\n\
             When complete, append to {}:\n\
             {{\"item_id\": {}, \"decision\": \"SOLVED\", \"summary\": \"<what was done>\"}}\n\
             If unable to complete: {{\"item_id\": {}, \"decision\": \"FAILED\", \"summary\": \"<reason>\"}}",
            EA_RESPONSES_PATH, item_id, item_id
        ));
    }

    msg
}

fn dispatch_solve_to_ea(app: &App, cluster_id: u64, item_name: &str, item_summary: &str) -> bool {
    let mesh_db = match app.mesh_db_path() {
        Some(p) => p,
        None => return false,
    };
    let research_db = &app.research_db_path;

    let ctx = io_layer::db::fetch_cluster_context(&mesh_db, research_db, cluster_id, item_name, item_summary);

    let mut msg = format!(
        "SOLVE TASK\n\nCluster: {}\n\nStrategy:\n{}\n",
        ctx.cluster_name, ctx.strategy_summary
    );

    if !ctx.issues.is_empty() {
        msg.push_str("\nIssues:\n");
        for (title, severity) in &ctx.issues {
            msg.push_str(&format!("  - {} [{}]\n", title, severity));
        }
    }

    if !ctx.solutions.is_empty() {
        msg.push_str("\nSolutions to execute:\n");
        for (issue, sol) in &ctx.solutions {
            msg.push_str(&format!("  Issue: {}\n  Solution: {}\n\n", issue, sol));
        }
    }

    msg.push_str("\nInstructions: Execute each solution concretely — edit files, draft emails into Drafts folders, write documents to appropriate directories, fix configs/code in-place. Do NOT log to Airtable. Actually perform the work and report what was done.");

    let ea_peer = "12D3KooWJtKPNjyKXjLTSmccExB9saN6N8mHuuZEMPa7M7LrDsSk";

    let result = std::process::Command::new("aqua")
        .args(["--dir", "/home/typhoon/.aqua/main", "send", ea_peer, "--message", &msg, "--topic", "ea.task"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !result {
        std::process::Command::new("aqua")
            .args(["send", ea_peer, "--message", &msg, "--topic", "ea.task"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        result
    }
}

fn cleanup_airtable_for_cluster(cluster_name: &str, env_map: &std::collections::HashMap<String, String>) {
    let api_key = env_map.get("AIRTABLE_API_KEY").cloned().unwrap_or_default();
    let base_id = env_map.get("AIRTABLE_BASE_ID").cloned().unwrap_or_default();
    let table_id = env_map.get("AIRTABLE_RESEARCH_TABLE_ID").cloned().unwrap_or_default();
    if api_key.is_empty() || base_id.is_empty() || table_id.is_empty() {
        return;
    }

    let safe_name = cluster_name.replace('"', "\\\"").replace(' ', "%20");
    let filter = format!("filterByFormula=FIND(%22{}%22%2C%7BName%7D)", safe_name);
    let list_url = format!("https://api.airtable.com/v0/{}/{}?{}", base_id, table_id, filter);

    let auth = format!("Authorization: Bearer {}", api_key);
    let output = std::process::Command::new("curl")
        .args(["-s", "-H", &auth, &list_url])
        .output();

    if let Ok(out) = output {
        let body = String::from_utf8_lossy(&out.stdout);
        let mut pos = 0;
        let mut record_ids = Vec::new();
        while let Some(i) = body[pos..].find("\"id\":\"rec") {
            let start = pos + i + 6;
            if let Some(end) = body[start..].find('"') {
                record_ids.push(body[start..start + end].to_string());
                pos = start + end;
            } else {
                break;
            }
        }
        for rid in record_ids {
            let del_url = format!("https://api.airtable.com/v0/{}/{}/{}", base_id, table_id, rid);
            let _ = std::process::Command::new("curl")
                .args(["-s", "-X", "DELETE", "-H", &auth, &del_url])
                .output();
        }
    }
}

fn open_mdr(app: &mut App, title: &str, content: &str) {
    app.overlay = Some(crate::app::OverlayState::new(title.to_string(), content.to_string()));
}

fn save_portal_section(app: &App, section: &str) {
    use io_layer::env_store;
    let mut values = HashMap::new();
    match section {
        "ai" => {
            values.insert("OPENROUTER_API_KEY".to_string(), app.portal.api_key.value.clone());
            values.insert("MODEL_OVERRIDE".to_string(), app.portal.model_dropdown.selected_value().to_string());
        }
        "dropbox" => {
            values.insert("DROPBOX_TOKEN".to_string(), app.portal.dropbox_token.value.clone());
            values.insert("DROPBOX_REFRESH_TOKEN".to_string(), app.portal.dropbox_token.value.clone());
        }
        "imap" => {
            values.insert("IMAP_HOST".to_string(), app.portal.imap_host.value.clone());
            values.insert("IMAP_PORT".to_string(), app.portal.imap_port.value.clone());
            // Write both key variants so learn --tui and Solvable both pick them up
            values.insert("IMAP_USER".to_string(), app.portal.imap_user.value.clone());
            values.insert("IMAP_USERNAME".to_string(), app.portal.imap_user.value.clone());
            values.insert("IMAP_PASS".to_string(), app.portal.imap_pass.value.clone());
            values.insert("IMAP_PASSWORD".to_string(), app.portal.imap_pass.value.clone());
        }
        "airtable" => {
            values.insert("AIRTABLE_API_KEY".to_string(), app.portal.airtable_key.value.clone());
            values.insert("AIRTABLE_BASE_ID".to_string(), app.portal.airtable_base.value.clone());
        }
        "protonmail" => {
            values.insert("PROTONMAIL_USERNAME".to_string(), app.portal.protonmail_user.value.clone());
            values.insert("PROTONMAIL_BRIDGE_PW".to_string(), app.portal.protonmail_pass.value.clone());
        }
        "gmail" => {
            values.insert("GMAIL_USER".to_string(), app.portal.gmail_user.value.clone());
            values.insert("GMAIL_PASS".to_string(), app.portal.gmail_pass.value.clone());
        }
        "m365_heartlab" => {
            values.insert("M365_HEARTLAB_USER".to_string(), app.portal.m365_hl_user.value.clone());
            values.insert("M365_HEARTLAB_APPLICATION_CLIENT_ID".to_string(), app.portal.m365_hl_client_id.value.clone());
            values.insert("M365_HEARTLAB_CLIENT_VALUE".to_string(), app.portal.m365_hl_client_secret.value.clone());
            values.insert("M365_HEARTLAB_DIRECTORY_ID".to_string(), app.portal.m365_hl_tenant.value.clone());
        }
        "m365_medishift" => {
            values.insert("M365_MEDISHIFT_USER".to_string(), app.portal.m365_ms_user.value.clone());
            values.insert("M365_MEDISHIFT_APPLICATION_CLIENT_ID".to_string(), app.portal.m365_ms_client_id.value.clone());
            values.insert("M365_MEDISHIFT_CLIENT_VALUE".to_string(), app.portal.m365_ms_client_secret.value.clone());
            values.insert("M365_MEDISHIFT_DIRECTORY_ID".to_string(), app.portal.m365_ms_tenant.value.clone());
        }
        "services" => {
            values.insert("N8N_API_KEY".to_string(), app.portal.n8n_key.value.clone());
            values.insert("SUPABASE_ACCESS_TOKEN".to_string(), app.portal.supabase_token.value.clone());
            values.insert("SUPABASE_ANON_KEY".to_string(), app.portal.supabase_anon.value.clone());
        }
        _ => {}
    }
    let _ = env_store::save(&app.env_path, &values);
}

/// Read {PROVIDER}_CLIENT_ID from .env
fn get_client_id_for_provider(app: &App, provider: &OAuthProvider) -> String {
    let env = io_layer::env_store::load(&app.env_path);
    let key = format!("{}_CLIENT_ID", provider.env_prefix());
    env.get(&key).cloned().unwrap_or_default()
}

/// Read {PROVIDER}_CLIENT_SECRET from .env
fn get_client_secret_for_provider(app: &App, provider: &OAuthProvider) -> String {
    let env = io_layer::env_store::load(&app.env_path);
    let key = format!("{}_CLIENT_SECRET", provider.env_prefix());
    env.get(&key).cloned().unwrap_or_default()
}
