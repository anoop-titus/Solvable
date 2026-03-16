use ratatui::layout::Rect;
use crate::app::Tab;

/// Tracks the rendered Rect of each tab label for mouse click detection.
pub struct TabBarState {
    pub tab_rects: [Rect; 8],
}

impl Default for TabBarState {
    fn default() -> Self {
        Self { tab_rects: [Rect::default(); 8] }
    }
}

impl TabBarState {
    /// Returns which tab was clicked, if any.
    pub fn hit_test(&self, col: u16, row: u16) -> Option<Tab> {
        for (i, rect) in self.tab_rects.iter().enumerate() {
            if rect.width > 0
                && col >= rect.x && col < rect.x + rect.width
                && row >= rect.y && row < rect.y + rect.height
            {
                return Tab::from_index(i);
            }
        }
        None
    }
}
