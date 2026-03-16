use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
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
        Tab::Learnings => crate::screens::learnings::render(f, app, panel_areas, main_layout[1]),
        Tab::Research => render_research_tab(f, app, panel_areas, main_layout[1]),
        Tab::Portal => crate::screens::portal::render(f, &mut app.portal, main_layout[1]),
        _ => render_stub_tab(f, app.current_tab, main_layout[1]),
    }

    match app.current_tab {
        Tab::Learnings => crate::screens::learnings::render_footer(f, app, main_layout[2]),
        Tab::Research => render_research_footer(f, app, main_layout[2]),
        _ => render_stub_footer(f, app, main_layout[2]),
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

// ──────────────── RESEARCH TAB ────────────────

fn render_research_tab(f: &mut Frame, app: &mut App, panel_areas: &mut PanelAreas, area: Rect) {
    if app.research_db_missing {
        f.render_widget(Paragraph::new("Research DB not found. Run 'research --once' to initialize.")
            .style(Style::default().fg(Color::Yellow))
            .block(theme::styled_block_accent("Research", Color::Magenta)), area);
        return;
    }

    let vert = Layout::default().direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)]).split(area);

    let top = Layout::default().direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)]).split(vert[0]);

    render_research_stats(f, app, top[0]);
    render_research_issues(f, app, top[1]);
    panel_areas.research_issues = top[1];

    render_research_solutions(f, app, vert[1]);
    panel_areas.research_solutions = vert[1];
}

fn render_research_stats(f: &mut Frame, app: &App, area: Rect) {
    let block = theme::styled_block_accent("Research Stats", Color::Magenta);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let s = &app.research_stats;
    f.render_widget(Paragraph::new(vec![
        Line::from(vec![Span::styled("  Total Issues:  ", theme::LABEL), Span::styled(format!("{}", s.total_issues), theme::DATA.add_modifier(Modifier::BOLD))]),
        Line::from(vec![Span::styled("  Open:          ", theme::LABEL), Span::styled(format!("{}", s.open_issues), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        Line::from(vec![Span::styled("  Solved:        ", theme::LABEL), Span::styled(format!("{}", s.solved_issues), theme::SUCCESS.add_modifier(Modifier::BOLD))]),
        Line::from(vec![Span::styled("  Solutions:     ", theme::LABEL), Span::styled(format!("{}", s.total_solutions), theme::DATA.add_modifier(Modifier::BOLD))]),
        Line::from(vec![Span::styled("  Pending:       ", theme::LABEL), Span::styled(format!("{}", s.pending_digest), theme::MAGENTA)]),
        Line::from(vec![]),
        Line::from(vec![Span::styled("  Last Scan:     ", theme::LABEL), Span::styled(&s.last_scan_at, theme::DATA)]),
        Line::from(vec![Span::styled("  Last Digest:   ", theme::LABEL), Span::styled(&s.last_digest_at, theme::DATA)]),
    ]), inner);
}

fn render_research_issues(f: &mut Frame, app: &mut App, area: Rect) {
    let block = theme::styled_block_accent("Recent Issues", Color::Magenta);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.research_issues.is_empty() {
        f.render_widget(Paragraph::new(Line::from(vec![Span::styled("  No issues detected yet", theme::LABEL)])), inner);
        return;
    }

    let available_width = inner.width as usize;
    let prefix_width = 12;
    let max_title_chars = available_width.saturating_sub(prefix_width);

    let items: Vec<ListItem> = app.research_issues.iter().map(|issue| {
        let icon = match issue.severity.as_str() {
            "critical" | "high" => Span::styled(" ! ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            "medium" => Span::styled(" * ", Style::default().fg(Color::Yellow)),
            _ => Span::styled(" . ", theme::SUCCESS),
        };
        let status_style = match issue.status.as_str() {
            "solved" => theme::SUCCESS,
            "researching" => Style::default().fg(Color::Yellow),
            _ => theme::DATA,
        };
        let title_display = if issue.title.chars().count() > max_title_chars && max_title_chars > 3 {
            format!("{}...", issue.title.chars().take(max_title_chars - 3).collect::<String>())
        } else { issue.title.clone() };

        ListItem::new(Line::from(vec![
            icon,
            Span::styled(format!("[{}] ", &issue.status[..3.min(issue.status.len())]), status_style),
            Span::styled(title_display, theme::DATA),
        ]))
    }).collect();

    f.render_stateful_widget(List::new(items).highlight_style(theme::HIGHLIGHT), inner, &mut app.research_issues_state);

    if app.research_issues.len() > inner.height as usize {
        let mut ss = ScrollbarState::new(app.research_issues.len()).position(app.research_issues_state.selected().unwrap_or(0));
        f.render_stateful_widget(Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(None).end_symbol(None), inner, &mut ss);
    }
}

fn render_research_solutions(f: &mut Frame, app: &mut App, area: Rect) {
    let block = theme::styled_block_accent("Recent Solutions", Color::Magenta);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.research_solutions.is_empty() {
        f.render_widget(Paragraph::new(Line::from(vec![Span::styled("  No solutions found yet", theme::LABEL)])), inner);
        return;
    }

    let available_width = inner.width as usize;
    let prefix_width = 28;
    let max_summary_chars = available_width.saturating_sub(prefix_width);

    let items: Vec<ListItem> = app.research_solutions.iter().map(|sol| {
        let conf_style = match sol.confidence.as_str() {
            "high" => theme::SUCCESS,
            "medium" => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::Red),
        };
        let issue_short: String = sol.issue_title.chars().take(20).collect();
        let summary_display = if sol.summary.chars().count() > max_summary_chars && max_summary_chars > 3 {
            format!("{}...", sol.summary.chars().take(max_summary_chars - 3).collect::<String>())
        } else { sol.summary.clone() };

        ListItem::new(Line::from(vec![
            Span::styled(format!(" {} ", sol.created_at), theme::LABEL),
            Span::styled(format!("[{}] ", &sol.confidence[..3.min(sol.confidence.len())]), conf_style),
            Span::styled(format!("{} -> ", issue_short), theme::MAGENTA),
            Span::styled(summary_display, theme::DATA),
        ]))
    }).collect();

    f.render_stateful_widget(List::new(items).highlight_style(theme::HIGHLIGHT), inner, &mut app.research_solutions_state);

    if app.research_solutions.len() > inner.height as usize {
        let mut ss = ScrollbarState::new(app.research_solutions.len()).position(app.research_solutions_state.selected().unwrap_or(0));
        f.render_stateful_widget(Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(None).end_symbol(None), inner, &mut ss);
    }
}

fn render_research_footer(f: &mut Frame, app: &App, area: Rect) {
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled("  Issues: ", theme::LABEL),
        Span::styled(format!("{}", app.research_stats.total_issues), theme::DATA),
        Span::styled("  Solutions: ", theme::LABEL),
        Span::styled(format!("{}", app.research_stats.total_solutions), theme::DATA),
        Span::styled("  DB: ", theme::LABEL),
        Span::styled(app.format_research_db_size(), theme::DATA),
        Span::styled("  Updated: ", theme::LABEL),
        Span::styled(&app.last_refresh, theme::DATA),
        Span::styled("  |  q: quit  r: refresh  Tab/1-8: switch  scroll: mouse", theme::LABEL),
    ])), area);
}

// ──────────────── STUB TABS ────────────────

fn render_stub_tab(f: &mut Frame, tab: Tab, area: Rect) {
    let msg = format!("  {} \u{2014} Coming soon", tab.label());
    f.render_widget(
        Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .block(theme::styled_block(tab.label())),
        area,
    );
}

fn render_stub_footer(f: &mut Frame, app: &App, area: Rect) {
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled("  Updated: ", theme::LABEL),
        Span::styled(&app.last_refresh, theme::DATA),
        Span::styled("  |  q: quit  r: refresh  Tab/1-8: switch", theme::LABEL),
    ])), area);
}
