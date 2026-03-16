use ratatui::{layout::Rect, widgets::Paragraph, Frame};
use crate::theme;

pub fn render(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new("  Solutions — Coming in Sub-project 3")
            .style(theme::LABEL)
            .block(theme::styled_block("Solutions")),
        area,
    );
}
