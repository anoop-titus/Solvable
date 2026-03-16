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

use crate::app::{App, SolveFocus, SolveProgress};
use crate::theme;
use crate::widgets::search::{render_search_bar, render_search_results};

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    if !app.solve_state.loaded {
        f.render_widget(
            Paragraph::new("  Mesh DB not found or Lvl2 analyses not loaded. Switch to this tab to load.")
                .style(Style::default().fg(Color::Yellow))
                .block(theme::styled_block_accent("Solve", Color::Magenta)),
            area,
        );
        return;
    }

    // Layout: stats bar (3) | search bar (3, conditional) | columns area (min) | solved box (6)
    if app.solve_state.search.active {
        let vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),   // stats bar
                Constraint::Length(3),   // search bar
                Constraint::Min(10),     // two-column area
                Constraint::Length(8),   // solved box
            ])
            .split(area);

        render_stats_bar(f, app, vert[0]);
        render_search_bar(f, &app.solve_state.search, vert[1]);
        render_columns(f, app, vert[2]);
        render_solved_box(f, app, vert[3]);

        // Search results overlay on top of columns
        if !app.solve_state.search.results.is_empty() {
            let overlay_area = Rect::new(
                vert[2].x,
                vert[2].y,
                vert[2].width,
                vert[2].height.min(12),
            );
            render_search_results(f, &mut app.solve_state.search, overlay_area, 10);
        }
    } else {
        let vert = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),   // stats bar
                Constraint::Min(10),     // two-column area
                Constraint::Length(8),   // solved box
            ])
            .split(area);

        render_stats_bar(f, app, vert[0]);
        render_columns(f, app, vert[1]);
        render_solved_box(f, app, vert[2]);
    }
}

pub fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let ai_checked = app.solve_state.checked_ai_count();
    let progress_text = match app.solve_state.progress {
        SolveProgress::Idle => String::new(),
        SolveProgress::Solving => {
            let done = app.solve_state.solve_current;
            let total = app.solve_state.solve_queue.len();
            format!("  Solving {}/{}...", done + 1, total)
        }
        SolveProgress::Done => "  Complete!".to_string(),
    };

    let focus_label = match app.solve_state.focus {
        SolveFocus::AiList => "AI List",
        SolveFocus::HumanList => "Human List",
        SolveFocus::Solved => "Solved",
        SolveFocus::AiActions => "Actions",
    };

    let search_hint = if app.solve_state.search.active {
        "  Esc: close search  Enter: jump"
    } else {
        "  /: search"
    };

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  Focus: ", theme::LABEL),
            Span::styled(focus_label, Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("  |  AI: {} items ({} checked)", app.solve_state.ai_items.len(), ai_checked),
                theme::LABEL,
            ),
            Span::styled(
                format!("  |  Human: {} items", app.solve_state.human_items.len()),
                theme::LABEL,
            ),
            Span::styled(
                &progress_text,
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  |  Space: toggle  Enter: action  Tab: cycle  L/R: columns",
                theme::LABEL,
            ),
            Span::styled(search_hint, Style::default().fg(Color::Yellow)),
        ])),
        area,
    );
}

fn render_stats_bar(f: &mut Frame, app: &App, area: Rect) {
    let block = theme::styled_block_accent("Solve", Color::Magenta);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let total = app.solve_state.total_count;
    let ai = app.solve_state.ai_items.len();
    let human = app.solve_state.human_items.len();
    let solved = app.solve_state.solved_items.len();

    let spans = vec![
        Span::styled("  Lvl2 Analyses: ", theme::LABEL),
        Span::styled(
            format!("{}", total),
            theme::DATA.add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  ", theme::LABEL),
        Span::styled("AI Solvable: ", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("{}", ai),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  ", theme::LABEL),
        Span::styled("Human: ", Style::default().fg(Color::Yellow)),
        Span::styled(
            format!("{}", human),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  |  ", theme::LABEL),
        Span::styled("Solved: ", Style::default().fg(Color::Green)),
        Span::styled(
            format!("{}", solved),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
    ];

    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn render_columns(f: &mut Frame, app: &mut App, area: Rect) {
    // Two columns: AI (left) | Human (right)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_ai_column(f, app, cols[0]);
    render_human_column(f, app, cols[1]);
}

fn render_ai_column(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = matches!(app.solve_state.focus, SolveFocus::AiList | SolveFocus::AiActions);
    let border_color = if is_focused { Color::Cyan } else { Color::DarkGray };
    let checked_count = app.solve_state.checked_ai_count();

    // Vertical split: list + action buttons row
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(3)])
        .split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" AI Solvable ({}) ", app.solve_state.ai_items.len()),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(vert[0]);
    f.render_widget(block, vert[0]);

    if app.solve_state.ai_items.is_empty() {
        f.render_widget(
            Paragraph::new("  No AI-solvable items").style(theme::LABEL),
            inner,
        );
    } else {
        let items: Vec<ListItem> = app.solve_state.ai_items.iter().enumerate().map(|(_i, item)| {
            let checkbox = if item.solving {
                let spinner_frames = ['|', '/', '-', '\\'];
                let frame = spinner_frames[(app.tick_count as usize / 2) % 4];
                Span::styled(
                    format!("[{}] ", frame),
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                )
            } else if item.queued {
                Span::styled("[..] ", Style::default().fg(Color::DarkGray))
            } else if item.checked {
                Span::styled("[x] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            } else {
                Span::styled("[ ] ", theme::LABEL)
            };

            let name_style = if item.green_flash_until.is_some() {
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
            } else if item.strikethrough {
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT)
            } else if item.solving {
                Style::default().fg(Color::Yellow)
            } else {
                theme::DATA
            };

            let max_name = (inner.width as usize).saturating_sub(6);
            let display_name: String = item.name.chars().take(max_name).collect();
            let display_name = if item.name.chars().count() > max_name && max_name > 3 {
                format!("{}...", item.name.chars().take(max_name - 3).collect::<String>())
            } else {
                display_name
            };

            ListItem::new(Line::from(vec![
                checkbox,
                Span::styled(display_name, name_style),
            ]))
        }).collect();

        let highlight = if matches!(app.solve_state.focus, SolveFocus::AiList) {
            theme::HIGHLIGHT
        } else {
            Style::default()
        };

        f.render_stateful_widget(
            List::new(items).highlight_style(highlight),
            inner,
            &mut app.solve_state.ai_list_state,
        );

        if app.solve_state.ai_items.len() > inner.height as usize {
            let mut ss = ScrollbarState::new(app.solve_state.ai_items.len())
                .position(app.solve_state.ai_selected);
            f.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight)
                    .begin_symbol(None)
                    .end_symbol(None),
                inner,
                &mut ss,
            );
        }
    }

    // Action buttons row
    let btn_focused = matches!(app.solve_state.focus, SolveFocus::AiActions);
    let btn_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if btn_focused { Color::Cyan } else { Color::DarkGray }));
    let btn_inner = btn_block.inner(vert[1]);
    f.render_widget(btn_block, vert[1]);

    let solving = app.solve_state.progress == SolveProgress::Solving;

    let buttons = vec![
        ("Solve", 0, Color::Green),
        ("Transfer>Human", 1, Color::Yellow),
        ("Dissolve", 2, Color::Red),
    ];

    let mut btn_spans = vec![Span::styled(" ", theme::LABEL)];
    for (label, idx, color) in &buttons {
        let style = if btn_focused && app.solve_state.active_button == *idx {
            Style::default().fg(Color::Black).bg(*color).add_modifier(Modifier::BOLD)
        } else if solving && *idx == 0 {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(*color)
        };

        let display = if solving && *idx == 0 {
            format!(" Solving... ")
        } else if *idx == 0 && checked_count > 0 {
            format!(" [{}] ({}) ", label, checked_count)
        } else {
            format!(" [{}] ", label)
        };

        btn_spans.push(Span::styled(display, style));
        btn_spans.push(Span::styled("  ", theme::LABEL));
    }

    f.render_widget(Paragraph::new(Line::from(btn_spans)), btn_inner);
}

fn render_human_column(f: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.solve_state.focus == SolveFocus::HumanList;
    let border_color = if is_focused { Color::Yellow } else { Color::DarkGray };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" Human Solvable ({}) ", app.solve_state.human_items.len()),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.solve_state.human_items.is_empty() {
        f.render_widget(
            Paragraph::new("  No human-solvable items").style(theme::LABEL),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = app.solve_state.human_items.iter().map(|item| {
        let (checkbox, name_style) = if item.strikethrough {
            (
                Span::styled("[S] ", Style::default().fg(Color::Green)),
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT),
            )
        } else {
            (
                Span::styled("[ ] ", theme::LABEL),
                theme::DATA,
            )
        };

        let max_name = (inner.width as usize).saturating_sub(6);
        let display_name = if item.name.chars().count() > max_name && max_name > 3 {
            format!("{}...", item.name.chars().take(max_name - 3).collect::<String>())
        } else {
            item.name.chars().take(max_name).collect()
        };

        ListItem::new(Line::from(vec![
            checkbox,
            Span::styled(display_name, name_style),
        ]))
    }).collect();

    let highlight = if is_focused { theme::HIGHLIGHT } else { Style::default() };

    f.render_stateful_widget(
        List::new(items).highlight_style(highlight),
        inner,
        &mut app.solve_state.human_list_state,
    );

    if app.solve_state.human_items.len() > inner.height as usize {
        let mut ss = ScrollbarState::new(app.solve_state.human_items.len())
            .position(app.solve_state.human_selected);
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
    let is_focused = app.solve_state.focus == SolveFocus::Solved;
    let border_color = if is_focused { Color::Green } else { Color::DarkGray };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(
            format!(" Solved ({}) ", app.solve_state.solved_items.len()),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.solve_state.solved_items.is_empty() {
        f.render_widget(
            Paragraph::new("  No items solved yet. Use [Solve] on AI items or mark Human items as done.")
                .style(theme::LABEL),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = app.solve_state.solved_items.iter().map(|item| {
        let method_style = if item.method == "AI" {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Yellow)
        };

        let max_name = (inner.width as usize).saturating_sub(30);
        let display_name = if item.name.chars().count() > max_name && max_name > 3 {
            format!("{}...", item.name.chars().take(max_name - 3).collect::<String>())
        } else {
            item.name.chars().take(max_name).collect()
        };

        ListItem::new(Line::from(vec![
            Span::styled("  V ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(display_name, theme::DATA),
            Span::styled(" -- ", theme::LABEL),
            Span::styled(&item.method, method_style),
            Span::styled(
                format!(" ({})", item.solved_at),
                theme::LABEL,
            ),
        ]))
    }).collect();

    let highlight = if is_focused { theme::HIGHLIGHT } else { Style::default() };

    f.render_stateful_widget(
        List::new(items).highlight_style(highlight),
        inner,
        &mut app.solve_state.solved_list_state,
    );
}
