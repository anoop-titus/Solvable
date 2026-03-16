use ratatui::{
    layout::{Constraint, Layout, Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};
use crate::theme;

pub fn render(f: &mut Frame, area: Rect) {
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),  // title
        Constraint::Length(1),  // blank
        Constraint::Length(1),  // tagline
        Constraint::Length(1),  // blank
        Constraint::Length(3),  // description
        Constraint::Length(1),  // blank
        Constraint::Length(1),  // button
        Constraint::Fill(1),
    ]).split(area);

    // Title
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Solvable", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ])).alignment(Alignment::Center),
        vertical[1],
    );

    // Tagline
    f.render_widget(
        Paragraph::new("Access to all your Solutions")
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center),
        vertical[3],
    );

    // Description
    f.render_widget(
        Paragraph::new("Ingests documents, emails, and files \u{2014} discovers issues,\nresearches solutions, and builds a knowledge mesh\nso nothing falls through the cracks.")
            .style(theme::LABEL)
            .alignment(Alignment::Center),
        vertical[5],
    );

    // Button
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("[ ", theme::LABEL),
            Span::styled("Get Started", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(" ]", theme::LABEL),
        ])).alignment(Alignment::Center),
        vertical[7],
    );
}

/// Simple hit test for the Get Started button area
pub fn hit_test_button(col: u16, row: u16, area: Rect) -> bool {
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ]).split(area);

    let button_rect = vertical[7];
    col >= button_rect.x && col < button_rect.x + button_rect.width
        && row >= button_rect.y && row < button_rect.y + button_rect.height
}
