use ratatui::{layout::Rect, widgets::Paragraph, Frame};
use crate::theme;

pub fn render(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new("  Solve — Coming in Sub-project 5")
            .style(theme::LABEL)
            .block(theme::styled_block("Solve")),
        area,
    );
}
