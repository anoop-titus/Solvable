use chrono::Local;
use ratatui::widgets::ListState;

use crate::io_layer::db::{
    self, ConfluenceData, ConfluenceRecord, IssueDetail, IssueStats,
    RecentLearning, ResearchIssue, ResearchSolution, ResearchStats, RunProgress,
    SolvableBy, SolutionDetail, SolutionStats,
};
use crate::io_layer::env_store;
use crate::screens::portal::PortalState;
use crate::screens::settings::SettingsState;
use crate::widgets::search::SearchState;
use crate::widgets::tree::TreeState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Main,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Tab {
    Learnings  = 0,
    Research   = 1,
    Issues     = 2,
    Solutions  = 3,
    Confluence = 4,
    Solve      = 5,
    Portal     = 6,
    Settings   = 7,
}

impl Tab {
    pub const ALL: [Tab; 8] = [
        Tab::Learnings, Tab::Research, Tab::Issues, Tab::Solutions,
        Tab::Confluence, Tab::Solve, Tab::Portal, Tab::Settings,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Tab::Learnings  => "Learnings",
            Tab::Research   => "Research",
            Tab::Issues     => "Issues",
            Tab::Solutions  => "Solutions",
            Tab::Confluence => "Confluence",
            Tab::Solve      => "Solve",
            Tab::Portal     => "Portal",
            Tab::Settings   => "\u{2699} Settings",
        }
    }

    pub fn index(&self) -> usize { *self as usize }

    pub fn from_index(i: usize) -> Option<Tab> {
        Tab::ALL.get(i).copied()
    }
}

// ──────────────── Issues Tab State ────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueFocus {
    Filters,
    List,
    Detail,
}

pub struct DropdownFilter {
    pub label: String,
    pub options: Vec<String>,
    pub selected_index: usize,
    pub expanded: bool,
}

impl DropdownFilter {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            options: vec!["All".to_string()],
            selected_index: 0,
            expanded: false,
        }
    }

    pub fn selected_value(&self) -> &str {
        self.options.get(self.selected_index).map(|s| s.as_str()).unwrap_or("All")
    }

    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    pub fn select_next(&mut self) {
        if self.selected_index + 1 < self.options.len() {
            self.selected_index += 1;
        }
    }

    pub fn select_prev(&mut self) {
        self.selected_index = self.selected_index.saturating_sub(1);
    }

    pub fn set_options(&mut self, values: &[(String, u64)]) {
        self.options = vec!["All".to_string()];
        for (val, _count) in values {
            self.options.push(val.clone());
        }
        // Reset selection if out of bounds
        if self.selected_index >= self.options.len() {
            self.selected_index = 0;
        }
    }
}

pub struct IssuesState {
    pub issues: Vec<IssueDetail>,
    pub stats: IssueStats,
    pub filtered_indices: Vec<usize>,
    pub selected_index: usize,
    pub severity_filter: DropdownFilter,
    pub status_filter: DropdownFilter,
    pub category_filter: DropdownFilter,
    pub list_state: ListState,
    pub detail_scroll: u16,
    pub focus: IssueFocus,
    pub active_filter: usize, // 0=severity, 1=status, 2=category
    pub loaded: bool,
    pub tree: TreeState,
    pub search: SearchState,
}

impl Default for IssuesState {
    fn default() -> Self {
        Self {
            issues: Vec::new(),
            stats: IssueStats::default(),
            filtered_indices: Vec::new(),
            selected_index: 0,
            severity_filter: DropdownFilter::new("Severity"),
            status_filter: DropdownFilter::new("Status"),
            category_filter: DropdownFilter::new("Category"),
            list_state: ListState::default(),
            detail_scroll: 0,
            focus: IssueFocus::List,
            active_filter: 0,
            loaded: false,
            tree: TreeState::default(),
            search: SearchState::default(),
        }
    }
}

impl IssuesState {
    pub fn apply_filters(&mut self) {
        let sev = self.severity_filter.selected_value().to_string();
        let sta = self.status_filter.selected_value().to_string();
        let cat = self.category_filter.selected_value().to_string();

        self.filtered_indices = self.issues.iter().enumerate()
            .filter(|(_, issue)| {
                (sev == "All" || issue.severity == sev)
                    && (sta == "All" || issue.status == sta)
                    && (cat == "All" || issue.category == cat)
            })
            .map(|(i, _)| i)
            .collect();

        // Reset selection
        if self.filtered_indices.is_empty() {
            self.selected_index = 0;
            self.list_state.select(None);
        } else {
            self.selected_index = 0;
            self.list_state.select(Some(0));
        }
        self.detail_scroll = 0;
    }

    pub fn scroll_list(&mut self, delta: i32) {
        if self.filtered_indices.is_empty() {
            return;
        }
        let max = self.filtered_indices.len().saturating_sub(1);
        let current = self.selected_index;
        let new_pos = if delta > 0 {
            (current + delta as usize).min(max)
        } else {
            current.saturating_sub((-delta) as usize)
        };
        self.selected_index = new_pos;
        self.list_state.select(Some(new_pos));
        self.detail_scroll = 0;
    }

    pub fn selected_issue(&self) -> Option<&IssueDetail> {
        self.filtered_indices
            .get(self.selected_index)
            .and_then(|&idx| self.issues.get(idx))
    }

    pub fn active_dropdown_mut(&mut self) -> &mut DropdownFilter {
        match self.active_filter {
            0 => &mut self.severity_filter,
            1 => &mut self.status_filter,
            _ => &mut self.category_filter,
        }
    }

    pub fn any_dropdown_expanded(&self) -> bool {
        self.severity_filter.expanded || self.status_filter.expanded || self.category_filter.expanded
    }

    pub fn collapse_all_dropdowns(&mut self) {
        self.severity_filter.expanded = false;
        self.status_filter.expanded = false;
        self.category_filter.expanded = false;
    }
}

// ──────────────── Solutions Tab State ────────────────

pub struct SolutionsState {
    pub solutions: Vec<SolutionDetail>,
    pub stats: SolutionStats,
    pub selected_index: usize,
    pub list_state: ListState,
    pub detail_scroll: u16,
    pub loaded: bool,
    pub tree: TreeState,
    pub search: SearchState,
}

impl Default for SolutionsState {
    fn default() -> Self {
        Self {
            solutions: Vec::new(),
            stats: SolutionStats::default(),
            selected_index: 0,
            list_state: ListState::default(),
            detail_scroll: 0,
            loaded: false,
            tree: TreeState::default(),
            search: SearchState::default(),
        }
    }
}

impl SolutionsState {
    pub fn scroll_list(&mut self, delta: i32) {
        if self.solutions.is_empty() {
            return;
        }
        let max = self.solutions.len().saturating_sub(1);
        let current = self.selected_index;
        let new_pos = if delta > 0 {
            (current + delta as usize).min(max)
        } else {
            current.saturating_sub((-delta) as usize)
        };
        self.selected_index = new_pos;
        self.list_state.select(Some(new_pos));
        self.detail_scroll = 0;
    }

    pub fn selected_solution(&self) -> Option<&SolutionDetail> {
        self.solutions.get(self.selected_index)
    }
}

// ──────────────── Solve Tab State ────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveFocus {
    AiList,
    HumanList,
    Solved,
    AiActions,   // Focus on action buttons below AI list
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveProgress {
    Idle,
    Solving,
    Done,
}

pub struct SolveItem {
    pub id: u64,
    pub name: String,
    pub summary: String,
    pub checked: bool,
    pub strikethrough: bool,
    pub strikethrough_tick: Option<u64>,
    pub green_flash_until: Option<u64>,
    pub solving: bool,      // currently being solved
    pub queued: bool,       // in solve queue
}

pub struct SolvedItem {
    pub name: String,
    pub method: String,     // "AI" or "Human"
    pub solved_at: String,
}

pub struct SolveState {
    pub ai_items: Vec<SolveItem>,
    pub human_items: Vec<SolveItem>,
    pub solved_items: Vec<SolvedItem>,
    pub focus: SolveFocus,
    pub ai_list_state: ListState,
    pub human_list_state: ListState,
    pub solved_list_state: ListState,
    pub ai_selected: usize,
    pub human_selected: usize,
    pub solved_selected: usize,
    pub progress: SolveProgress,
    pub solve_queue: Vec<usize>,      // indices into ai_items
    pub solve_current: usize,         // current index in solve_queue
    pub solve_tick: u64,              // tick when current solve started
    pub active_button: usize,         // 0=Solve, 1=Transfer, 2=Dissolve
    pub loaded: bool,
    pub ai_count: u64,
    pub human_count: u64,
    pub total_count: u64,
    pub search: SearchState,
    pub ai_tree: TreeState,
    pub human_tree: TreeState,
}

impl Default for SolveState {
    fn default() -> Self {
        Self {
            ai_items: Vec::new(),
            human_items: Vec::new(),
            solved_items: Vec::new(),
            focus: SolveFocus::AiList,
            ai_list_state: ListState::default(),
            human_list_state: ListState::default(),
            solved_list_state: ListState::default(),
            ai_selected: 0,
            human_selected: 0,
            solved_selected: 0,
            progress: SolveProgress::Idle,
            solve_queue: Vec::new(),
            solve_current: 0,
            solve_tick: 0,
            active_button: 0,
            loaded: false,
            ai_count: 0,
            human_count: 0,
            total_count: 0,
            search: SearchState::default(),
            ai_tree: TreeState::default(),
            human_tree: TreeState::default(),
        }
    }
}

impl SolveState {
    pub fn scroll_ai(&mut self, delta: i32) {
        if self.ai_items.is_empty() { return; }
        let max = self.ai_items.len().saturating_sub(1);
        self.ai_selected = if delta > 0 {
            (self.ai_selected + delta as usize).min(max)
        } else {
            self.ai_selected.saturating_sub((-delta) as usize)
        };
        self.ai_list_state.select(Some(self.ai_selected));
    }

    pub fn scroll_human(&mut self, delta: i32) {
        if self.human_items.is_empty() { return; }
        let max = self.human_items.len().saturating_sub(1);
        self.human_selected = if delta > 0 {
            (self.human_selected + delta as usize).min(max)
        } else {
            self.human_selected.saturating_sub((-delta) as usize)
        };
        self.human_list_state.select(Some(self.human_selected));
    }

    pub fn scroll_solved(&mut self, delta: i32) {
        if self.solved_items.is_empty() { return; }
        let max = self.solved_items.len().saturating_sub(1);
        self.solved_selected = if delta > 0 {
            (self.solved_selected + delta as usize).min(max)
        } else {
            self.solved_selected.saturating_sub((-delta) as usize)
        };
        self.solved_list_state.select(Some(self.solved_selected));
    }

    pub fn toggle_ai_check(&mut self) {
        if let Some(item) = self.ai_items.get_mut(self.ai_selected) {
            item.checked = !item.checked;
        }
    }

    pub fn toggle_human_check(&mut self) {
        if let Some(item) = self.human_items.get_mut(self.human_selected) {
            if item.strikethrough {
                // Undo strikethrough
                item.strikethrough = false;
                item.strikethrough_tick = None;
            } else {
                // Start strikethrough with auto-delete timer
                item.strikethrough = true;
                // strikethrough_tick is set by the caller (needs tick_count)
            }
        }
    }

    /// Start AI solving on all checked items.
    pub fn start_solve(&mut self, tick: u64) -> bool {
        if self.progress != SolveProgress::Idle {
            return false;
        }
        self.solve_queue = self.ai_items.iter().enumerate()
            .filter(|(_, item)| item.checked)
            .map(|(i, _)| i)
            .collect();
        if self.solve_queue.is_empty() {
            return false;
        }
        // Mark queued items
        for &idx in &self.solve_queue {
            self.ai_items[idx].queued = true;
        }
        // Start solving first item
        self.solve_current = 0;
        if let Some(&idx) = self.solve_queue.first() {
            self.ai_items[idx].solving = true;
            self.ai_items[idx].queued = false;
        }
        self.solve_tick = tick;
        self.progress = SolveProgress::Solving;
        true
    }

    /// Transfer checked AI items to Human column.
    pub fn transfer_to_human(&mut self) {
        let mut transferred = Vec::new();
        let mut keep_indices = Vec::new();
        for (i, item) in self.ai_items.iter().enumerate() {
            if item.checked {
                transferred.push(SolveItem {
                    id: item.id,
                    name: item.name.clone(),
                    summary: item.summary.clone(),
                    checked: false,
                    strikethrough: false,
                    strikethrough_tick: None,
                    green_flash_until: None,
                    solving: false,
                    queued: false,
                });
            } else {
                keep_indices.push(i);
            }
        }
        let remaining: Vec<SolveItem> = keep_indices.into_iter()
            .map(|i| std::mem::replace(&mut self.ai_items[i], SolveItem {
                id: 0, name: String::new(), summary: String::new(),
                checked: false, strikethrough: false, strikethrough_tick: None,
                green_flash_until: None, solving: false, queued: false,
            }))
            .collect();
        self.ai_items = remaining;
        self.human_items.extend(transferred);
        self.fix_ai_selection();
        self.fix_human_selection();
    }

    /// Dissolve (discard) checked AI items with strikethrough animation.
    pub fn dissolve_checked(&mut self, tick: u64) {
        for item in &mut self.ai_items {
            if item.checked {
                item.strikethrough = true;
                item.strikethrough_tick = Some(tick);
                item.checked = false;
            }
        }
    }

    /// Tick-based animations. Returns true if state changed (needs re-render).
    pub fn tick(&mut self, tick: u64) -> bool {
        let mut changed = false;

        // Process solve queue
        if self.progress == SolveProgress::Solving {
            let elapsed = tick.wrapping_sub(self.solve_tick);
            if elapsed >= 10 {  // 2 seconds per item (10 ticks at 200ms)
                if let Some(&idx) = self.solve_queue.get(self.solve_current) {
                    // Complete current solve
                    self.ai_items[idx].solving = false;
                    self.ai_items[idx].green_flash_until = Some(tick + 25); // 5s flash

                    let now = chrono::Local::now().format("%H:%M").to_string();
                    self.solved_items.push(SolvedItem {
                        name: self.ai_items[idx].name.clone(),
                        method: "AI".to_string(),
                        solved_at: now,
                    });

                    // Advance to next item
                    self.solve_current += 1;
                    if self.solve_current < self.solve_queue.len() {
                        let next_idx = self.solve_queue[self.solve_current];
                        self.ai_items[next_idx].solving = true;
                        self.ai_items[next_idx].queued = false;
                        self.solve_tick = tick;
                    } else {
                        // All done, remove solved items from AI list
                        let solved_ids: Vec<u64> = self.solve_queue.iter()
                            .filter_map(|&i| self.ai_items.get(i).map(|item| item.id))
                            .collect();
                        self.ai_items.retain(|item| !solved_ids.contains(&item.id));
                        self.solve_queue.clear();
                        self.progress = SolveProgress::Done;
                        self.fix_ai_selection();
                        self.fix_solved_selection();
                    }
                    changed = true;
                }
            }
        }

        // Clear Done state after a moment
        if self.progress == SolveProgress::Done {
            self.progress = SolveProgress::Idle;
        }

        // Process green flash expirations
        for item in &mut self.ai_items {
            if let Some(until) = item.green_flash_until {
                if tick >= until {
                    item.green_flash_until = None;
                    changed = true;
                }
            }
        }

        // Process human strikethrough auto-deletes (25 ticks = ~5 seconds)
        let mut moved_to_solved = Vec::new();
        self.human_items.retain(|item| {
            if item.strikethrough {
                if let Some(st) = item.strikethrough_tick {
                    if tick.wrapping_sub(st) >= 25 {
                        moved_to_solved.push(SolvedItem {
                            name: item.name.clone(),
                            method: "Human".to_string(),
                            solved_at: chrono::Local::now().format("%H:%M").to_string(),
                        });
                        return false;
                    }
                }
            }
            true
        });
        if !moved_to_solved.is_empty() {
            self.solved_items.extend(moved_to_solved);
            self.fix_human_selection();
            self.fix_solved_selection();
            changed = true;
        }

        // Process AI dissolve strikethrough auto-deletes
        self.ai_items.retain(|item| {
            if item.strikethrough {
                if let Some(st) = item.strikethrough_tick {
                    if tick.wrapping_sub(st) >= 25 {
                        return false;
                    }
                }
            }
            true
        });

        changed
    }

    fn fix_ai_selection(&mut self) {
        if self.ai_items.is_empty() {
            self.ai_selected = 0;
            self.ai_list_state.select(None);
        } else {
            self.ai_selected = self.ai_selected.min(self.ai_items.len() - 1);
            self.ai_list_state.select(Some(self.ai_selected));
        }
    }

    fn fix_human_selection(&mut self) {
        if self.human_items.is_empty() {
            self.human_selected = 0;
            self.human_list_state.select(None);
        } else {
            self.human_selected = self.human_selected.min(self.human_items.len() - 1);
            self.human_list_state.select(Some(self.human_selected));
        }
    }

    fn fix_solved_selection(&mut self) {
        if self.solved_items.is_empty() {
            self.solved_selected = 0;
            self.solved_list_state.select(None);
        } else {
            self.solved_selected = self.solved_items.len() - 1;
            self.solved_list_state.select(Some(self.solved_selected));
        }
    }

    pub fn checked_ai_count(&self) -> usize {
        self.ai_items.iter().filter(|i| i.checked).count()
    }
}

// ──────────────── Confluence Tab State ────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfluenceFocus {
    Met,
    Unmet,
    Solved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolveStatus {
    Idle,
    Solving(u64),     // confluence ID being solved
    Solved(u64),      // ID of just-solved item (flash state)
}

pub struct SolvedConfluence {
    pub id: u64,
    pub name: String,
    pub solved_at: String,
    pub method: String, // "AI" or "Human"
}

pub struct ConfluenceState {
    pub data: ConfluenceData,
    pub met_list_state: ListState,
    pub unmet_list_state: ListState,
    pub met_selected: usize,
    pub unmet_selected: usize,
    pub solved_items: Vec<SolvedConfluence>,
    pub solved_list_state: ListState,
    pub solved_selected: usize,
    pub focus: ConfluenceFocus,
    pub solve_status: SolveStatus,
    pub solve_tick: u64,         // tick when solve started (for simulated delay)
    pub flash_tick: u64,         // tick when flash started (for green highlight)
    pub loaded: bool,
}

impl Default for ConfluenceState {
    fn default() -> Self {
        Self {
            data: ConfluenceData::default(),
            met_list_state: ListState::default(),
            unmet_list_state: ListState::default(),
            met_selected: 0,
            unmet_selected: 0,
            solved_items: Vec::new(),
            solved_list_state: ListState::default(),
            solved_selected: 0,
            focus: ConfluenceFocus::Met,
            solve_status: SolveStatus::Idle,
            solve_tick: 0,
            flash_tick: 0,
            loaded: false,
        }
    }
}

impl ConfluenceState {
    pub fn scroll_met(&mut self, delta: i32) {
        if self.data.met.is_empty() { return; }
        let max = self.data.met.len().saturating_sub(1);
        self.met_selected = if delta > 0 {
            (self.met_selected + delta as usize).min(max)
        } else {
            self.met_selected.saturating_sub((-delta) as usize)
        };
        self.met_list_state.select(Some(self.met_selected));
    }

    pub fn scroll_unmet(&mut self, delta: i32) {
        if self.data.unmet.is_empty() { return; }
        let max = self.data.unmet.len().saturating_sub(1);
        self.unmet_selected = if delta > 0 {
            (self.unmet_selected + delta as usize).min(max)
        } else {
            self.unmet_selected.saturating_sub((-delta) as usize)
        };
        self.unmet_list_state.select(Some(self.unmet_selected));
    }

    pub fn scroll_solved(&mut self, delta: i32) {
        if self.solved_items.is_empty() { return; }
        let max = self.solved_items.len().saturating_sub(1);
        self.solved_selected = if delta > 0 {
            (self.solved_selected + delta as usize).min(max)
        } else {
            self.solved_selected.saturating_sub((-delta) as usize)
        };
        self.solved_list_state.select(Some(self.solved_selected));
    }

    pub fn selected_confluence(&self) -> Option<&ConfluenceRecord> {
        match self.focus {
            ConfluenceFocus::Met => self.data.met.get(self.met_selected),
            ConfluenceFocus::Unmet => self.data.unmet.get(self.unmet_selected),
            ConfluenceFocus::Solved => None,
        }
    }

    /// Initiate a simulated solve on the currently selected confluence.
    pub fn trigger_solve(&mut self, tick: u64) -> bool {
        if !matches!(self.solve_status, SolveStatus::Idle) {
            return false;
        }
        if let Some(conf) = self.selected_confluence() {
            self.solve_status = SolveStatus::Solving(conf.id);
            self.solve_tick = tick;
            true
        } else {
            false
        }
    }

    /// Check if simulated solve has completed (called from tick).
    /// Returns true if a solve just completed and needs re-render.
    pub fn tick_solve(&mut self, tick: u64) -> bool {
        match self.solve_status {
            SolveStatus::Solving(id) => {
                // Simulate 2-second solve (10 ticks at 200ms)
                if tick.wrapping_sub(self.solve_tick) >= 10 {
                    // Find the record name for the solved item
                    let name = self.data.met.iter()
                        .chain(self.data.unmet.iter())
                        .find(|r| r.id == id)
                        .map(|r| format!("{} <> {}", r.issue_cluster_name, r.solution_cluster_name))
                        .unwrap_or_else(|| format!("Confluence #{}", id));

                    let now = chrono::Local::now().format("%H:%M").to_string();
                    self.solved_items.push(SolvedConfluence {
                        id,
                        name,
                        solved_at: now,
                        method: "AI".to_string(),
                    });

                    // Remove from met/unmet lists
                    self.data.met.retain(|r| r.id != id);
                    self.data.unmet.retain(|r| r.id != id);

                    // Fix selections after removal
                    if !self.data.met.is_empty() {
                        self.met_selected = self.met_selected.min(self.data.met.len() - 1);
                        self.met_list_state.select(Some(self.met_selected));
                    } else {
                        self.met_selected = 0;
                        self.met_list_state.select(None);
                    }
                    if !self.data.unmet.is_empty() {
                        self.unmet_selected = self.unmet_selected.min(self.data.unmet.len() - 1);
                        self.unmet_list_state.select(Some(self.unmet_selected));
                    } else {
                        self.unmet_selected = 0;
                        self.unmet_list_state.select(None);
                    }

                    // Select the new solved item
                    if !self.solved_items.is_empty() {
                        self.solved_selected = self.solved_items.len() - 1;
                        self.solved_list_state.select(Some(self.solved_selected));
                    }

                    self.solve_status = SolveStatus::Solved(id);
                    self.flash_tick = tick;
                    return true;
                }
            }
            SolveStatus::Solved(_) => {
                // Clear flash after 25 ticks (~5 seconds)
                if tick.wrapping_sub(self.flash_tick) >= 25 {
                    self.solve_status = SolveStatus::Idle;
                    return true;
                }
            }
            SolveStatus::Idle => {}
        }
        false
    }
}

pub struct App {
    pub source_counts: Vec<(String, u64)>,
    pub agent_counts: Vec<(String, u64)>,
    pub dropbox_runs: Vec<RunProgress>,
    pub email_runs: Vec<RunProgress>,
    pub recent_learnings: Vec<RecentLearning>,
    pub total_learnings: u64,
    pub db_size_bytes: u64,
    pub last_refresh: String,
    pub should_quit: bool,
    pub db_missing: bool,
    pub dropbox_runs_state: ListState,
    pub email_runs_state: ListState,
    pub recent_learnings_state: ListState,
    pub tick_count: u64,
    db_path: String,

    // Screen / navigation state
    pub screen: Screen,
    pub env_path: std::path::PathBuf,

    // Portal state
    pub portal: PortalState,

    // Tab state
    pub current_tab: Tab,

    // Research tab
    pub research_issues: Vec<ResearchIssue>,
    pub research_solutions: Vec<ResearchSolution>,
    pub research_stats: ResearchStats,
    pub research_issues_state: ListState,
    pub research_solutions_state: ListState,
    pub research_db_missing: bool,
    research_db_path: String,

    // Issues tab
    pub issues_state: IssuesState,

    // Solutions tab
    pub solutions_state: SolutionsState,

    // Confluence tab
    pub confluence_state: ConfluenceState,

    // Solve tab
    pub solve_state: SolveState,

    // Settings tab
    pub settings: SettingsState,
}

impl App {
    pub fn new(db_path: String, research_db_path: String) -> Self {
        let env_path = env_store::resolve_env_path();
        let screen = if env_store::has_credentials(&env_path) {
            Screen::Main
        } else {
            Screen::Welcome
        };

        let mut portal = PortalState::new();
        let env_values = env_store::load(&env_path);
        portal.load_from_env(&env_values);

        let settings_dir = env_path.parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        let settings = SettingsState::new(&settings_dir);

        let mut app = Self {
            source_counts: Vec::new(),
            agent_counts: Vec::new(),
            dropbox_runs: Vec::new(),
            email_runs: Vec::new(),
            recent_learnings: Vec::new(),
            total_learnings: 0,
            db_size_bytes: 0,
            last_refresh: String::new(),
            should_quit: false,
            db_missing: false,
            dropbox_runs_state: ListState::default(),
            email_runs_state: ListState::default(),
            recent_learnings_state: ListState::default(),
            tick_count: 0,
            db_path,
            screen,
            env_path,
            portal,
            current_tab: Tab::Learnings,
            research_issues: Vec::new(),
            research_solutions: Vec::new(),
            research_stats: ResearchStats::default(),
            research_issues_state: ListState::default(),
            research_solutions_state: ListState::default(),
            research_db_missing: false,
            research_db_path,
            issues_state: IssuesState::default(),
            solutions_state: SolutionsState::default(),
            confluence_state: ConfluenceState::default(),
            solve_state: SolveState::default(),
            settings,
        };
        app.refresh();
        app
    }

    pub fn next_tab(&mut self) {
        let i = self.current_tab.index();
        let next = Tab::from_index((i + 1) % Tab::ALL.len()).unwrap();
        self.set_tab(next);
    }

    pub fn prev_tab(&mut self) {
        let i = self.current_tab.index();
        let prev = Tab::from_index((i + Tab::ALL.len() - 1) % Tab::ALL.len()).unwrap();
        self.set_tab(prev);
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.current_tab = tab;
        // Refresh data on tab switch (lazy load, not every tick)
        match tab {
            Tab::Issues => self.refresh_issues(),
            Tab::Solutions => self.refresh_solutions(),
            Tab::Confluence => self.refresh_confluences(),
            Tab::Solve => self.refresh_solve(),
            _ => {}
        }
    }

    pub fn scroll_dropbox_runs(&mut self, delta: i32) {
        if self.dropbox_runs.is_empty() { return; }
        let max = self.dropbox_runs.len().saturating_sub(1);
        let current = self.dropbox_runs_state.selected().unwrap_or(0);
        let new_pos = if delta > 0 { (current + delta as usize).min(max) } else { current.saturating_sub((-delta) as usize) };
        self.dropbox_runs_state.select(Some(new_pos));
    }

    pub fn scroll_email_runs(&mut self, delta: i32) {
        if self.email_runs.is_empty() { return; }
        let max = self.email_runs.len().saturating_sub(1);
        let current = self.email_runs_state.selected().unwrap_or(0);
        let new_pos = if delta > 0 { (current + delta as usize).min(max) } else { current.saturating_sub((-delta) as usize) };
        self.email_runs_state.select(Some(new_pos));
    }

    pub fn scroll_recent_learnings(&mut self, delta: i32) {
        if self.recent_learnings.is_empty() { return; }
        let max = self.recent_learnings.len().saturating_sub(1);
        let current = self.recent_learnings_state.selected().unwrap_or(0);
        let new_pos = if delta > 0 { (current + delta as usize).min(max) } else { current.saturating_sub((-delta) as usize) };
        self.recent_learnings_state.select(Some(new_pos));
    }

    pub fn scroll_research_issues(&mut self, delta: i32) {
        if self.research_issues.is_empty() { return; }
        let max = self.research_issues.len().saturating_sub(1);
        let current = self.research_issues_state.selected().unwrap_or(0);
        let new_pos = if delta > 0 { (current + delta as usize).min(max) } else { current.saturating_sub((-delta) as usize) };
        self.research_issues_state.select(Some(new_pos));
    }

    pub fn scroll_research_solutions(&mut self, delta: i32) {
        if self.research_solutions.is_empty() { return; }
        let max = self.research_solutions.len().saturating_sub(1);
        let current = self.research_solutions_state.selected().unwrap_or(0);
        let new_pos = if delta > 0 { (current + delta as usize).min(max) } else { current.saturating_sub((-delta) as usize) };
        self.research_solutions_state.select(Some(new_pos));
    }

    pub fn research_db_path(&self) -> &str {
        &self.research_db_path
    }

    /// Derive mesh.db path from research.db path (same parent directory)
    pub fn mesh_db_path(&self) -> Option<String> {
        let p = std::path::Path::new(&self.research_db_path);
        p.parent().map(|dir| dir.join("mesh.db").to_string_lossy().to_string())
    }

    pub fn refresh_issues(&mut self) {
        let mesh_path = self.mesh_db_path();
        match db::fetch_issues_detailed(&self.research_db_path, mesh_path.as_deref()) {
            Some(data) => {
                self.issues_state.stats = data.stats;
                self.issues_state.severity_filter.set_options(&self.issues_state.stats.by_severity.clone());
                self.issues_state.status_filter.set_options(&self.issues_state.stats.by_status.clone());
                self.issues_state.category_filter.set_options(&self.issues_state.stats.by_category.clone());
                self.issues_state.issues = data.issues;
                self.issues_state.apply_filters();
                // Build tree view from issues
                self.issues_state.tree = TreeState::from_issues(&self.issues_state.issues);
                self.issues_state.loaded = true;
            }
            None => {
                self.issues_state.loaded = false;
            }
        }
    }

    pub fn refresh_confluences(&mut self) {
        if let Some(mesh_path) = self.mesh_db_path() {
            match db::fetch_confluences(&mesh_path) {
                Some(data) => {
                    // Preserve solved items across refreshes
                    let solved = std::mem::take(&mut self.confluence_state.solved_items);
                    let focus = self.confluence_state.focus;
                    let solve_status = self.confluence_state.solve_status;
                    let solve_tick = self.confluence_state.solve_tick;
                    let flash_tick = self.confluence_state.flash_tick;

                    self.confluence_state.data = data;
                    self.confluence_state.solved_items = solved;
                    self.confluence_state.focus = focus;
                    self.confluence_state.solve_status = solve_status;
                    self.confluence_state.solve_tick = solve_tick;
                    self.confluence_state.flash_tick = flash_tick;

                    // Initialize list selections
                    if !self.confluence_state.data.met.is_empty() && self.confluence_state.met_list_state.selected().is_none() {
                        self.confluence_state.met_list_state.select(Some(0));
                    }
                    if !self.confluence_state.data.unmet.is_empty() && self.confluence_state.unmet_list_state.selected().is_none() {
                        self.confluence_state.unmet_list_state.select(Some(0));
                    }
                    self.confluence_state.loaded = true;
                }
                None => {
                    self.confluence_state.loaded = false;
                }
            }
        } else {
            self.confluence_state.loaded = false;
        }
    }

    pub fn refresh_solve(&mut self) {
        // Only load if not already loaded (to preserve in-session state)
        if self.solve_state.loaded {
            return;
        }
        if let Some(mesh_path) = self.mesh_db_path() {
            match db::fetch_lvl2_analyses(&mesh_path) {
                Some(data) => {
                    let mut ai_items = Vec::new();
                    let mut human_items = Vec::new();
                    for analysis in data.analyses {
                        let item = SolveItem {
                            id: analysis.id,
                            name: analysis.cluster_name,
                            summary: analysis.strategy_summary,
                            checked: false,
                            strikethrough: false,
                            strikethrough_tick: None,
                            green_flash_until: None,
                            solving: false,
                            queued: false,
                        };
                        match analysis.solvable_by {
                            SolvableBy::AI => ai_items.push(item),
                            SolvableBy::Human | SolvableBy::Unknown => human_items.push(item),
                        }
                    }
                    self.solve_state.ai_count = data.ai_count;
                    self.solve_state.human_count = data.human_count;
                    self.solve_state.total_count = data.ai_count + data.human_count;
                    self.solve_state.ai_items = ai_items;
                    self.solve_state.human_items = human_items;
                    if !self.solve_state.ai_items.is_empty() {
                        self.solve_state.ai_list_state.select(Some(0));
                    }
                    if !self.solve_state.human_items.is_empty() {
                        self.solve_state.human_list_state.select(Some(0));
                    }
                    // Build tree views for solve items
                    self.solve_state.ai_tree = TreeState::from_solve_items(&self.solve_state.ai_items, "AI Solvable");
                    self.solve_state.human_tree = TreeState::from_solve_items(&self.solve_state.human_items, "Human Solvable");
                    self.solve_state.loaded = true;
                }
                None => {
                    self.solve_state.loaded = false;
                }
            }
        }
    }

    pub fn refresh_solutions(&mut self) {
        match db::fetch_solutions_detailed(&self.research_db_path) {
            Some(data) => {
                self.solutions_state.stats = data.stats;
                self.solutions_state.solutions = data.solutions;
                if !self.solutions_state.solutions.is_empty() && self.solutions_state.list_state.selected().is_none() {
                    self.solutions_state.list_state.select(Some(0));
                }
                // Build tree view from solutions
                self.solutions_state.tree = TreeState::from_solutions(&self.solutions_state.solutions);
                self.solutions_state.loaded = true;
            }
            None => {
                self.solutions_state.loaded = false;
            }
        }
    }

    pub fn has_focused_input(&self) -> bool {
        match self.current_tab {
            Tab::Portal => self.portal.has_focused_input(),
            Tab::Settings => self.settings.has_focused_input(),
            _ => self.is_search_active(),
        }
    }

    /// Check if any search bar is currently active.
    pub fn is_search_active(&self) -> bool {
        match self.current_tab {
            Tab::Issues => self.issues_state.search.active,
            Tab::Solutions => self.solutions_state.search.active,
            Tab::Solve => self.solve_state.search.active,
            _ => false,
        }
    }

    pub fn has_focused_widget(&self) -> bool {
        match self.current_tab {
            Tab::Portal => true, // Portal always captures Tab for its focus chain
            Tab::Settings => true, // Settings always captures Tab for its focus chain
            _ => false,
        }
    }

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
        // Clear portal status after 15 ticks (~3 seconds)
        if self.portal.status_message.is_some() {
            if self.tick_count.wrapping_sub(self.portal.status_tick) > 15 {
                self.portal.status_message = None;
            }
        }
        // Clear settings status after 15 ticks (~3 seconds)
        if self.settings.status_message.is_some() {
            if self.tick_count.wrapping_sub(self.settings.status_tick) > 15 {
                self.settings.status_message = None;
            }
        }
        // Advance confluence solve simulation
        self.confluence_state.tick_solve(self.tick_count);
        // Advance solve tab animations
        self.solve_state.tick(self.tick_count);
    }

    pub fn refresh(&mut self) {
        self.last_refresh = Local::now().format("%H:%M:%S").to_string();
        self.refresh_learnings();
        self.refresh_research();
    }

    fn refresh_learnings(&mut self) {
        match db::fetch_learnings(&self.db_path) {
            Some(data) => {
                self.db_missing = false;
                self.source_counts = data.source_counts;
                self.agent_counts = data.agent_counts;
                self.dropbox_runs = data.dropbox_runs;
                self.email_runs = data.email_runs;
                self.recent_learnings = data.recent_learnings;
                self.total_learnings = data.total_learnings;
                self.db_size_bytes = data.db_size_bytes;
            }
            None => {
                self.db_missing = true;
            }
        }
    }

    fn refresh_research(&mut self) {
        match db::fetch_research(&self.research_db_path) {
            Some(data) => {
                self.research_db_missing = false;
                self.research_issues = data.issues;
                self.research_solutions = data.solutions;
                self.research_stats = data.stats;
            }
            None => {
                self.research_db_missing = true;
            }
        }
    }

    pub fn format_db_size(&self) -> String {
        format_size(self.db_size_bytes)
    }

    pub fn format_research_db_size(&self) -> String {
        format_size(self.research_stats.db_size_bytes)
    }
}

fn format_size(bytes: u64) -> String {
    let b = bytes as f64;
    if b < 1024.0 { format!("{} B", bytes) }
    else if b < 1024.0 * 1024.0 { format!("{:.1} KB", b / 1024.0) }
    else { format!("{:.1} MB", b / (1024.0 * 1024.0)) }
}
