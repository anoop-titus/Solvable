# Learner Agent

Multi-source AI learning extraction system. Ingests documents from Dropbox, IMAP email, and Airtable, extracts structured insights via LLM (OpenRouter), and stores them in a local SQLite database for agent recall.

## Architecture

```
  Data Sources                  Processing              Storage & Access
  ============                  ==========              ================

  +----------+
  | Dropbox  |--+
  +----------+  |
                |    +------------+    +----------+    +-------------+
  +----------+  +--->| LLM        |--->| SQLite   |--->| Recall CLI  |
  | IMAP     |--+--->| Extraction |    | DB       |    | (recall.py) |
  +----------+  |    | (OpenRouter|    +----------+    +-------------+
                |    +------------+         |
  +----------+  |                           |          +-------------+
  | Airtable |--+                           +--------->| Ratatui TUI |
  +----------+                                         | (dashboard) |
                                                       +-------------+
```

## Quick Start

```bash
git clone <repo-url> learner-agent
cd learner-agent
./setup.sh           # installs deps, creates config files, builds TUI
# Edit .env with your API keys
# Edit config.yaml with your agents and data sources
./learn --once       # single pass through all sources
```

## Configuration

### config.yaml

The main configuration file. Copy from `config.example.yaml`:

| Section | Description |
|---------|-------------|
| `model` | OpenRouter model identifier for LLM extraction |
| `chunk_size` | Max chars per LLM chunk (default: 100000) |
| `imap` | IMAP server settings + account definitions |
| `airtable` | Account-to-agent mapping for Airtable email source |
| `dropbox.folders` | List of Dropbox folder paths mapped to agents |
| `agents` | Per-agent system prompts for LLM extraction |
| `gws_bin` | Path to Google Workspace CLI (optional) |
| `research_script` | Path to companion research daemon (optional) |

### .env

API keys and credentials (shell export format):

| Variable | Required | Description |
|----------|----------|-------------|
| `OPENROUTER_API_KEY` | Yes | OpenRouter API key for LLM calls |
| `DROPBOX_REFRESH_TOKEN` | For Dropbox | OAuth2 refresh token |
| `DROPBOX_APP_KEY` | For Dropbox | Dropbox app key |
| `DROPBOX_APP_SECRET` | For Dropbox | Dropbox app secret |
| `AIRTABLE_API_KEY` | For Airtable | Airtable personal access token |
| `AIRTABLE_BASE_ID` | For Airtable | Airtable base ID |
| `AIRTABLE_EMAIL_TABLE_ID` | For Airtable | Table ID containing email records |
| `IMAP_USERNAME` | For IMAP | IMAP login username |
| `IMAP_PASSWORD` | For IMAP | IMAP login password |

## Data Sources

### Dropbox

Recursively lists and downloads files from configured Dropbox folders. Supports text files, PDFs, DOCX, XLSX, PPTX. Skips video/audio binaries and dev artifact directories (node_modules, .git, etc.). Files are prioritized: business documents first, source code last.

### IMAP Email

Connects directly to an IMAP server, fetches emails with full body text and attachments. Includes spam filtering, per-account `filter_to` routing (for shared mailboxes), and IMAP IDLE watch mode for real-time processing of new emails.

### Airtable

Reads email records from an Airtable base (populated by an external ingestion workflow). Follows Google Drive links found in email bodies and extracts content via the Google Workspace CLI.

## CLI Reference

```bash
# Main orchestrator
./learn                    # Start daemon (sequential, loops forever)
./learn --once             # Single pass through all sources, then exit
./learn -p                 # Parallel mode (all sources concurrently)
./learn -p --dropbox       # Parallel Dropbox only
./learn -p --email         # Parallel email only
./learn -p -a              # Aggressive: 3x process multiplier per Dropbox folder
./learn --stop             # Stop all learn processes
./learn --stop RUN_ID      # Stop a specific run by ID
./learn --status           # Show running processes
./learn --jobs             # List active jobs from DB
./learn --tui              # Open Ratatui TUI dashboard
./learn --research         # Include research daemon alongside learning

# Individual scrapers
python3 learn_dropbox.py --folder all           # Process all Dropbox folders
python3 learn_dropbox.py --folder my-agent      # Process one agent's folder
python3 learn_email.py --sender all             # All Airtable email accounts
python3 learn_email.py --sender my-group        # One sender group
python3 learn_email_imap.py                     # All IMAP accounts
python3 learn_email_imap.py --account account-1 # Specific account
python3 learn_email_imap.py --limit 50          # Limit emails fetched
python3 learn_email_imap.py --no-watch          # No watch mode after backlog

# Recall
python3 recall.py --agent my-agent --query "budget forecast"
python3 recall.py --agent my-agent --all --limit 50
python3 recall.py --agent my-agent --stats
python3 recall.py --agent my-agent --query "contracts" --json
```

## TUI Dashboard

The Ratatui-based terminal dashboard provides:

- Real-time run progress with per-agent stats
- Learning counts by source (dropbox, email, imap, gdrive)
- Active/watching/completed run status
- IMAP watch mode heartbeat indicator
- Research DB integration (optional second DB)

**Keybindings:**

| Key | Action |
|-----|--------|
| `q` / `Ctrl+C` | Quit |
| `r` | Force refresh |
| `Tab` / `1-4` | Switch tabs |
| `j/k` or arrows | Scroll lists |
| `w` | Toggle watch/radar mode |

Build: `cd learner-tui && cargo build --release`

## Recall API

`recall.py` provides read-only keyword search over the learnings database:

```python
# Programmatic usage
from recall import recall_learnings, recall_stats
results = recall_learnings("my-agent", "deployment strategy", limit=10)
stats = recall_stats("my-agent")
```

Keyword matching uses case-insensitive LIKE queries with automatic stopword removal. Results are ranked by keyword hit count.

## Agent Prompts

Each agent has a configurable system prompt in `config.yaml` under `agents.<name>.prompt`. The prompt tells the LLM what domain knowledge to focus on when extracting learnings. Example:

```yaml
agents:
  finance-agent:
    prompt: >
      You are a learning agent for a finance team.
      Focus on: transaction details, account numbers, payment terms,
      vendor relationships, budget allocations.
      Extract 3-8 key learnings as a JSON array of strings.
      Always respond in English.
```

## Database Schema

Three tables in `db/learnings.db`:

**learnings** -- Individual extracted insights
| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Auto-increment |
| source | TEXT | dropbox, email, imap, gdrive |
| agent | TEXT | Agent name |
| folder | TEXT | Source folder/account |
| file_path | TEXT | Original file path or record ID |
| file_name | TEXT | Display name |
| learning | TEXT | Extracted insight text |
| processed_at | DATETIME | Timestamp |

**processed_files** -- Deduplication tracking
| Column | Type | Description |
|--------|------|-------------|
| file_path | TEXT PK | Unique identifier |
| source | TEXT | Source type |
| file_size | INTEGER | Bytes |
| learning_count | INTEGER | Insights extracted |
| error | TEXT | Error message if failed |

**run_progress** -- Run tracking for TUI/status
| Column | Type | Description |
|--------|------|-------------|
| id | INTEGER PK | Auto-increment |
| run_id | TEXT | UUID per run |
| source | TEXT | Source type |
| agent | TEXT | Agent name |
| total_files | INTEGER | Files to process |
| processed | INTEGER | Files done |
| status | TEXT | running, watching, completed, crashed |

## Tests

```bash
python3 -m pytest tests/ -v
```

Tests cover:
- Email fetch ordering (newest-first)
- Spam filtering (financial emails pass, actual spam blocked)
- IMAP credential configuration validation
- Parallel task generation (correct process counts)
- Watch mode status transitions

## License

MIT
