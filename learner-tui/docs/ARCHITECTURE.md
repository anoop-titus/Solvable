# Solvable TUI — Architecture

## Overview

Solvable is a Rust/Ratatui terminal UI for triaging, classifying, and dispatching AI-solvable
issues to an OpenClaw EA agent. It reads from three SQLite databases produced by the learner and
researcher daemons and provides a structured workflow for resolving issues.

---

## Data Sources

| Database | Location | Contents |
|---|---|---|
| `learnings.db` | `$workspace/learner/` | Dropbox/email ingestion runs, raw learnings |
| `research.db` | `$workspace-researcher-agent/db/` | Issues (508), Solutions (2214), Actions |
| `mesh.db` | `$workspace-researcher-agent/db/` | Issue clusters, lvl2 analyses, similarity edges |

Key relationships:
```
mesh.db: lvl2_analyses.cluster_id → issue_clusters.id → member_ids[] → research.db: issues.id
research.db: issues.id → solutions.issue_id
```

---

## Tabs

| Tab | Key | Purpose |
|---|---|---|
| Learnings | 1 | Ingestion run history (Dropbox + Email) |
| Research | 2 | Raw issue/solution browse |
| Portal | 3 | Credentials, OAuth, API key management |
| Issues | 4 | Issue list with severity filter |
| Solutions | 5 | Solution list linked to issues |
| Confluence | 6 | Issue↔Solution cluster mappings |
| Solve | 7 | AI/Human solvable triage + EA dispatch |
| Settings | 8 | App config |

---

## Solve Tab — AI-Human Filter Rules

Items in the AI Solvable list must pass 10 rules before and after dispatch:

1. **EA context**: EA agent can obtain full context (Issue → Cluster → Solution) and execute concretely
2. **Not Airtable**: Solving ≠ logging to Airtable; EA must actually perform work
3. **Sequential**: One item at a time — next item dispatched only after EA confirms completion
4. **Priority order**: Sorted by `issue_clusters.severity` (critical → high → medium → low)
5. **Surface (S)**: If ALL auto_actions are `[telegram, airtable, db_update]` → tagged Surface
6. **Deep research**: Surface items sent to EA as "DEEP RESEARCH TASK" — EA guts issue/cluster/solution depth and reclassifies
7. **Re-list**: If EA re-classifies as `AI_SOLVABLE` → returns to list as Deep (D) for immediate dispatch
8. **DB cleanup**: After solve — delete from `lvl2_analyses`, `issue_clusters`, `solutions`, `issues`
9. **Solved overlay**: Solved items stored with summary; Enter opens mdr overlay
10. **Failure fallback**: EA `FAILED`/`HUMAN_REQUIRED` response or 300-tick timeout → move to Human

### Surface vs Deep

```
(S) item → DEEP RESEARCH TASK → EA appends to ea-responses.jsonl
                                   ↓ AI_SOLVABLE → (D) item
                                   ↓ HUMAN_REQUIRED → Human list

(D) item → SOLVE TASK → EA performs work → appends SOLVED/FAILED
                                   ↓ SOLVED → Solved box + DB deletion
                                   ↓ FAILED → Human list
```

### EA Response Protocol

EA agent appends JSON lines to:
```
/home/typhoon/.openclaw/workspace-researcher-agent/db/ea-responses.jsonl
```

Format:
```json
{"item_id": 42, "decision": "SOLVED|FAILED|AI_SOLVABLE|HUMAN_REQUIRED", "summary": "..."}
```

TUI polls this file every tick (~200ms), processes entries, rewrites file without consumed lines.

---

## Module Structure

```
src/
├── main.rs           — Event loop, key/mouse handlers, EA dispatch, response polling
├── app.rs            — App state, SolveState, SolveItem, SolvedItem, OverlayState
├── ui.rs             — Top-level render, tab bar, overlay renderer
├── theme.rs          — Color constants, styled_block
├── io_layer/
│   ├── db.rs         — All SQLite queries, classify_by_strategy, is_surface_only,
│   │                   delete_solved_cluster, fetch_cluster_context
│   ├── env_store.rs  — .env load/save, resolve_env_path
│   └── oauth.rs      — OAuth2 + device code flows
├── screens/
│   ├── learnings.rs  — Dropbox/email run history
│   ├── research.rs   — Raw issues/solutions
│   ├── portal.rs     — Credentials UI
│   ├── issues.rs     — Issue list + filter
│   ├── solutions.rs  — Solution list
│   ├── confluence.rs — Issue↔Solution mappings
│   ├── solve.rs      — Solve tab: AI/Human/Solved lists, buttons, footer
│   └── settings.rs   — App settings
└── widgets/
    ├── tab_bar.rs    — Tab bar + mouse hit detection
    ├── text_input.rs — Text input widget
    └── tree.rs       — Collapsible tree widget
```

---

## Key Data Structures

### SolveItem
```rust
pub struct SolveItem {
    pub id: u64,
    pub cluster_id: u64,
    pub name: String,
    pub summary: String,          // strategy_summary from lvl2_analyses
    pub actions: Vec<String>,     // parsed auto_actions JSON
    pub member_ids_json: String,  // raw JSON for DB deletion after solve
    pub surface: bool,            // true = all actions are surface-only
    pub severity: u8,             // 0=critical 1=high 2=medium 3=low
    pub dispatched: bool,
    pub dispatch_tick: Option<u64>, // for timeout detection (300 ticks ≈ 60s)
    pub failed: bool,
    // animation state ...
}
```

### SolvedItem
```rust
pub struct SolvedItem {
    pub name: String,
    pub method: String,   // "AI" or "Human"
    pub solved_at: String,
    pub summary: String,  // stored for mdr overlay on Enter
}
```

---

## EA Agent Dispatch

- **Peer ID**: `12D3KooWJtKPNjyKXjLTSmccExB9saN6N8mHuuZEMPa7M7LrDsSk`
- **Topic**: `ea.task`
- **Rate limit**: 1 dispatch per tick (auto-solve path)
- **Transport**: `aqua --dir /home/typhoon/.aqua/main send <peer> --message <msg> --topic ea.task`
- **Stderr**: Suppressed via `Stdio::null()` to prevent TUI corruption

---

## Classification Logic

`classify_by_strategy(summary)` in `db.rs`:
1. Human signals checked first (meeting, approval, hiring, clinical, policy, etc.) → `Human`
2. AI signals checked second (automat, api call, script, deploy, webhook, code fix, etc.) → `AI`
3. Default → `Human` (conservative)

`is_surface_only(actions)`:
- Returns `true` if ALL actions contain only: `telegram`, `airtable`, `db_update`, `notification`, `slack`
- Empty actions → `true` (Surface)

---

## Overlay System

In-TUI markdown viewer (no subprocess):
- 78% terminal width, 85% height, centered
- Shadow (DarkGray block at +2/+1 offset)
- Renders `# H1`, `## H2`, `---` dividers, `**bold**` inline
- Esc/q close, ↑↓/PgUp/PgDn scroll
- Triggered: Enter on AI/Human/Solutions/Solved lists

Last updated: 2026-03-23
