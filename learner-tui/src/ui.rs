use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Gauge, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget},
    Frame,
};

use crate::app::{App, RunProgress, Tab};
use crate::theme;

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

pub fn render(f: &mut Frame, app: &mut App, panel_areas: &mut PanelAreas) {
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

    render_tab_bar(f, app, main_layout[0]);

    match app.current_tab {
        Tab::Learnings => render_learnings_tab(f, app, panel_areas, main_layout[1]),
        Tab::Research => render_research_tab(f, app, panel_areas, main_layout[1]),
        _ => render_stub_tab(f, app.current_tab, main_layout[1]),
    }

    match app.current_tab {
        Tab::Learnings => render_learnings_footer(f, app, main_layout[2]),
        Tab::Research => render_research_footer(f, app, main_layout[2]),
        _ => render_stub_footer(f, app, main_layout[2]),
    }
}

fn render_tab_bar(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::styled("  ", theme::LABEL)];
    for (i, tab) in Tab::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", theme::LABEL));
        }
        let style = if app.current_tab == *tab {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD).add_modifier(Modifier::REVERSED)
        } else {
            theme::LABEL
        };
        spans.push(Span::styled(format!(" {} ", tab.label()), style));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ──────────────── LEARNINGS TAB ────────────────

fn render_learnings_tab(f: &mut Frame, app: &mut App, panel_areas: &mut PanelAreas, area: Rect) {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(25), Constraint::Percentage(45)])
        .split(area);

    let top = Layout::default().direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)]).split(vert[0]);
    render_sources(f, app, top[0]);
    let tick = app.tick_count;
    render_run_panel(f, "Dropbox Runs", &app.dropbox_runs, &mut app.dropbox_runs_state, top[1], tick);
    panel_areas.dropbox_runs = top[1];

    let mid = Layout::default().direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)]).split(vert[1]);
    render_agents(f, app, mid[0]);
    render_run_panel(f, "Email Runs", &app.email_runs, &mut app.email_runs_state, mid[1], tick);
    panel_areas.email_runs = mid[1];

    render_recent_learnings(f, app, vert[2]);
    panel_areas.recent_learnings = vert[2];
}

fn render_sources(f: &mut Frame, app: &App, area: Rect) {
    let block = theme::styled_block("Sources");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let items: Vec<ListItem> = app.source_counts.iter().map(|(name, count)| {
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<14}", name), theme::DATA),
            Span::styled(format!("{:>6}", count), theme::DATA.add_modifier(Modifier::BOLD)),
        ]))
    }).collect();
    f.render_widget(List::new(items), inner);
}

struct RadarBar { tick: u64 }
impl RadarBar { fn new(tick: u64) -> Self { Self { tick } } }
impl Widget for RadarBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let width = area.width as usize;
        if width < 4 { return; }
        let beam_width = 3.min(width);
        let travel = width.saturating_sub(beam_width);
        if travel == 0 { return; }
        let cycle = travel * 2;
        let pos_in_cycle = (self.tick as usize) % cycle;
        let beam_start = if pos_in_cycle < travel { pos_in_cycle } else { cycle - pos_in_cycle };
        let dim_style = Style::default().fg(Color::DarkGray);
        let beam_style = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
        let glow_style = Style::default().fg(Color::Blue);
        let y = area.y;
        for x_offset in 0..width {
            let x = area.x + x_offset as u16;
            let dist_to_beam = if x_offset >= beam_start && x_offset < beam_start + beam_width { 0 }
                else if x_offset < beam_start { beam_start - x_offset }
                else { x_offset - (beam_start + beam_width - 1) };
            let (ch, style) = if dist_to_beam == 0 { ('▓', beam_style) }
                else if dist_to_beam <= 2 { ('░', glow_style) }
                else { ('░', dim_style) };
            if let Some(cell) = buf.cell_mut((x, y)) { cell.set_char(ch); cell.set_style(style); }
        }
    }
}

fn render_run_panel(f: &mut Frame, title: &str, runs: &[RunProgress], state: &mut ListState, area: Rect, tick: u64) {
    let block = theme::styled_block(title);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if runs.is_empty() {
        f.render_widget(Paragraph::new(Line::from(vec![Span::styled("  all idle", theme::LABEL)])), inner);
        return;
    }

    let constraints: Vec<Constraint> = runs.iter().flat_map(|_| [Constraint::Length(1), Constraint::Length(1)]).collect();
    let rows = Layout::default().direction(Direction::Vertical).constraints(constraints).split(inner);

    for (i, run) in runs.iter().enumerate() {
        let is_watching = run.status == "watching";
        let pct = if run.total_files > 0 { (run.processed as f64 / run.total_files as f64 * 100.0) as u16 } else { 0 };
        let label_text = if run.folder.is_empty() { run.agent.clone() } else { format!("{} / {}", run.agent, run.folder) };
        let pid_str = if run.pid > 0 { format!("  [PID {}]", run.pid) } else { String::new() };
        let status_str = if is_watching {
            format!("  Watching...{}", pid_str)
        } else {
            format!("  {}/{}{}", run.processed, run.total_files, pid_str)
        };

        let row_label = i * 2;
        let row_gauge = i * 2 + 1;
        if row_gauge >= rows.len() { break; }

        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled("  ", theme::LABEL),
            Span::styled(&label_text, theme::SUCCESS),
            Span::styled(status_str, theme::LABEL),
        ])), rows[row_label]);

        let gauge_area = Rect { x: rows[row_gauge].x + 2, width: rows[row_gauge].width.saturating_sub(4), ..rows[row_gauge] };
        if is_watching {
            f.render_widget(RadarBar::new(tick), gauge_area);
        } else {
            f.render_widget(Gauge::default().gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray)).percent(pct).label(format!("{}%", pct)), gauge_area);
        }
    }

    let visible_slots = inner.height as usize / 2;
    if runs.len() > visible_slots {
        let mut scrollbar_state = ScrollbarState::new(runs.len()).position(state.selected().unwrap_or(0));
        f.render_stateful_widget(Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(None).end_symbol(None), inner, &mut scrollbar_state);
    }
}

fn render_agents(f: &mut Frame, app: &App, area: Rect) {
    let block = theme::styled_block("Agents");
    let inner = block.inner(area);
    f.render_widget(block, area);
    let items: Vec<ListItem> = app.agent_counts.iter().map(|(name, count)| {
        ListItem::new(Line::from(vec![
            Span::styled(format!("  {:<18}", name), theme::DATA),
            Span::styled(format!("{:>5}", count), theme::DATA.add_modifier(Modifier::BOLD)),
        ]))
    }).collect();
    f.render_widget(List::new(items), inner);
}

fn render_recent_learnings(f: &mut Frame, app: &mut App, area: Rect) {
    let block = theme::styled_block("Recent Learnings");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let available_width = inner.width as usize;
    let prefix_width = 7 + 16 + 2;
    let max_learning_chars = available_width.saturating_sub(prefix_width);

    let items: Vec<ListItem> = app.recent_learnings.iter().map(|l| {
        let learning_display = if l.learning.chars().count() > max_learning_chars && max_learning_chars > 3 {
            format!("{}...", l.learning.chars().take(max_learning_chars - 3).collect::<String>())
        } else { l.learning.clone() };
        ListItem::new(Line::from(vec![
            Span::styled(format!(" {} ", l.processed_at), theme::LABEL),
            Span::styled(format!("{}: ", l.agent), Style::default().fg(Color::Cyan)),
            Span::styled(learning_display, theme::DATA),
        ]))
    }).collect();

    f.render_stateful_widget(List::new(items).highlight_style(theme::HIGHLIGHT), inner, &mut app.recent_learnings_state);

    let visible_lines = inner.height as usize;
    if app.recent_learnings.len() > visible_lines {
        let mut scrollbar_state = ScrollbarState::new(app.recent_learnings.len()).position(app.recent_learnings_state.selected().unwrap_or(0));
        f.render_stateful_widget(Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(None).end_symbol(None), inner, &mut scrollbar_state);
    }
}

fn render_learnings_footer(f: &mut Frame, app: &App, area: Rect) {
    f.render_widget(Paragraph::new(Line::from(vec![
        Span::styled("  Total: ", theme::LABEL),
        Span::styled(format!("{}", app.total_learnings), theme::DATA),
        Span::styled("  DB: ", theme::LABEL),
        Span::styled(app.format_db_size(), theme::DATA),
        Span::styled("  Updated: ", theme::LABEL),
        Span::styled(&app.last_refresh, theme::DATA),
        Span::styled("  |  q: quit  r: refresh  Tab/1-8: switch  scroll: mouse", theme::LABEL),
    ])), area);
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
