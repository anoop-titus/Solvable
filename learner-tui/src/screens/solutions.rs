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

use crate::app::App;
use crate::theme;

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    if app.research_db_missing || !app.solutions_state.loaded {
        f.render_widget(
            Paragraph::new("  Research DB not found or not loaded. Press 'r' to refresh.")
                .style(Style::default().fg(Color::Yellow))
                .block(theme::styled_block_accent("Solutions", Color::Green)),
            area,
        );
        return;
    }

    // Main vertical: stats (3) | solution list (min) | detail (10)
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // stats bar
            Constraint::Min(6),    // solution list
            Constraint::Length(12), // detail panel
        ])
        .split(area);

    render_stats_bar(f, app, vert[0]);
    render_solution_list(f, app, vert[1]);
    render_detail_panel(f, app, vert[2]);
}

pub fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let total = app.solutions_state.solutions.len();
    let selected = if total > 0 {
        format!("{}/{}", app.solutions_state.selected_index + 1, total)
    } else {
        "0/0".to_string()
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Solutions: ", theme::LABEL),
            Span::styled(selected, theme::DATA),
            Span::styled(
                "  |  q: quit  r: refresh  Tab/1-8: switch  Up/Down: navigate  scroll: mouse",
                theme::LABEL,
            ),
        ])),
        area,
    );
}

fn render_stats_bar(f: &mut Frame, app: &App, area: Rect) {
    let stats = &app.solutions_state.stats;
    let mut spans = vec![
        Span::styled("  Total: ", theme::LABEL),
        Span::styled(
            format!("{}", stats.total),
            theme::DATA.add_modifier(Modifier::BOLD),
        ),
    ];

    for (conf, count) in &stats.by_confidence {
        let color = confidence_color(conf);
        spans.push(Span::styled("  ", theme::LABEL));
        spans.push(Span::styled(
            capitalize(conf),
            Style::default().fg(color),
        ));
        spans.push(Span::styled(": ", theme::LABEL));
        spans.push(Span::styled(
            format!("{}", count),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }

    let block = theme::styled_block_accent("Solutions", Color::Green);
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn render_solution_list(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            format!(" Solution List ({}) ", app.solutions_state.solutions.len()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.solutions_state.solutions.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "  No solutions found yet",
                theme::LABEL,
            )])),
            inner,
        );
        return;
    }

    let available_width = inner.width as usize;
    let prefix_width = 30; // [hig] !critical issue_title -> summary
    let max_content_chars = available_width.saturating_sub(prefix_width);

    let items: Vec<ListItem> = app
        .solutions_state
        .solutions
        .iter()
        .map(|sol| {
            let conf_style = confidence_style(&sol.confidence);
            let conf_abbrev = if sol.confidence.len() >= 3 {
                &sol.confidence[..3]
            } else {
                &sol.confidence
            };

            let sev_icon = severity_icon(&sol.issue_severity);
            let sev_label = if sol.issue_severity.len() > 8 {
                format!("{:<8}", &sol.issue_severity[..8])
            } else {
                format!("{:<8}", sol.issue_severity)
            };

            let issue_short: String = sol.issue_title.chars().take(20).collect();
            let arrow = " -> ";
            let remaining = max_content_chars.saturating_sub(issue_short.len() + arrow.len());
            let summary_display = if sol.summary.chars().count() > remaining && remaining > 3 {
                format!(
                    "{}...",
                    sol.summary.chars().take(remaining - 3).collect::<String>()
                )
            } else {
                sol.summary.chars().take(remaining).collect()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("[{}] ", conf_abbrev), conf_style),
                sev_icon,
                Span::styled(sev_label, severity_style(&sol.issue_severity)),
                Span::styled(issue_short, theme::MAGENTA),
                Span::styled(arrow, theme::LABEL),
                Span::styled(summary_display, theme::DATA),
            ]))
        })
        .collect();

    f.render_stateful_widget(
        List::new(items).highlight_style(theme::HIGHLIGHT),
        inner,
        &mut app.solutions_state.list_state,
    );

    if app.solutions_state.solutions.len() > inner.height as usize {
        let mut ss = ScrollbarState::new(app.solutions_state.solutions.len())
            .position(app.solutions_state.selected_index);
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
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Solution Detail ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let sol = match app.solutions_state.selected_solution() {
        Some(s) => s,
        None => {
            f.render_widget(
                Paragraph::new("  Select a solution to view details").style(theme::LABEL),
                inner,
            );
            return;
        }
    };

    let conf_color = confidence_color(&sol.confidence);
    let sev_color = severity_color(&sol.issue_severity);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("  Solution #{} ", sol.id), theme::LABEL),
            Span::styled("for Issue #", theme::LABEL),
            Span::styled(format!("{}: ", sol.issue_id), theme::DATA),
            Span::styled(
                sol.issue_title.as_str(),
                theme::DATA.add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Confidence: ", theme::LABEL),
            Span::styled(
                sol.confidence.as_str(),
                Style::default().fg(conf_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  |  Issue Severity: ", theme::LABEL),
            Span::styled(
                sol.issue_severity.as_str(),
                Style::default().fg(sev_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  |  Status: ", theme::LABEL),
            Span::styled(sol.issue_status.as_str(), status_style(&sol.issue_status)),
        ]),
    ];

    // Source info
    if !sol.source_title.is_empty() || !sol.source_url.is_empty() {
        let source_display = if !sol.source_title.is_empty() {
            sol.source_title.as_str()
        } else {
            sol.source_url.as_str()
        };
        lines.push(Line::from(vec![
            Span::styled("  Source: ", theme::LABEL),
            Span::styled(source_display, Style::default().fg(Color::Blue)),
        ]));
        if !sol.source_title.is_empty() && !sol.source_url.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("  URL: ", theme::LABEL),
                Span::styled(sol.source_url.as_str(), Style::default().fg(Color::Blue)),
            ]));
        }
    }

    lines.push(Line::from(vec![
        Span::styled("  Created: ", theme::LABEL),
        Span::styled(sol.created_at.as_str(), theme::DATA),
    ]));
    lines.push(Line::from(vec![]));

    // Word-wrap the summary
    let wrap_width = inner.width.saturating_sub(4) as usize;
    if !sol.summary.is_empty() && wrap_width > 0 {
        for chunk in wrap_text(&sol.summary, wrap_width) {
            lines.push(Line::from(vec![
                Span::styled("  ", theme::LABEL),
                Span::styled(chunk, theme::DATA),
            ]));
        }
    }

    let scroll = app.solutions_state.detail_scroll;
    f.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
}

// ──────────────── Helpers ────────────────

fn confidence_color(confidence: &str) -> Color {
    match confidence {
        "high" => Color::Green,
        "medium" => Color::Yellow,
        "low" => Color::Red,
        _ => Color::DarkGray,
    }
}

fn confidence_style(confidence: &str) -> Style {
    match confidence {
        "high" => theme::SUCCESS,
        "medium" => Style::default().fg(Color::Yellow),
        "low" => Style::default().fg(Color::Red),
        _ => theme::LABEL,
    }
}

fn severity_icon(severity: &str) -> Span<'static> {
    match severity {
        "critical" => Span::styled(
            "!",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        ),
        "high" => Span::styled(
            "!",
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        ),
        "medium" => Span::styled("*", Style::default().fg(Color::Yellow)),
        _ => Span::styled(".", theme::SUCCESS),
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

fn severity_style(severity: &str) -> Style {
    match severity {
        "critical" => Style::default().fg(Color::Red),
        "high" => Style::default().fg(Color::LightRed),
        "medium" => Style::default().fg(Color::Yellow),
        "low" => theme::SUCCESS,
        _ => theme::LABEL,
    }
}

fn status_style(status: &str) -> Style {
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
