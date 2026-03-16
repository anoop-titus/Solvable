mod app;
mod theme;
mod ui;

use std::io;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::{App, Tab};
use ui::PanelAreas;

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
    let mut tick_since_refresh: u64 = 0;

    terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas))?;

    loop {
        if event::poll(Duration::from_millis(TICK_MS))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind != KeyEventKind::Press { continue; }
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                        KeyCode::Char('r') => {
                            app.refresh();
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas))?;
                        }
                        KeyCode::Tab => {
                            app.next_tab();
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas))?;
                        }
                        KeyCode::Up => {
                            match app.current_tab {
                                Tab::Learnings => app.scroll_recent_learnings(-1),
                                Tab::Research => app.scroll_research_issues(-1),
                            }
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas))?;
                        }
                        KeyCode::Down => {
                            match app.current_tab {
                                Tab::Learnings => app.scroll_recent_learnings(1),
                                Tab::Research => app.scroll_research_issues(1),
                            }
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas))?;
                        }
                        _ => {}
                    }
                }
                Event::Mouse(mouse) => {
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
                            }
                            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas))?;
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
            terminal.draw(|f| ui::render(f, &mut app, &mut panel_areas))?;
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
