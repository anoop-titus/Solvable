use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::app::App;
use crate::theme;
use crate::ui::PanelAreas;

pub fn render(f: &mut Frame, app: &mut App, panel_areas: &mut PanelAreas, area: Rect) {
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

pub fn render_footer(f: &mut Frame, app: &App, area: Rect) {
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
