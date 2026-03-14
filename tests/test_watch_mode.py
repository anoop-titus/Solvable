"""Tests for IMAP watch mode (continuous email monitoring).

After processing the backlog, the email script should enter watch mode
and update run_progress status to 'watching'.
"""
import sys
import os
import sqlite3

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def test_watch_mode_function_exists():
    """watch_for_new_emails function must exist."""
    from learn_email_imap import watch_for_new_emails
    assert callable(watch_for_new_emails)


def test_watch_sets_status_watching():
    """Watch mode must set status='watching' in run_progress."""
    from learn_email_imap import watch_for_new_emails
    from unittest.mock import MagicMock, patch
    import threading

    # Set up in-memory DB with run_progress table
    conn = sqlite3.connect(":memory:")
    conn.execute("""
        CREATE TABLE run_progress (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            run_id TEXT, source TEXT, agent TEXT, folder TEXT,
            total_files INTEGER DEFAULT 0, processed INTEGER DEFAULT 0,
            skipped INTEGER DEFAULT 0, errors INTEGER DEFAULT 0,
            learnings_count INTEGER DEFAULT 0,
            started_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            status TEXT DEFAULT 'running', pid INTEGER DEFAULT 0
        )
    """)
    conn.execute(
        "INSERT INTO run_progress (run_id, source, agent, folder, status, pid) "
        "VALUES ('test-run', 'imap', 'test-agent', 'account-1', 'running', 1234)"
    )
    conn.commit()
    progress_id = conn.execute("SELECT last_insert_rowid()").fetchone()[0]

    # Mock IMAP that immediately raises to break out of the watch loop
    mock_imap = MagicMock()
    mock_imap.select.return_value = ("OK", [b"1"])
    mock_imap.send.side_effect = ConnectionError("test: break out of watch loop")

    account_info = {
        "email": "user@example.com",
        "agent": "test-agent",
    }
    env = {"OPENROUTER_API_KEY": "test-key"}

    try:
        watch_for_new_emails(mock_imap, "account-1", account_info, conn, env, progress_id)
    except ConnectionError:
        pass

    row = conn.execute(
        "SELECT status FROM run_progress WHERE id = ?", (progress_id,)
    ).fetchone()
    assert row[0] == "watching", f"Expected status='watching', got '{row[0]}'"
    conn.close()
