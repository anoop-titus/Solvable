use chrono::Local;
use ratatui::widgets::ListState;
use rusqlite::{Connection, OpenFlags};
use std::fs;
use std::path::Path;

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

#[allow(dead_code)]
pub struct RunProgress {
    pub source: String,
    pub agent: String,
    pub folder: String,
    pub total_files: u64,
    pub processed: u64,
    pub status: String,
    pub pid: u64,
}

pub struct RecentLearning {
    pub agent: String,
    pub learning: String,
    pub processed_at: String,
}

pub struct ResearchIssue {
    pub title: String,
    pub category: String,
    pub severity: String,
    pub status: String,
    pub created_at: String,
}

pub struct ResearchSolution {
    pub issue_title: String,
    pub summary: String,
    pub source_url: String,
    pub confidence: String,
    pub created_at: String,
}

pub struct ResearchStats {
    pub total_issues: u64,
    pub open_issues: u64,
    pub solved_issues: u64,
    pub total_solutions: u64,
    pub pending_digest: u64,
    pub last_scan_at: String,
    pub last_digest_at: String,
    pub db_size_bytes: u64,
}

impl Default for ResearchStats {
    fn default() -> Self {
        Self {
            total_issues: 0,
            open_issues: 0,
            solved_issues: 0,
            total_solutions: 0,
            pending_digest: 0,
            last_scan_at: String::new(),
            last_digest_at: String::new(),
            db_size_bytes: 0,
        }
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
}

impl App {
    pub fn new(db_path: String, research_db_path: String) -> Self {
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
            current_tab: Tab::Learnings,
            research_issues: Vec::new(),
            research_solutions: Vec::new(),
            research_stats: ResearchStats::default(),
            research_issues_state: ListState::default(),
            research_solutions_state: ListState::default(),
            research_db_missing: false,
            research_db_path,
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

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.wrapping_add(1);
    }

    pub fn refresh(&mut self) {
        self.last_refresh = Local::now().format("%H:%M:%S").to_string();
        self.refresh_learnings();
        self.refresh_research();
    }

    fn refresh_learnings(&mut self) {
        let db_file = Path::new(&self.db_path);
        if !db_file.exists() { self.db_missing = true; return; }
        self.db_missing = false;

        if let Ok(meta) = fs::metadata(db_file) { self.db_size_bytes = meta.len(); }

        let conn = match Connection::open_with_flags(&self.db_path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX) {
            Ok(c) => c,
            Err(_) => { self.db_missing = true; return; }
        };

        self.source_counts = conn
            .prepare("SELECT source, COUNT(*) FROM learnings GROUP BY source ORDER BY COUNT(*) DESC")
            .and_then(|mut stmt| stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))).and_then(|rows| rows.collect()))
            .unwrap_or_default();

        self.agent_counts = conn
            .prepare("SELECT agent, COUNT(*) FROM learnings GROUP BY agent ORDER BY COUNT(*) DESC")
            .and_then(|mut stmt| stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))).and_then(|rows| rows.collect()))
            .unwrap_or_default();

        self.total_learnings = conn.query_row("SELECT COUNT(*) FROM learnings", [], |row| row.get(0)).unwrap_or(0);

        let all_runs: Vec<RunProgress> = conn
            .prepare("SELECT COALESCE(source, ''), COALESCE(agent, ''), COALESCE(folder, ''), total_files, processed, status, COALESCE(pid, 0) FROM run_progress WHERE status IN ('running', 'watching') ORDER BY updated_at DESC")
            .and_then(|mut stmt| stmt.query_map([], |row| Ok(RunProgress {
                source: row.get(0)?, agent: row.get(1)?, folder: row.get(2)?,
                total_files: row.get(3)?, processed: row.get(4)?, status: row.get(5)?,
                pid: row.get::<_, i64>(6).unwrap_or(0) as u64,
            })).and_then(|rows| rows.collect()))
            .unwrap_or_default();

        self.dropbox_runs = Vec::new();
        self.email_runs = Vec::new();
        for run in all_runs {
            if run.source == "dropbox" { self.dropbox_runs.push(run); } else { self.email_runs.push(run); }
        }

        self.recent_learnings = conn
            .prepare("SELECT agent, learning, processed_at FROM learnings ORDER BY id DESC LIMIT 100")
            .and_then(|mut stmt| stmt.query_map([], |row| {
                let processed_at: String = row.get::<_, String>(2).unwrap_or_default();
                let display_time = if processed_at.len() >= 16 { processed_at[11..16].to_string() } else { processed_at.clone() };
                Ok(RecentLearning { agent: row.get(0)?, learning: row.get(1)?, processed_at: display_time })
            }).and_then(|rows| rows.collect()))
            .unwrap_or_default();
    }

    fn refresh_research(&mut self) {
        let db_file = Path::new(&self.research_db_path);
        if !db_file.exists() { self.research_db_missing = true; return; }
        self.research_db_missing = false;

        if let Ok(meta) = fs::metadata(db_file) { self.research_stats.db_size_bytes = meta.len(); }

        let conn = match Connection::open_with_flags(&self.research_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX) {
            Ok(c) => c,
            Err(_) => { self.research_db_missing = true; return; }
        };

        self.research_stats.total_issues = conn.query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0)).unwrap_or(0);
        self.research_stats.open_issues = conn.query_row("SELECT COUNT(*) FROM issues WHERE status IN ('open', 'researching')", [], |row| row.get(0)).unwrap_or(0);
        self.research_stats.solved_issues = conn.query_row("SELECT COUNT(*) FROM issues WHERE status = 'solved'", [], |row| row.get(0)).unwrap_or(0);
        self.research_stats.total_solutions = conn.query_row("SELECT COUNT(*) FROM solutions", [], |row| row.get(0)).unwrap_or(0);
        self.research_stats.pending_digest = conn.query_row("SELECT COUNT(*) FROM daily_output", [], |row| row.get(0)).unwrap_or(0);

        if let Ok(row) = conn.query_row(
            "SELECT COALESCE(last_scan_at, ''), COALESCE(last_digest_at, '') FROM scan_cursor WHERE id = 1", [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        ) {
            self.research_stats.last_scan_at = if row.0.len() >= 16 { row.0[11..16].to_string() } else { row.0 };
            self.research_stats.last_digest_at = if row.1.len() >= 16 { row.1[11..16].to_string() } else { row.1 };
        }

        self.research_issues = conn
            .prepare("SELECT title, COALESCE(category, ''), COALESCE(severity, ''), COALESCE(status, ''), COALESCE(created_at, '') FROM issues ORDER BY id DESC LIMIT 100")
            .and_then(|mut stmt| stmt.query_map([], |row| {
                let created_at: String = row.get(4)?;
                let display_time = if created_at.len() >= 16 { created_at[11..16].to_string() } else { created_at.clone() };
                Ok(ResearchIssue { title: row.get(0)?, category: row.get(1)?, severity: row.get(2)?, status: row.get(3)?, created_at: display_time })
            }).and_then(|rows| rows.collect()))
            .unwrap_or_default();

        self.research_solutions = conn
            .prepare("SELECT COALESCE(i.title, ''), COALESCE(s.summary, ''), COALESCE(s.source_url, ''), COALESCE(s.confidence, ''), COALESCE(s.created_at, '') FROM solutions s JOIN issues i ON s.issue_id = i.id ORDER BY s.id DESC LIMIT 100")
            .and_then(|mut stmt| stmt.query_map([], |row| {
                let created_at: String = row.get(4)?;
                let display_time = if created_at.len() >= 16 { created_at[11..16].to_string() } else { created_at.clone() };
                Ok(ResearchSolution { issue_title: row.get(0)?, summary: row.get(1)?, source_url: row.get(2)?, confidence: row.get(3)?, created_at: display_time })
            }).and_then(|rows| rows.collect()))
            .unwrap_or_default();
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
