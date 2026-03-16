use ratatui::{layout::Rect, widgets::Paragraph, Frame};
use crate::theme;

pub fn render(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new("  Settings — Coming in Sub-project 2")
            .style(theme::LABEL)
            .block(theme::styled_block("\u{2699} Settings")),
        area,
    );
}
