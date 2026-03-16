use ratatui::{layout::Rect, widgets::Paragraph, Frame};
use crate::theme;

pub fn render(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new("  Confluence — Coming in Sub-project 4")
            .style(theme::LABEL)
            .block(theme::styled_block("Confluence")),
        area,
    );
}
