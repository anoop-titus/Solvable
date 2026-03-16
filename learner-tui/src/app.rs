use chrono::Local;
use ratatui::widgets::ListState;

use crate::io_layer::db::{
    self, IssueDetail, IssueStats, RecentLearning, ResearchIssue,
    ResearchSolution, ResearchStats, RunProgress, SolutionDetail, SolutionStats,
};
use crate::io_layer::env_store;
use crate::screens::portal::PortalState;
use crate::screens::settings::SettingsState;

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
            settings,
        };
        app.refresh();
        app
    }

    pub fn next_tab(&mut self) {
        let i = self.current_tab.index();
        self.current_tab = Tab::from_index((i + 1) % Tab::ALL.len()).unwrap();
    }

    pub fn prev_tab(&mut self) {
        let i = self.current_tab.index();
        self.current_tab = Tab::from_index((i + Tab::ALL.len() - 1) % Tab::ALL.len()).unwrap();
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.current_tab = tab;
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
                self.issues_state.loaded = true;
            }
            None => {
                self.issues_state.loaded = false;
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
