use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::app::{App, Screen, Tab};
use crate::screens;
use crate::theme;
use crate::widgets::tab_bar::TabBarState;

pub struct PanelAreas {
    pub dropbox_runs: Rect,
    pub email_runs: Rect,
    pub recent_learnings: Rect,
    pub research_issues: Rect,
    pub research_solutions: Rect,
}

impl Default for PanelAreas {
    fn default() -> Self {
        Self {
            dropbox_runs: Rect::default(),
            email_runs: Rect::default(),
            recent_learnings: Rect::default(),
            research_issues: Rect::default(),
            research_solutions: Rect::default(),
        }
    }
}

pub fn render(f: &mut Frame, app: &mut App, panel_areas: &mut PanelAreas, tab_bar_state: &mut TabBarState) {
    if app.screen == Screen::Welcome {
        screens::welcome::render(f, f.area());
        return;
    }

    if app.db_missing && app.current_tab == Tab::Learnings {
        let msg = Paragraph::new("No database found. Waiting...")
            .style(Style::default().fg(Color::Yellow))
            .block(theme::styled_block("Solvable").title_alignment(ratatui::layout::Alignment::Center));
        f.render_widget(msg, f.area());
        return;
    }

    let outer = theme::styled_block("Solvable")
        .title_alignment(ratatui::layout::Alignment::Center);
    let outer_area = f.area();
    let inner_area = outer.inner(outer_area);
    f.render_widget(outer, outer_area);

    // Tab bar (1 line) + content + footer (1 line)
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(3), Constraint::Length(1)])
        .split(inner_area);

    render_tab_bar(f, app, main_layout[0], tab_bar_state);

    match app.current_tab {
        Tab::Learnings   => screens::learnings::render(f, app, panel_areas, main_layout[1]),
        Tab::Research    => screens::research::render(f, app, panel_areas, main_layout[1]),
        Tab::Portal      => screens::portal::render(f, &mut app.portal, main_layout[1]),
        Tab::Issues      => screens::issues::render(f, app, main_layout[1]),
        Tab::Solutions   => screens::solutions::render(f, app, main_layout[1]),
        Tab::Confluence  => screens::confluence::render(f, app, main_layout[1]),
        Tab::Solve       => screens::solve::render(f, main_layout[1]),
        Tab::Settings    => screens::settings::render(f, &mut app.settings, main_layout[1]),
    }

    match app.current_tab {
        Tab::Learnings   => screens::learnings::render_footer(f, app, main_layout[2]),
        Tab::Research    => screens::research::render_footer(f, app, main_layout[2]),
        Tab::Issues      => screens::issues::render_footer(f, app, main_layout[2]),
        Tab::Solutions   => screens::solutions::render_footer(f, app, main_layout[2]),
        Tab::Confluence  => screens::confluence::render_footer(f, app, main_layout[2]),
        Tab::Settings    => screens::settings::render_footer(f, &app.settings, main_layout[2]),
        _                => render_stub_footer(f, app, main_layout[2]),
    }
}

fn render_tab_bar(f: &mut Frame, app: &App, area: Rect, tab_bar_state: &mut TabBarState) {
    let mut spans = vec![Span::styled("  ", theme::LABEL)];
    let mut x_offset = area.x + 2; // account for leading "  " padding
    for (i, tab) in Tab::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", theme::LABEL));
            x_offset += 3; // " | " separator width
        }
        let style = if app.current_tab == *tab {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD).add_modifier(Modifier::REVERSED)
        } else {
            theme::LABEL
        };
        let label = format!(" {} ", tab.label());
        let label_width = label.len() as u16;
        tab_bar_state.tab_rects[i] = Rect::new(x_offset, area.y, label_width, 1);
        spans.push(Span::styled(label, style));
        x_offset += label_width;
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ──────────────── SHARED FOOTER ────────────────

fn render_stub_footer(f: &mut Frame, app: &App, area: Rect) {
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled("  Updated: ", theme::LABEL),
        Span::styled(&app.last_refresh, theme::DATA),
        Span::styled("  |  q: quit  r: refresh  Tab/1-8: switch", theme::LABEL),
    ])), area);
}
