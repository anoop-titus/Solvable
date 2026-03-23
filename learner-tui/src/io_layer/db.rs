use rusqlite::{Connection, OpenFlags};
use serde_json;
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

// ──────────────── Issues Tab (detailed) ────────────────

pub struct IssueDetail {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub category: String,
    pub severity: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub solution_count: u64,
    pub cluster_name: Option<String>,
}

pub struct IssueStats {
    pub total: u64,
    pub by_severity: Vec<(String, u64)>,
    pub by_status: Vec<(String, u64)>,
    pub by_category: Vec<(String, u64)>,
}

impl Default for IssueStats {
    fn default() -> Self {
        Self {
            total: 0,
            by_severity: Vec::new(),
            by_status: Vec::new(),
            by_category: Vec::new(),
        }
    }
}

pub struct IssuesDetailedData {
    pub issues: Vec<IssueDetail>,
    pub stats: IssueStats,
}

/// Query research.db (and optionally mesh.db) for the full Issues tab view.
pub fn fetch_issues_detailed(research_db: &str, mesh_db: Option<&str>) -> Option<IssuesDetailedData> {
    let db_file = Path::new(research_db);
    if !db_file.exists() {
        return None;
    }

    let conn = Connection::open_with_flags(
        research_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;

    // Compute stats
    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM issues", [], |row| row.get(0))
        .unwrap_or(0);

    let by_severity = conn
        .prepare("SELECT COALESCE(severity, 'unknown'), COUNT(*) FROM issues GROUP BY severity ORDER BY COUNT(*) DESC")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)))
                .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let by_status = conn
        .prepare("SELECT COALESCE(status, 'unknown'), COUNT(*) FROM issues GROUP BY status ORDER BY COUNT(*) DESC")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)))
                .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let by_category = conn
        .prepare("SELECT COALESCE(category, 'unknown'), COUNT(*) FROM issues GROUP BY category ORDER BY COUNT(*) DESC")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)))
                .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let stats = IssueStats {
        total,
        by_severity,
        by_status,
        by_category,
    };

    // Load cluster mapping from mesh.db if available
    let mut cluster_map: std::collections::HashMap<u64, String> = std::collections::HashMap::new();
    if let Some(mesh_path) = mesh_db {
        if Path::new(mesh_path).exists() {
            if let Ok(mesh_conn) = Connection::open_with_flags(
                mesh_path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) {
                if let Ok(mut stmt) = mesh_conn.prepare("SELECT id, name, member_ids FROM issue_clusters") {
                    if let Ok(rows) = stmt.query_map([], |row| {
                        Ok((
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                        ))
                    }) {
                        for row in rows.flatten() {
                            let (cluster_name, member_ids_json) = row;
                            // member_ids is a JSON array of issue IDs
                            if let Ok(ids) = serde_json::from_str::<Vec<u64>>(&member_ids_json) {
                                for id in ids {
                                    cluster_map.insert(id, cluster_name.clone());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fetch all issues with solution counts
    let issues = conn
        .prepare(
            "SELECT i.id, i.title, COALESCE(i.description, ''), COALESCE(i.category, ''), \
             COALESCE(i.severity, ''), COALESCE(i.status, ''), COALESCE(i.created_at, ''), \
             COALESCE(i.updated_at, ''), \
             (SELECT COUNT(*) FROM solutions s WHERE s.issue_id = i.id) as sol_count \
             FROM issues i ORDER BY i.id DESC",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                let id: u64 = row.get(0)?;
                let created_at: String = row.get(6)?;
                let display_created = if created_at.len() >= 16 {
                    created_at[..16].to_string()
                } else {
                    created_at
                };
                let updated_at: String = row.get(7)?;
                let display_updated = if updated_at.len() >= 16 {
                    updated_at[..16].to_string()
                } else {
                    updated_at
                };
                Ok((id, row.get::<_, String>(1)?, row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?, row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?, display_created, display_updated,
                    row.get::<_, u64>(8)?))
            })
            .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
        })
        .unwrap_or_default();

    let issue_details: Vec<IssueDetail> = issues
        .into_iter()
        .map(|(id, title, description, category, severity, status, created_at, updated_at, solution_count)| {
            let cluster_name = cluster_map.get(&id).cloned();
            IssueDetail {
                id,
                title,
                description,
                category,
                severity,
                status,
                created_at,
                updated_at,
                solution_count,
                cluster_name,
            }
        })
        .collect();

    Some(IssuesDetailedData {
        issues: issue_details,
        stats,
    })
}

// ──────────────── Solutions Tab (detailed) ────────────────

pub struct SolutionDetail {
    pub id: u64,
    pub issue_id: u64,
    pub issue_title: String,
    pub summary: String,
    pub source_url: String,
    pub source_title: String,
    pub confidence: String,
    pub created_at: String,
    pub issue_severity: String,
    pub issue_status: String,
}

pub struct SolutionStats {
    pub total: u64,
    pub by_confidence: Vec<(String, u64)>,
}

impl Default for SolutionStats {
    fn default() -> Self {
        Self {
            total: 0,
            by_confidence: Vec::new(),
        }
    }
}

pub struct SolutionsDetailedData {
    pub solutions: Vec<SolutionDetail>,
    pub stats: SolutionStats,
}

/// Query research.db for the full Solutions tab view.
pub fn fetch_solutions_detailed(research_db: &str) -> Option<SolutionsDetailedData> {
    let db_file = Path::new(research_db);
    if !db_file.exists() {
        return None;
    }

    let conn = Connection::open_with_flags(
        research_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;

    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM solutions", [], |row| row.get(0))
        .unwrap_or(0);

    let by_confidence = conn
        .prepare("SELECT COALESCE(confidence, 'unknown'), COUNT(*) FROM solutions GROUP BY confidence ORDER BY COUNT(*) DESC")
        .and_then(|mut stmt| {
            stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?)))
                .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let stats = SolutionStats {
        total,
        by_confidence,
    };

    let solutions = conn
        .prepare(
            "SELECT s.id, s.issue_id, COALESCE(i.title, ''), COALESCE(s.summary, ''), \
             COALESCE(s.source_url, ''), COALESCE(s.source_title, ''), \
             COALESCE(s.confidence, ''), COALESCE(s.created_at, ''), \
             COALESCE(i.severity, ''), COALESCE(i.status, '') \
             FROM solutions s JOIN issues i ON s.issue_id = i.id \
             ORDER BY s.id DESC",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                let created_at: String = row.get(7)?;
                let display_created = if created_at.len() >= 16 {
                    created_at[..16].to_string()
                } else {
                    created_at
                };
                Ok(SolutionDetail {
                    id: row.get(0)?,
                    issue_id: row.get(1)?,
                    issue_title: row.get(2)?,
                    summary: row.get(3)?,
                    source_url: row.get(4)?,
                    source_title: row.get(5)?,
                    confidence: row.get(6)?,
                    created_at: display_created,
                    issue_severity: row.get(8)?,
                    issue_status: row.get(9)?,
                })
            })
            .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    Some(SolutionsDetailedData {
        solutions,
        stats,
    })
}

// ──────────────── Confluence Tab ────────────────

pub struct ConfluenceRecord {
    pub id: u64,
    pub issue_cluster_name: String,
    pub solution_cluster_name: String,
    pub topical_similarity: f64,
    pub confluence_score: f64,
    pub status: String,
    pub computed_at: String,
}

pub struct ConfluenceData {
    pub met: Vec<ConfluenceRecord>,
    pub unmet: Vec<ConfluenceRecord>,
    pub gap: Vec<ConfluenceRecord>,
    pub distant: Vec<ConfluenceRecord>,
    pub stale: Vec<ConfluenceRecord>,
    pub total: u64,
}

impl Default for ConfluenceData {
    fn default() -> Self {
        Self {
            met: Vec::new(),
            unmet: Vec::new(),
            gap: Vec::new(),
            distant: Vec::new(),
            stale: Vec::new(),
            total: 0,
        }
    }
}

/// Query mesh.db for confluence data, split by status.
pub fn fetch_confluences(mesh_db: &str) -> Option<ConfluenceData> {
    let db_file = Path::new(mesh_db);
    if !db_file.exists() {
        return None;
    }

    let conn = Connection::open_with_flags(
        mesh_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;

    // Check if confluences table exists
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='confluences'",
            [],
            |row| row.get::<_, u64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !table_exists {
        return None;
    }

    let total: u64 = conn
        .query_row("SELECT COUNT(*) FROM confluences", [], |row| row.get(0))
        .unwrap_or(0);

    let all_records: Vec<ConfluenceRecord> = conn
        .prepare(
            "SELECT id, COALESCE(issue_cluster_name, ''), COALESCE(solution_cluster_name, ''), \
             COALESCE(topical_similarity, 0.0), COALESCE(confluence_score, 0.0), \
             COALESCE(status, 'unknown'), COALESCE(computed_at, '') \
             FROM confluences ORDER BY confluence_score DESC",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                let computed_at: String = row.get(6)?;
                let display_time = if computed_at.len() >= 16 {
                    computed_at[..16].to_string()
                } else {
                    computed_at
                };
                Ok(ConfluenceRecord {
                    id: row.get(0)?,
                    issue_cluster_name: row.get(1)?,
                    solution_cluster_name: row.get(2)?,
                    topical_similarity: row.get(3)?,
                    confluence_score: row.get(4)?,
                    status: row.get(5)?,
                    computed_at: display_time,
                })
            })
            .and_then(|rows| rows.collect())
        })
        .unwrap_or_default();

    let mut data = ConfluenceData {
        total,
        ..ConfluenceData::default()
    };

    for record in all_records {
        match record.status.as_str() {
            "met" => data.met.push(record),
            "unmet" => data.unmet.push(record),
            "gap" => data.gap.push(record),
            "distant" => data.distant.push(record),
            "stale" => data.stale.push(record),
            _ => data.gap.push(record), // unknown status -> gap
        }
    }

    Some(data)
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

// ──────────────── Solve Tab (Lvl2 Analyses) ────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolvableBy {
    Unknown,
    Human,
    AI,
}

pub struct Lvl2Analysis {
    pub id: u64,
    pub cluster_id: u64,
    pub cluster_name: String,
    pub strategy_summary: String,
    pub auto_actions: Vec<String>,
    pub output_path: String,
    pub generated_at: String,
    pub solvable_by: SolvableBy,
    pub source: Lvl2Source,
    pub severity: u8,   // 0=critical 1=high 2=medium 3=low (from issue_clusters join)
    pub surface: bool,  // true if all auto_actions are surface-only (telegram/airtable/db_update)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lvl2Source {
    Cluster,
    Issue,
}

pub struct Lvl2Data {
    pub analyses: Vec<Lvl2Analysis>,
    pub ai_count: u64,
    pub human_count: u64,
}

/// Query mesh.db for lvl2_analyses and issue_lvl2_analyses.
/// Classifies items: non-empty auto_actions => AI solvable; else => Human solvable.
pub fn fetch_lvl2_analyses(mesh_db: &str) -> Option<Lvl2Data> {
    let db_file = Path::new(mesh_db);
    if !db_file.exists() {
        return None;
    }

    let conn = Connection::open_with_flags(
        mesh_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;

    let mut analyses = Vec::new();

    // Fetch cluster-level lvl2 analyses (join issue_clusters for severity)
    if let Ok(mut stmt) = conn.prepare(
        "SELECT la.id, COALESCE(la.cluster_name,''), COALESCE(la.strategy_summary,''), \
         COALESCE(la.auto_actions,'[]'), COALESCE(la.output_path,''), \
         COALESCE(la.generated_at,''), COALESCE(la.cluster_id,0), \
         COALESCE(ic.severity,'medium') \
         FROM lvl2_analyses la \
         LEFT JOIN issue_clusters ic ON ic.id = la.cluster_id \
         ORDER BY la.id DESC",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            let id: u64 = row.get(0)?;
            let cluster_name: String = row.get(1)?;
            let strategy_summary: String = row.get(2)?;
            let auto_actions_json: String = row.get(3)?;
            let output_path: String = row.get(4)?;
            let generated_at: String = row.get(5)?;
            let cluster_id: u64 = row.get::<_, i64>(6).unwrap_or(0) as u64;
            let severity_text: String = row.get(7)?;
            Ok((id, cluster_name, strategy_summary, auto_actions_json, output_path, generated_at, cluster_id, severity_text))
        }) {
            for row in rows.flatten() {
                let (id, cluster_name, strategy_summary, auto_actions_json, output_path, generated_at, cluster_id, severity_text) = row;
                let parsed_actions = parse_auto_actions(&auto_actions_json);
                let solvable_by = classify_by_strategy(&strategy_summary);
                let surface = is_surface_only(&parsed_actions);
                let severity = match severity_text.to_lowercase().as_str() {
                    "critical" => 0,
                    "high" => 1,
                    "medium" => 2,
                    _ => 3,
                };
                let display_time = if generated_at.len() >= 16 {
                    generated_at[..16].to_string()
                } else {
                    generated_at
                };
                analyses.push(Lvl2Analysis {
                    id,
                    cluster_id,
                    cluster_name,
                    strategy_summary,
                    auto_actions: parsed_actions,
                    output_path,
                    generated_at: display_time,
                    solvable_by,
                    source: Lvl2Source::Cluster,
                    severity,
                    surface,
                });
            }
        }
    }

    // Fetch issue-level lvl2 analyses (offset IDs to avoid collision)
    let id_offset = analyses.len() as u64 * 10000 + 100000;
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, COALESCE(cluster_name, ''), COALESCE(strategy_summary, ''), \
         COALESCE(auto_actions, '[]'), COALESCE(output_path, ''), \
         COALESCE(generated_at, '') FROM issue_lvl2_analyses ORDER BY id DESC",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            let id: u64 = row.get(0)?;
            let cluster_name: String = row.get(1)?;
            let strategy_summary: String = row.get(2)?;
            let auto_actions_json: String = row.get(3)?;
            let output_path: String = row.get(4)?;
            let generated_at: String = row.get(5)?;
            Ok((id, cluster_name, strategy_summary, auto_actions_json, output_path, generated_at))
        }) {
            for row in rows.flatten() {
                let (id, cluster_name, strategy_summary, auto_actions_json, output_path, generated_at) = row;
                let parsed_actions = parse_auto_actions(&auto_actions_json);
                let solvable_by = classify_by_strategy(&strategy_summary);
                let display_time = if generated_at.len() >= 16 {
                    generated_at[..16].to_string()
                } else {
                    generated_at
                };
                let surface = is_surface_only(&parsed_actions);
                analyses.push(Lvl2Analysis {
                    id: id + id_offset,
                    cluster_id: 0, // issue-level analyses don't have a cluster_id FK
                    cluster_name: format!("[Issue] {}", cluster_name),
                    strategy_summary,
                    auto_actions: parsed_actions,
                    output_path,
                    generated_at: display_time,
                    solvable_by,
                    source: Lvl2Source::Issue,
                    severity: 2, // no cluster join for issue-level; default medium
                    surface,
                });
            }
        }
    }

    let ai_count = analyses.iter().filter(|a| a.solvable_by == SolvableBy::AI).count() as u64;
    let human_count = analyses.iter().filter(|a| a.solvable_by == SolvableBy::Human).count() as u64;

    Some(Lvl2Data {
        analyses,
        ai_count,
        human_count,
    })
}

/// Returns true if ALL auto_actions are surface-only (telegram/airtable/db_update).
/// Surface items need deep EA research before they can be solved.
pub fn is_surface_only(actions: &[String]) -> bool {
    if actions.is_empty() { return true; }
    let surface = ["telegram", "airtable", "db_update", "database_update", "notification", "slack"];
    actions.iter().all(|a| {
        let lower = a.to_lowercase();
        surface.iter().any(|kw| lower.contains(kw))
    })
}

fn classify_by_strategy(summary: &str) -> SolvableBy {
    let s = summary.to_lowercase();

    // Human signals — take priority. If any match → Human.
    let human_signals = [
        "human-required", "human required", "human involvement",
        "manual review", "manually", "requires manual",
        "approval needed", "requires approval", "board approval",
        "board decision", "committee", "ratif",
        "hire ", "hiring", "recruit",
        "clinical", "physician", "patient consent",
        "physical", "on-site", "in-person",
        "meeting", "workshop", "training session",
        "legal review", "compliance officer", "executive decision",
        "policy change", "policy ratification",
    ];

    for signal in &human_signals {
        if s.contains(signal) {
            return SolvableBy::Human;
        }
    }

    // AI signals — only if no human signals matched.
    let ai_signals = [
        "automat",
        "api call", "via api", "api integration", "rest api",
        "script", "scripted",
        "deploy", "deployment",
        "webhook",
        "cron", "scheduled task", "scheduled job",
        "code fix", "code change", "bug fix", "patch",
        "configuration update", "config change", "config update",
        "database update", "db update", "sql query",
        "credential", "api key", "token refresh",
        "pipeline", "data pipeline",
        "monitor", "alert rule", "threshold",
    ];

    for signal in &ai_signals {
        if s.contains(signal) {
            return SolvableBy::AI;
        }
    }

    // Default: conservative — require human unless AI signal found
    SolvableBy::Human
}

/// Parse auto_actions JSON field into descriptive strings.
fn parse_auto_actions(json_str: &str) -> Vec<String> {
    let trimmed = json_str.trim();
    if trimmed.is_empty() || trimmed == "[]" || trimmed == "null" {
        return Vec::new();
    }

    // Try parsing as array of objects with "type" and "payload" fields
    if let Ok(actions) = serde_json::from_str::<Vec<serde_json::Value>>(trimmed) {
        let result: Vec<String> = actions
            .iter()
            .filter_map(|action| {
                let action_type = action.get("type").and_then(|v| v.as_str()).unwrap_or("action");
                let payload = action.get("payload").and_then(|v| v.as_str()).unwrap_or("");
                if payload.is_empty() {
                    None
                } else {
                    // Truncate payload for display
                    let display: String = payload.chars().take(80).collect();
                    Some(format!("[{}] {}", action_type, display))
                }
            })
            .collect();
        if !result.is_empty() {
            return result;
        }
    }

    // Fallback: try as array of strings
    if let Ok(strings) = serde_json::from_str::<Vec<String>>(trimmed) {
        if !strings.is_empty() {
            return strings;
        }
    }

    // If we couldn't parse it but it's not empty/null/[], treat as single action
    vec![trimmed.chars().take(100).collect()]
}

// ──────────────── Cluster Context (for EA dispatch) ────────────────

pub struct ClusterContext {
    pub cluster_name: String,
    pub strategy_summary: String,
    pub issues: Vec<(String, String)>,    // (title, severity)
    pub solutions: Vec<(String, String)>, // (issue_title, solution_summary)
}

pub fn fetch_cluster_context(
    mesh_db: &str,
    research_db: &str,
    cluster_id: u64,
    cluster_name: &str,
    strategy_summary: &str,
) -> ClusterContext {
    // Step 1: Get member_ids using cluster_id (the real FK)
    let member_ids_json = {
        if let Ok(conn) = Connection::open(mesh_db) {
            conn.query_row(
                "SELECT COALESCE(member_ids,'[]') FROM issue_clusters WHERE id=?1 LIMIT 1",
                [cluster_id],
                |r| r.get::<_, String>(0),
            ).unwrap_or_else(|_| "[]".to_string())
        } else {
            "[]".to_string()
        }
    };

    // Step 2: Parse member_ids — manual JSON array parse (no extra dep needed)
    let ids: Vec<u64> = {
        let s = member_ids_json.trim().trim_start_matches('[').trim_end_matches(']');
        s.split(',')
         .filter_map(|tok| tok.trim().parse::<u64>().ok())
         .collect()
    };

    let mut issues = Vec::new();
    let mut solutions = Vec::new();

    if !ids.is_empty() {
        if let Ok(conn) = Connection::open(research_db) {
            for id in &ids {
                if let Ok(mut stmt) = conn.prepare(
                    "SELECT COALESCE(title,''), COALESCE(severity,'medium') FROM issues WHERE id=?1"
                ) {
                    if let Ok((title, severity)) = stmt.query_row([id], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    }) {
                        issues.push((title.clone(), severity));
                        // Fetch top solutions for this issue
                        if let Ok(mut s2) = conn.prepare(
                            "SELECT COALESCE(summary,'') FROM solutions WHERE issue_id=?1 LIMIT 3"
                        ) {
                            if let Ok(rows) = s2.query_map([id], |r| r.get::<_, String>(0)) {
                                for sol in rows.flatten() {
                                    if !sol.is_empty() {
                                        solutions.push((title.clone(), sol));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    ClusterContext {
        cluster_name: cluster_name.to_string(),
        strategy_summary: strategy_summary.to_string(),
        issues,
        solutions,
    }
}

/// Delete solutions containing "No actionable" from research.db,
/// plus their linked issues and cascade records.
/// Returns (issues_deleted, solutions_deleted).
pub fn delete_no_actionable_research(research_db: &str) -> Result<(usize, usize), rusqlite::Error> {
    let conn = Connection::open(research_db)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    let mut stmt = conn.prepare(
        "SELECT id, issue_id FROM solutions WHERE summary LIKE '%No actionable%'"
    )?;
    let pairs: Vec<(i64, i64)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    if pairs.is_empty() { return Ok((0, 0)); }
    let sol_ids: Vec<i64> = pairs.iter().map(|p| p.0).collect();
    let issue_ids: Vec<i64> = pairs.iter().map(|p| p.1).collect();
    for &iid in &issue_ids {
        conn.execute("DELETE FROM actions WHERE issue_id=?1", [iid]).ok();
        conn.execute("DELETE FROM repairs WHERE issue_id=?1", [iid]).ok();
        conn.execute("DELETE FROM daily_output WHERE issue_id=?1", [iid]).ok();
    }
    for &sid in &sol_ids {
        conn.execute("DELETE FROM daily_output WHERE solution_id=?1", [sid]).ok();
        conn.execute("DELETE FROM solutions WHERE id=?1", [sid])?;
    }
    let n_issues = issue_ids.len();
    for &iid in &issue_ids {
        conn.execute("DELETE FROM issues WHERE id=?1", [iid])?;
    }
    Ok((n_issues, sol_ids.len()))
}

/// Delete a solved cluster from mesh.db and its linked issues/solutions from research.db.
/// Called after EA confirms a cluster is SOLVED.
pub fn delete_solved_cluster(
    mesh_db: &str,
    research_db: &str,
    cluster_id: u64,
    member_ids_json: &str,
) -> Result<(), rusqlite::Error> {
    // 1. Remove from mesh.db
    {
        let conn = Connection::open(mesh_db)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute("DELETE FROM lvl2_analyses WHERE cluster_id=?1", [cluster_id as i64])?;
        conn.execute("DELETE FROM issue_clusters WHERE id=?1", [cluster_id as i64])?;
    }

    // 2. Parse member_ids JSON → Vec<i64>
    let ids: Vec<i64> = {
        let s = member_ids_json.trim().trim_start_matches('[').trim_end_matches(']');
        s.split(',')
         .filter_map(|tok| tok.trim().parse::<i64>().ok())
         .collect()
    };

    if ids.is_empty() { return Ok(()); }

    // 3. Delete linked issues and solutions from research.db
    {
        let conn = Connection::open(research_db)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        for &iid in &ids {
            conn.execute("DELETE FROM actions WHERE issue_id=?1", [iid]).ok();
            conn.execute("DELETE FROM repairs WHERE issue_id=?1", [iid]).ok();
            conn.execute("DELETE FROM solutions WHERE issue_id=?1", [iid]).ok();
            conn.execute("DELETE FROM issues WHERE id=?1", [iid]).ok();
        }
    }

    Ok(())
}

/// Delete learnings whose text contains "No actionable" from learnings.db.
pub fn delete_no_actionable_learnings(learnings_db: &str) -> Result<usize, rusqlite::Error> {
    let conn = Connection::open(learnings_db)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    let n = conn.execute("DELETE FROM learnings WHERE learning LIKE '%No actionable%'", [])?;
    Ok(n)
}
