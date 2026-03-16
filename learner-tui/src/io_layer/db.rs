use rusqlite::{Connection, OpenFlags};
use std::fs;
use std::path::Path;

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

pub struct LearningsData {
    pub source_counts: Vec<(String, u64)>,
    pub agent_counts: Vec<(String, u64)>,
    pub dropbox_runs: Vec<RunProgress>,
    pub email_runs: Vec<RunProgress>,
    pub recent_learnings: Vec<RecentLearning>,
    pub total_learnings: u64,
    pub db_size_bytes: u64,
}

pub struct ResearchData {
    pub issues: Vec<ResearchIssue>,
    pub solutions: Vec<ResearchSolution>,
    pub stats: ResearchStats,
}

/// Query learnings.db and return all dashboard data.
/// Returns None if the DB file is missing or cannot be opened.
pub fn fetch_learnings(db_path: &str) -> Option<LearningsData> {
    let db_file = Path::new(db_path);
    if !db_file.exists() {
        return None;
    }

    let db_size_bytes = fs::metadata(db_file).map(|m| m.len()).unwrap_or(0);

    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;

    let source_counts = conn
        .prepare("SELECT source, COUNT(*) FROM learnings GROUP BY source ORDER BY COUNT(*) DESC")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)))
                .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let agent_counts = conn
        .prepare("SELECT agent, COUNT(*) FROM learnings GROUP BY agent ORDER BY COUNT(*) DESC")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)))
                .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let total_learnings = conn
        .query_row("SELECT COUNT(*) FROM learnings", [], |row| row.get(0))
        .unwrap_or(0);

    let all_runs: Vec<RunProgress> = conn
        .prepare(
            "SELECT COALESCE(source, ''), COALESCE(agent, ''), COALESCE(folder, ''), \
             total_files, processed, status, COALESCE(pid, 0) \
             FROM run_progress WHERE status IN ('running', 'watching') \
             ORDER BY updated_at DESC",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok(RunProgress {
                    source: row.get(0)?,
                    agent: row.get(1)?,
                    folder: row.get(2)?,
                    total_files: row.get(3)?,
                    processed: row.get(4)?,
                    status: row.get(5)?,
                    pid: row.get::<_, i64>(6).unwrap_or(0) as u64,
                })
            })
            .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let mut dropbox_runs = Vec::new();
    let mut email_runs = Vec::new();
    for run in all_runs {
        if run.source == "dropbox" {
            dropbox_runs.push(run);
        } else {
            email_runs.push(run);
        }
    }

    let recent_learnings = conn
        .prepare("SELECT agent, learning, processed_at FROM learnings ORDER BY id DESC LIMIT 100")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                let processed_at: String = row.get::<_, String>(2).unwrap_or_default();
                let display_time = if processed_at.len() >= 16 {
                    processed_at[11..16].to_string()
                } else {
                    processed_at.clone()
                };
                Ok(RecentLearning {
                    agent: row.get(0)?,
                    learning: row.get(1)?,
                    processed_at: display_time,
                })
            })
            .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    Some(LearningsData {
        source_counts,
        agent_counts,
        dropbox_runs,
        email_runs,
        recent_learnings,
        total_learnings,
        db_size_bytes,
    })
}

/// Query research.db and return all research data.
/// Returns None if the DB file is missing or cannot be opened.
pub fn fetch_research(db_path: &str) -> Option<ResearchData> {
    let db_file = Path::new(db_path);
    if !db_file.exists() {
        return None;
    }

    let db_size_bytes = fs::metadata(db_file).map(|m| m.len()).unwrap_or(0);

    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;

    let mut stats = ResearchStats {
        db_size_bytes,
        ..ResearchStats::default()
    };

    stats.total_issues = conn
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap_or(0);
    stats.open_issues = conn
        .query_row(
            "SELECT COUNT(*) FROM issues WHERE status IN ('open', 'researching')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    stats.solved_issues = conn
        .query_row(
            "SELECT COUNT(*) FROM issues WHERE status = 'solved'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    stats.total_solutions = conn
        .query_row("SELECT COUNT(*) FROM solutions", [], |row| row.get(0))
        .unwrap_or(0);
    stats.pending_digest = conn
        .query_row("SELECT COUNT(*) FROM daily_output", [], |row| row.get(0))
        .unwrap_or(0);

    if let Ok(row) = conn.query_row(
        "SELECT COALESCE(last_scan_at, ''), COALESCE(last_digest_at, '') \
         FROM scan_cursor WHERE id = 1",
        [],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ) {
        stats.last_scan_at = if row.0.len() >= 16 {
            row.0[11..16].to_string()
        } else {
            row.0
        };
        stats.last_digest_at = if row.1.len() >= 16 {
            row.1[11..16].to_string()
        } else {
            row.1
        };
    }

    let issues = conn
        .prepare(
            "SELECT title, COALESCE(category, ''), COALESCE(severity, ''), \
             COALESCE(status, ''), COALESCE(created_at, '') \
             FROM issues ORDER BY id DESC LIMIT 100",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                let created_at: String = row.get(4)?;
                let display_time = if created_at.len() >= 16 {
                    created_at[11..16].to_string()
                } else {
                    created_at.clone()
                };
                Ok(ResearchIssue {
                    title: row.get(0)?,
                    category: row.get(1)?,
                    severity: row.get(2)?,
                    status: row.get(3)?,
                    created_at: display_time,
                })
            })
            .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let solutions = conn
        .prepare(
            "SELECT COALESCE(i.title, ''), COALESCE(s.summary, ''), \
             COALESCE(s.source_url, ''), COALESCE(s.confidence, ''), COALESCE(s.created_at, '') \
             FROM solutions s JOIN issues i ON s.issue_id = i.id \
             ORDER BY s.id DESC LIMIT 100",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                let created_at: String = row.get(4)?;
                let display_time = if created_at.len() >= 16 {
                    created_at[11..16].to_string()
                } else {
                    created_at.clone()
                };
                Ok(ResearchSolution {
                    issue_title: row.get(0)?,
                    summary: row.get(1)?,
                    source_url: row.get(2)?,
                    confidence: row.get(3)?,
                    created_at: display_time,
                })
            })
            .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    Some(ResearchData {
        issues,
        solutions,
        stats,
    })
}
