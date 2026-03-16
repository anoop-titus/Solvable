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

use app::{App, Screen, Tab};
use io_layer::oauth::{self, OAuthProvider, OAuthStatus, DeviceFlowState};
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
                                _ => {} // stub tabs have no scrollable content yet
                            }
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas, &mut tab_bar_state))?;
                        }
                        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                            if let Some(tab) = tab_bar_state.hit_test(mouse.column, mouse.row) {
                                app.set_tab(tab);
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
