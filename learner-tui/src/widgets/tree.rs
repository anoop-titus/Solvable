use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, ListState, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::theme;

/// A single node in the tree hierarchy.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub label: String,
    pub expanded: bool,
    pub children: Vec<TreeNode>,
    pub data_id: Option<u64>,
    pub depth: usize,
}

impl TreeNode {
    /// Create a folder node (no data_id, has children).
    pub fn folder(label: &str, depth: usize) -> Self {
        Self {
            label: label.to_string(),
            expanded: true,
            children: Vec::new(),
            data_id: None,
            depth,
        }
    }

    /// Create a leaf node (has data_id, no children).
    pub fn leaf(label: &str, depth: usize, data_id: u64) -> Self {
        Self {
            label: label.to_string(),
            expanded: false,
            children: Vec::new(),
            data_id: Some(data_id),
            depth,
        }
    }

    pub fn is_folder(&self) -> bool {
        !self.children.is_empty()
    }
}

/// A flattened representation of a visible tree node for rendering.
#[derive(Debug, Clone)]
pub struct FlatNode {
    pub label: String,
    pub depth: usize,
    pub is_folder: bool,
    pub expanded: bool,
    pub data_id: Option<u64>,
    pub node_path: Vec<usize>, // path of indices to reach this node in the tree
    pub is_last_child: bool,   // whether this is the last child at its level
}

/// The full tree widget state.
pub struct TreeState {
    pub roots: Vec<TreeNode>,
    pub flat_nodes: Vec<FlatNode>,
    pub selected: usize,
    pub list_state: ListState,
    pub scroll_offset: usize,
}

impl Default for TreeState {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            flat_nodes: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            scroll_offset: 0,
        }
    }
}

impl TreeState {
    /// Build a tree from issues grouped by cluster_name.
    pub fn from_issues(issues: &[crate::io_layer::db::IssueDetail]) -> Self {
        use std::collections::BTreeMap;

        let mut groups: BTreeMap<String, Vec<(u64, String)>> = BTreeMap::new();
        for issue in issues {
            let group = issue
                .cluster_name
                .as_deref()
                .unwrap_or("Uncategorized")
                .to_string();
            groups
                .entry(group)
                .or_default()
                .push((issue.id, issue.title.clone()));
        }

        let mut roots = Vec::new();
        for (group_name, items) in &groups {
            let mut folder = TreeNode::folder(group_name, 0);
            for (id, title) in items {
                folder.children.push(TreeNode::leaf(title, 1, *id));
            }
            roots.push(folder);
        }

        let mut state = Self {
            roots,
            flat_nodes: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            scroll_offset: 0,
        };
        state.flatten();
        if !state.flat_nodes.is_empty() {
            state.list_state.select(Some(0));
        }
        state
    }

    /// Build a tree from solutions grouped by confidence level.
    pub fn from_solutions(solutions: &[crate::io_layer::db::SolutionDetail]) -> Self {
        use std::collections::BTreeMap;

        let mut groups: BTreeMap<String, Vec<(u64, String)>> = BTreeMap::new();
        for sol in solutions {
            let group = if sol.confidence.is_empty() {
                "Unknown".to_string()
            } else {
                capitalize(&sol.confidence)
            };
            groups
                .entry(group)
                .or_default()
                .push((sol.id, format!("{} -> {}", sol.issue_title, sol.summary)));
        }

        let mut roots = Vec::new();
        for (group_name, items) in &groups {
            let mut folder = TreeNode::folder(group_name, 0);
            for (id, title) in items {
                folder.children.push(TreeNode::leaf(title, 1, *id));
            }
            roots.push(folder);
        }

        let mut state = Self {
            roots,
            flat_nodes: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            scroll_offset: 0,
        };
        state.flatten();
        if !state.flat_nodes.is_empty() {
            state.list_state.select(Some(0));
        }
        state
    }

    /// Build a tree from solve items grouped by cluster-name prefix (first word).
    pub fn from_solve_items(items: &[crate::app::SolveItem], label: &str) -> Self {
        use std::collections::BTreeMap;

        let mut groups: BTreeMap<String, Vec<(u64, String)>> = BTreeMap::new();
        for item in items {
            // Group by first significant word of the cluster name
            let group = item
                .name
                .split_whitespace()
                .next()
                .unwrap_or("Other")
                .to_string();
            groups
                .entry(group)
                .or_default()
                .push((item.id, item.name.clone()));
        }

        // If there's only one group or very few items, skip grouping
        let roots = if groups.len() <= 1 || items.len() <= 5 {
            let mut folder = TreeNode::folder(label, 0);
            for item in items {
                folder.children.push(TreeNode::leaf(&item.name, 1, item.id));
            }
            vec![folder]
        } else {
            let mut roots = Vec::new();
            for (group_name, items) in &groups {
                let mut folder = TreeNode::folder(group_name, 0);
                for (id, title) in items {
                    folder.children.push(TreeNode::leaf(title, 1, *id));
                }
                roots.push(folder);
            }
            roots
        };

        let mut state = Self {
            roots,
            flat_nodes: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            scroll_offset: 0,
        };
        state.flatten();
        if !state.flat_nodes.is_empty() {
            state.list_state.select(Some(0));
        }
        state
    }

    /// Rebuild the flat_nodes list from the current tree state (visible nodes only).
    pub fn flatten(&mut self) {
        self.flat_nodes.clear();
        let mut result = Vec::new();
        let root_count = self.roots.len();
        for (i, root) in self.roots.iter().enumerate() {
            let is_last = i == root_count - 1;
            flatten_node_recursive(root, &[i], is_last, &mut result);
        }
        self.flat_nodes = result;
    }

    /// Toggle expand/collapse on the currently selected node (if it's a folder).
    pub fn toggle_selected(&mut self) {
        if let Some(flat) = self.flat_nodes.get(self.selected) {
            if flat.is_folder {
                let path = flat.node_path.clone();
                if let Some(node) = self.node_at_path_mut(&path) {
                    node.expanded = !node.expanded;
                }
                self.flatten();
                // Clamp selection
                if self.selected >= self.flat_nodes.len() {
                    self.selected = self.flat_nodes.len().saturating_sub(1);
                }
                self.list_state.select(if self.flat_nodes.is_empty() {
                    None
                } else {
                    Some(self.selected)
                });
            }
        }
    }

    /// Get the data_id of the currently selected node (if it's a leaf).
    pub fn selected_data_id(&self) -> Option<u64> {
        self.flat_nodes
            .get(self.selected)
            .and_then(|n| n.data_id)
    }

    /// Navigate up in the flattened list.
    pub fn select_prev(&mut self) {
        if self.flat_nodes.is_empty() {
            return;
        }
        self.selected = self.selected.saturating_sub(1);
        self.list_state.select(Some(self.selected));
    }

    /// Navigate down in the flattened list.
    pub fn select_next(&mut self) {
        if self.flat_nodes.is_empty() {
            return;
        }
        let max = self.flat_nodes.len().saturating_sub(1);
        if self.selected < max {
            self.selected += 1;
        }
        self.list_state.select(Some(self.selected));
    }

    /// Get mutable reference to a tree node by its path.
    fn node_at_path_mut(&mut self, path: &[usize]) -> Option<&mut TreeNode> {
        if path.is_empty() {
            return None;
        }
        let mut current = self.roots.get_mut(path[0])?;
        for &idx in &path[1..] {
            current = current.children.get_mut(idx)?;
        }
        Some(current)
    }

    /// Check if tree has any nodes.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
}

/// Recursively flatten a tree node into a list of FlatNodes.
fn flatten_node_recursive(
    node: &TreeNode,
    path: &[usize],
    is_last: bool,
    result: &mut Vec<FlatNode>,
) {
    result.push(FlatNode {
        label: node.label.clone(),
        depth: node.depth,
        is_folder: node.is_folder(),
        expanded: node.expanded,
        data_id: node.data_id,
        node_path: path.to_vec(),
        is_last_child: is_last,
    });

    if node.expanded {
        let child_count = node.children.len();
        for (i, child) in node.children.iter().enumerate() {
            let is_last_child = i == child_count - 1;
            let mut child_path = path.to_vec();
            child_path.push(i);
            flatten_node_recursive(child, &child_path, is_last_child, result);
        }
    }
}

/// Render the tree view inside a given area.
pub fn render_tree(
    f: &mut Frame,
    state: &mut TreeState,
    area: Rect,
    focused: bool,
) {
    if state.flat_nodes.is_empty() {
        f.render_widget(
            ratatui::widgets::Paragraph::new("  (empty)")
                .style(theme::LABEL),
            area,
        );
        return;
    }

    let available_width = area.width as usize;

    let items: Vec<ListItem> = state
        .flat_nodes
        .iter()
        .map(|node| {
            let indent = "  ".repeat(node.depth);
            let (icon, icon_style) = if node.is_folder {
                if node.expanded {
                    ("\u{25bc} ", Style::default().fg(Color::Yellow)) // down-pointing triangle
                } else {
                    ("\u{25b6} ", Style::default().fg(Color::Yellow)) // right-pointing triangle
                }
            } else {
                // Leaf connector
                let connector = if node.is_last_child {
                    "\u{2514}\u{2500} " // corner
                } else {
                    "\u{251c}\u{2500} " // tee
                };
                (connector, Style::default().fg(Color::DarkGray))
            };

            let label_style = if node.is_folder {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme::DATA
            };

            // Truncate label to fit
            let prefix_len = indent.len() + icon.chars().count();
            let max_label = available_width.saturating_sub(prefix_len);
            let display_label = if node.label.chars().count() > max_label && max_label > 3 {
                format!(
                    "{}...",
                    node.label.chars().take(max_label - 3).collect::<String>()
                )
            } else {
                node.label.clone()
            };

            // Add child count for folders
            let count_span = if node.is_folder {
                // Count children from the tree (not flat nodes)
                let child_count = format!(" ({})", count_children_label(&node.label, &state.roots));
                Span::styled(child_count, Style::default().fg(Color::DarkGray))
            } else {
                Span::raw("")
            };

            ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled(icon, icon_style),
                Span::styled(display_label, label_style),
                count_span,
            ]))
        })
        .collect();

    let highlight = if focused {
        theme::HIGHLIGHT
    } else {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    };

    f.render_stateful_widget(
        List::new(items).highlight_style(highlight),
        area,
        &mut state.list_state,
    );

    // Scrollbar
    if state.flat_nodes.len() > area.height as usize {
        let mut ss = ScrollbarState::new(state.flat_nodes.len()).position(state.selected);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            area,
            &mut ss,
        );
    }
}

/// Count children of a folder by label (helper for rendering).
fn count_children_label(label: &str, roots: &[TreeNode]) -> usize {
    for root in roots {
        if root.label == label {
            return root.children.len();
        }
        // Check nested
        if let Some(count) = count_in_children(label, &root.children) {
            return count;
        }
    }
    0
}

fn count_in_children(label: &str, children: &[TreeNode]) -> Option<usize> {
    for child in children {
        if child.label == label {
            return Some(child.children.len());
        }
        if let Some(c) = count_in_children(label, &child.children) {
            return Some(c);
        }
    }
    None
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + c.as_str(),
    }
}
