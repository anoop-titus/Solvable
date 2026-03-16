use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

use crate::app::{App, IssueFocus};
use crate::theme;

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    if app.research_db_missing || !app.issues_state.loaded {
        f.render_widget(
            Paragraph::new("  Research DB not found or not loaded. Press 'r' to refresh.")
                .style(Style::default().fg(Color::Yellow))
                .block(theme::styled_block_accent("Issues", Color::Red)),
            area,
        );
        return;
    }

    // Main vertical split: stats bar (3 lines) | body | detail (variable)
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // stats bar
            Constraint::Min(8),    // filter panel + issue list
            Constraint::Length(10), // detail panel
        ])
        .split(area);

    render_stats_bar(f, app, vert[0]);

    // Body: filter panel (left) + issue list (right)
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(18), Constraint::Min(20)])
        .split(vert[1]);

    render_filter_panel(f, app, body[0]);
    render_issue_list(f, app, body[1]);
    render_detail_panel(f, app, vert[2]);
}

pub fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let filtered = app.issues_state.filtered_indices.len();
    let total = app.issues_state.issues.len();
    let focus_label = match app.issues_state.focus {
        IssueFocus::Filters => "Filters",
        IssueFocus::List => "List",
        IssueFocus::Detail => "Detail",
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Showing: ", theme::LABEL),
            Span::styled(format!("{}/{}", filtered, total), theme::DATA),
            Span::styled("  Focus: ", theme::LABEL),
            Span::styled(focus_label, Style::default().fg(Color::Cyan)),
            Span::styled(
                "  |  q: quit  r: refresh  Tab/1-8: switch  Up/Down: navigate  Left/Right: focus  Enter: toggle filter",
                theme::LABEL,
            ),
        ])),
        area,
    );
}

fn render_stats_bar(f: &mut Frame, app: &App, area: Rect) {
    let stats = &app.issues_state.stats;
    let mut spans = vec![
        Span::styled("  Total: ", theme::LABEL),
        Span::styled(
            format!("{}", stats.total),
            theme::DATA.add_modifier(Modifier::BOLD),
        ),
    ];

    // Add severity breakdown inline
    for (sev, count) in &stats.by_severity {
        let color = severity_color(sev);
        spans.push(Span::styled("  ", theme::LABEL));
        spans.push(Span::styled(
            capitalize(sev),
            Style::default().fg(color),
        ));
        spans.push(Span::styled(": ", theme::LABEL));
        spans.push(Span::styled(
            format!("{}", count),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }

    let block = theme::styled_block_accent("Issues", Color::Red);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn render_filter_panel(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.issues_state.focus == IssueFocus::Filters;
    let accent = if is_focused { Color::Cyan } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            " Filters ",
            Style::default()
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 6 || inner.width < 10 {
        return;
    }

    // Lay out 3 filter dropdowns vertically
    let filter_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(inner);

    render_inline_dropdown(f, &app.issues_state.severity_filter, filter_layout[0],
        is_focused && app.issues_state.active_filter == 0);
    render_inline_dropdown(f, &app.issues_state.status_filter, filter_layout[1],
        is_focused && app.issues_state.active_filter == 1);
    render_inline_dropdown(f, &app.issues_state.category_filter, filter_layout[2],
        is_focused && app.issues_state.active_filter == 2);

    // Render expanded dropdown overlay on top (must be last to appear above)
    if is_focused {
        let filters = [
            (&app.issues_state.severity_filter, filter_layout[0]),
            (&app.issues_state.status_filter, filter_layout[1]),
            (&app.issues_state.category_filter, filter_layout[2]),
        ];
        for (filter, filter_area) in &filters {
            if filter.expanded {
                render_dropdown_overlay(f, filter, *filter_area);
            }
        }
    }
}

fn render_inline_dropdown(f: &mut Frame, filter: &crate::app::DropdownFilter, area: Rect, active: bool) {
    let border_style = if active {
        theme::INPUT_BORDER_FOCUSED
    } else {
        theme::INPUT_BORDER
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Span::styled(
            format!(" {} ", filter.label),
            if active {
                theme::INPUT_BORDER_FOCUSED.add_modifier(Modifier::BOLD)
            } else {
                theme::LABEL
            },
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let indicator = if filter.expanded { "\u{25b2}" } else { "\u{25bc}" };
    let display_val = filter.selected_value();
    let max_w = inner.width as usize;
    let val_display = if display_val.len() + 2 > max_w {
        format!("{}...", &display_val[..max_w.saturating_sub(4)])
    } else {
        display_val.to_string()
    };

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} ", val_display), theme::INPUT_TEXT),
            Span::styled(indicator, theme::LABEL),
        ])),
        inner,
    );
}

fn render_dropdown_overlay(f: &mut Frame, filter: &crate::app::DropdownFilter, trigger_area: Rect) {
    let list_height = (filter.options.len() as u16).min(8);
    let overlay_area = Rect::new(
        trigger_area.x,
        trigger_area.y + trigger_area.height,
        trigger_area.width,
        list_height + 2,
    );
    f.render_widget(Clear, overlay_area);
    let items: Vec<ListItem> = filter
        .options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let style = if i == filter.selected_index {
                theme::HIGHLIGHT
            } else {
                theme::DATA
            };
            ListItem::new(Line::from(Span::styled(format!(" {} ", opt), style)))
        })
        .collect();
    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(theme::INPUT_BORDER_FOCUSED),
        ),
        overlay_area,
    );
}

fn render_issue_list(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.issues_state.focus == IssueFocus::List;
    let accent = if is_focused { Color::Cyan } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            format!(" Issue List ({}) ", app.issues_state.filtered_indices.len()),
            Style::default()
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.issues_state.filtered_indices.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  No issues match filters",
                theme::LABEL,
            )])),
            inner,
        );
        return;
    }

    let available_width = inner.width as usize;
    let prefix_width = 12;
    let max_title_chars = available_width.saturating_sub(prefix_width);

    let items: Vec<ListItem> = app
        .issues_state
        .filtered_indices
        .iter()
        .map(|&idx| {
            let issue = &app.issues_state.issues[idx];
            let icon = severity_icon(&issue.severity);
            let status_style = status_color(&issue.status);
            let status_abbrev = if issue.status.len() >= 3 {
                &issue.status[..3]
            } else {
                &issue.status
            };
            let title_display = if issue.title.chars().count() > max_title_chars && max_title_chars > 3
            {
                format!(
                    "{}...",
                    issue.title.chars().take(max_title_chars - 3).collect::<String>()
                )
            } else {
                issue.title.clone()
            };

            ListItem::new(Line::from(vec![
                icon,
                Span::styled(format!("[{}] ", status_abbrev), status_style),
                Span::styled(title_display, theme::DATA),
            ]))
        })
        .collect();

    f.render_stateful_widget(
        List::new(items).highlight_style(theme::HIGHLIGHT),
        inner,
        &mut app.issues_state.list_state,
    );

    if app.issues_state.filtered_indices.len() > inner.height as usize {
        let mut ss = ScrollbarState::new(app.issues_state.filtered_indices.len())
            .position(app.issues_state.selected_index);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            inner,
            &mut ss,
        );
    }
}

fn render_detail_panel(f: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.issues_state.focus == IssueFocus::Detail;
    let accent = if is_focused { Color::Cyan } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .title(Span::styled(
            " Detail ",
            Style::default()
                .fg(accent)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let issue = match app.issues_state.selected_issue() {
        Some(i) => i,
        None => {
            f.render_widget(
                Paragraph::new("  Select an issue to view details").style(theme::LABEL),
                inner,
            );
            return;
        }
    };

    let sev_color = severity_color(&issue.severity);
    let sol_label = if issue.solution_count == 1 {
        "solution".to_string()
    } else {
        "solutions".to_string()
    };
    let cluster_line = match &issue.cluster_name {
        Some(name) => Line::from(vec![
            Span::styled("  Cluster: ", theme::LABEL),
            Span::styled(name.as_str(), Style::default().fg(Color::Magenta)),
        ]),
        None => Line::from(vec![
            Span::styled("  Cluster: ", theme::LABEL),
            Span::styled("(none)", theme::LABEL),
        ]),
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("  #{}: ", issue.id), theme::LABEL),
            Span::styled(
                issue.title.as_str(),
                theme::DATA.add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Category: ", theme::LABEL),
            Span::styled(issue.category.as_str(), theme::DATA),
            Span::styled("  Severity: ", theme::LABEL),
            Span::styled(
                issue.severity.as_str(),
                Style::default().fg(sev_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Status: ", theme::LABEL),
            Span::styled(issue.status.as_str(), status_color(&issue.status)),
            Span::styled(format!("  ({} {})", issue.solution_count, sol_label), theme::LABEL),
        ]),
        Line::from(vec![
            Span::styled("  Created: ", theme::LABEL),
            Span::styled(issue.created_at.as_str(), theme::DATA),
            Span::styled("  Updated: ", theme::LABEL),
            Span::styled(issue.updated_at.as_str(), theme::DATA),
        ]),
        cluster_line,
        Line::from(vec![]),
    ];

    // Word-wrap the description to fit the inner width
    let desc = &issue.description;
    if !desc.is_empty() {
        let wrap_width = inner.width.saturating_sub(4) as usize;
        if wrap_width > 0 {
            for chunk in wrap_text(desc, wrap_width) {
                lines.push(Line::from(vec![
                    Span::styled("  ", theme::LABEL),
                    Span::styled(chunk, theme::DATA),
                ]));
            }
        }
    } else {
        lines.push(Line::from(vec![Span::styled(
            "  (no description)",
            theme::LABEL,
        )]));
    }

    let scroll = app.issues_state.detail_scroll;
    f.render_widget(
        Paragraph::new(lines).scroll((scroll, 0)),
        inner,
    );
}

// ──────────────── Helpers ────────────────

fn severity_icon(severity: &str) -> Span<'static> {
    match severity {
        "critical" => Span::styled(
            " ! ",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
        "high" => Span::styled(
            " ! ",
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        ),
        "medium" => Span::styled(" * ", Style::default().fg(Color::Yellow)),
        _ => Span::styled(" . ", theme::SUCCESS),
    }
}

fn severity_color(severity: &str) -> Color {
    match severity {
        "critical" => Color::Red,
        "high" => Color::LightRed,
        "medium" => Color::Yellow,
        "low" => Color::Green,
        _ => Color::DarkGray,
    }
}

fn status_color(status: &str) -> Style {
    match status {
        "solved" => theme::SUCCESS,
        "researching" => Style::default().fg(Color::Yellow),
        "dispatched" => theme::DATA,
        "open" => Style::default().fg(Color::LightRed),
        _ => theme::LABEL,
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + c.as_str(),
    }
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        let mut current_line = String::new();
        for word in words {
            if current_line.is_empty() {
                current_line = word.to_string();
            } else if current_line.len() + 1 + word.len() <= width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }
    lines
}
