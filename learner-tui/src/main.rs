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

use app::{App, ConfluenceFocus, IssueFocus, Screen, SolveFocus, SolveProgress, SolveStatus, Tab};
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

fn main() -> io::Result<()> {
    let db_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_DB_PATH.to_string());

    let research_db_path = std::env::args()
        .nth(2)
        .unwrap_or_else(|| RESEARCH_DB_PATH.to_string());

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(db_path, research_db_path);
    let mut panel_areas = PanelAreas::default();
    let mut tab_bar_state = TabBarState::default();
    let mut tick_since_refresh: u64 = 0;

    // OAuth background state
    let oauth_result: OAuthResult = Arc::new(Mutex::new(None));
    let mut device_poll: Option<DevicePollState> = None;

    terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;

    loop {
        if event::poll(Duration::from_millis(TICK_MS))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press { continue; }

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
                            }
                            KeyCode::BackTab => {
                                app.portal.retreat_focus();
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

                    // Issues tab captures input for focus navigation and filter dropdowns
                    if app.current_tab == Tab::Issues {
                        let mut handled = true;
                        match key.code {
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

                    // Solutions tab captures input for list navigation
                    if app.current_tab == Tab::Solutions {
                        let mut handled = true;
                        match key.code {
                            KeyCode::Up => {
                                app.solutions_state.scroll_list(-1);
                            }
                            KeyCode::Down => {
                                app.solutions_state.scroll_list(1);
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
                                        if app.solve_state.active_button < 2 {
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
                            KeyCode::Enter => {
                                match app.solve_state.focus {
                                    SolveFocus::AiActions => {
                                        match app.solve_state.active_button {
                                            0 => {
                                                // Solve
                                                if app.solve_state.progress == SolveProgress::Idle {
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
                                            _ => {}
                                        }
                                    }
                                    SolveFocus::AiList => {
                                        // Enter on AI list goes to actions
                                        app.solve_state.focus = SolveFocus::AiActions;
                                    }
                                    _ => { handled = false; }
                                }
                            }
                            KeyCode::Char('s') => {
                                // Quick solve shortcut
                                if app.solve_state.progress == SolveProgress::Idle {
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
                            KeyCode::Char('a') => {
                                // Select all AI items
                                let all_checked = app.solve_state.ai_items.iter().all(|i| i.checked);
                                for item in &mut app.solve_state.ai_items {
                                    item.checked = !all_checked;
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
                Event::Mouse(mouse) => {
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
                                    match app.solve_state.focus {
                                        SolveFocus::AiList | SolveFocus::AiActions => app.solve_state.scroll_ai(delta),
                                        SolveFocus::HumanList => app.solve_state.scroll_human(delta),
                                        SolveFocus::Solved => app.solve_state.scroll_solved(delta),
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
        }
        "imap" => {
            values.insert("IMAP_HOST".to_string(), app.portal.imap_host.value.clone());
            values.insert("IMAP_PORT".to_string(), app.portal.imap_port.value.clone());
            values.insert("IMAP_USER".to_string(), app.portal.imap_user.value.clone());
            values.insert("IMAP_PASS".to_string(), app.portal.imap_pass.value.clone());
        }
        "airtable" => {
            values.insert("AIRTABLE_API_KEY".to_string(), app.portal.airtable_key.value.clone());
            values.insert("AIRTABLE_BASE_ID".to_string(), app.portal.airtable_base.value.clone());
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
