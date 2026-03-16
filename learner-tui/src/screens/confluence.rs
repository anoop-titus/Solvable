use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, List, ListItem, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

use crate::app::{App, ConfluenceFocus, SolveStatus};
use crate::theme;

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    if !app.confluence_state.loaded {
        f.render_widget(
            Paragraph::new("  Mesh DB not found or confluences table missing. Press 'r' to refresh.")
                .style(Style::default().fg(Color::Yellow))
                .block(theme::styled_block_accent("Confluence", Color::Magenta)),
            area,
        );
        return;
    }

    // Main vertical: stats bar (3) | met/unmet panels (min) | solved box (6)
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),   // stats bar
            Constraint::Min(8),     // met + unmet side-by-side
            Constraint::Length(8),   // solved box
        ])
        .split(area);

    render_stats_bar(f, app, vert[0]);
    render_met_unmet_panels(f, app, vert[1]);
    render_solved_box(f, app, vert[2]);
}

pub fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let focus_label = match app.confluence_state.focus {
        ConfluenceFocus::Met => "Met",
        ConfluenceFocus::Unmet => "Unmet",
        ConfluenceFocus::Solved => "Solved",
    };

    let status_span = match app.confluence_state.solve_status {
        SolveStatus::Idle => Span::styled("", theme::LABEL),
        SolveStatus::Solving(id) => Span::styled(
            format!("  Solving #{}...", id),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        SolveStatus::Solved(id) => Span::styled(
            format!("  Solved #{}", id),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
    };

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Focus: ", theme::LABEL),
            Span::styled(focus_label, Style::default().fg(Color::Cyan)),
            status_span,
            Span::styled(
                "  |  q: quit  r: refresh  Left/Right: panel  Up/Down: scroll  Tab: solved  Enter: solve",
                theme::LABEL,
            ),
        ])),
        area,
    );
}

fn render_stats_bar(f: &mut Frame, app: &App, area: Rect) {
    let data = &app.confluence_state.data;
    let met_count = data.met.len();
    let unmet_count = data.unmet.len();
    let gap_count = data.gap.len();
    let distant_count = data.distant.len();
    let stale_count = data.stale.len();
    let solved_count = app.confluence_state.solved_items.len();

    let spans = vec![
        Span::styled("  Met: ", theme::LABEL),
        Span::styled(
            format!("{}", met_count),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  Unmet: ", theme::LABEL),
        Span::styled(
            format!("{}", unmet_count),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  Gap: ", theme::LABEL),
        Span::styled(
            format!("{}", gap_count),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  Distant: ", theme::LABEL),
        Span::styled(
            format!("{}", distant_count),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  Stale: ", theme::LABEL),
        Span::styled(
            format!("{}", stale_count),
            theme::LABEL.add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  Solved: ", theme::LABEL),
        Span::styled(
            format!("{}", solved_count),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  (total: {})", app.confluence_state.data.total),
            theme::LABEL,
        ),
    ];

    let block = theme::styled_block_accent("Confluence", Color::Magenta);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn render_met_unmet_panels(f: &mut Frame, app: &mut App, area: Rect) {
    // Split horizontally: met (left) | unmet (right)
    let horiz = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_met_panel(f, app, horiz[0]);
    render_unmet_panel(f, app, horiz[1]);
}

fn render_met_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.confluence_state.focus == ConfluenceFocus::Met;
    let accent = if is_focused { Color::Green } else { Color::DarkGray };
    let solving_id = match app.confluence_state.solve_status {
        SolveStatus::Solving(id) => Some(id),
        _ => None,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            format!(" Met Confluences ({}) ", app.confluence_state.data.met.len()),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.confluence_state.data.met.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  No met confluences", theme::LABEL),
            ])),
            inner,
        );
        return;
    }

    let available_width = inner.width as usize;

    let items: Vec<ListItem> = app
        .confluence_state
        .data
        .met
        .iter()
        .map(|record| {
            let is_solving = solving_id == Some(record.id);
            let icon = if is_solving { "~ " } else { "# " };
            let icon_style = if is_solving {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Green)
            };

            let pair = format!(
                "{} <> {}",
                truncate(&record.issue_cluster_name, 16),
                truncate(&record.solution_cluster_name, 16),
            );
            let score_str = format!("  {:.2}", record.confluence_score);
            let content_width = pair.len() + score_str.len() + 2; // icon + spaces
            let pair_display = if content_width > available_width && available_width > 10 {
                truncate(&pair, available_width.saturating_sub(score_str.len() + 5))
            } else {
                pair
            };

            ListItem::new(Line::from(vec![
                Span::styled(icon, icon_style),
                Span::styled(pair_display, theme::DATA),
                Span::styled(
                    score_str,
                    Style::default().fg(Color::Green),
                ),
            ]))
        })
        .collect();

    f.render_stateful_widget(
        List::new(items).highlight_style(
            if is_focused { theme::HIGHLIGHT } else { Style::default() }
        ),
        inner,
        &mut app.confluence_state.met_list_state,
    );

    if app.confluence_state.data.met.len() > inner.height as usize {
        let mut ss = ScrollbarState::new(app.confluence_state.data.met.len())
            .position(app.confluence_state.met_selected);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            inner,
            &mut ss,
        );
    }
}

fn render_unmet_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.confluence_state.focus == ConfluenceFocus::Unmet;
    let accent = if is_focused { Color::Yellow } else { Color::DarkGray };
    let solving_id = match app.confluence_state.solve_status {
        SolveStatus::Solving(id) => Some(id),
        _ => None,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            format!(" Unmet Confluences ({}) ", app.confluence_state.data.unmet.len()),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.confluence_state.data.unmet.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  No unmet confluences", theme::LABEL),
            ])),
            inner,
        );
        return;
    }

    let available_width = inner.width as usize;

    let items: Vec<ListItem> = app
        .confluence_state
        .data
        .unmet
        .iter()
        .map(|record| {
            let is_solving = solving_id == Some(record.id);
            let icon = if is_solving { "~ " } else { "x " };
            let icon_style = if is_solving {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Red)
            };

            let pair = format!(
                "{} <> {}",
                truncate(&record.issue_cluster_name, 16),
                truncate(&record.solution_cluster_name, 16),
            );
            let score_str = format!("  {:.2}", record.confluence_score);
            let content_width = pair.len() + score_str.len() + 2;
            let pair_display = if content_width > available_width && available_width > 10 {
                truncate(&pair, available_width.saturating_sub(score_str.len() + 5))
            } else {
                pair
            };

            ListItem::new(Line::from(vec![
                Span::styled(icon, icon_style),
                Span::styled(pair_display, theme::DATA),
                Span::styled(
                    score_str,
                    Style::default().fg(Color::Yellow),
                ),
            ]))
        })
        .collect();

    f.render_stateful_widget(
        List::new(items).highlight_style(
            if is_focused { theme::HIGHLIGHT } else { Style::default() }
        ),
        inner,
        &mut app.confluence_state.unmet_list_state,
    );

    if app.confluence_state.data.unmet.len() > inner.height as usize {
        let mut ss = ScrollbarState::new(app.confluence_state.data.unmet.len())
            .position(app.confluence_state.unmet_selected);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            inner,
            &mut ss,
        );
    }
}

fn render_solved_box(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.confluence_state.focus == ConfluenceFocus::Solved;
    let accent = if is_focused { Color::Cyan } else { Color::DarkGray };

    let flash_id = match app.confluence_state.solve_status {
        SolveStatus::Solved(id) => Some(id),
        _ => None,
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            format!(" Solved ({}) ", app.confluence_state.solved_items.len()),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.confluence_state.solved_items.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  No solved confluences yet. Select a met/unmet item and press Enter to solve.", theme::LABEL),
            ])),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .confluence_state
        .solved_items
        .iter()
        .map(|solved| {
            let is_flashing = flash_id == Some(solved.id);
            let icon_style = if is_flashing {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default().fg(Color::Green)
            };
            let text_style = if is_flashing {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else {
                theme::DATA
            };

            ListItem::new(Line::from(vec![
                Span::styled("# ", icon_style),
                Span::styled(&solved.name, text_style),
                Span::styled(
                    format!(" -- solved via {} ({})", solved.method, solved.solved_at),
                    theme::LABEL,
                ),
            ]))
        })
        .collect();

    f.render_stateful_widget(
        List::new(items).highlight_style(
            if is_focused { theme::HIGHLIGHT } else { Style::default() }
        ),
        inner,
        &mut app.confluence_state.solved_list_state,
    );

    if app.confluence_state.solved_items.len() > inner.height as usize {
        let mut ss = ScrollbarState::new(app.confluence_state.solved_items.len())
            .position(app.confluence_state.solved_selected);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            inner,
            &mut ss,
        );
    }
}

// ──────────────── Helpers ────────────────

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max && max > 3 {
        format!("{}...", s.chars().take(max - 3).collect::<String>())
    } else {
        s.to_string()
    }
}
