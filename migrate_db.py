#!/usr/bin/env python3
"""
One-time migration: dropbox-learnings.db -> unified learnings.db
Explodes JSON arrays into individual learning rows. No content storage.
"""
import os
import json
import sqlite3

DB_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "db")
OLD_DB = os.path.join(DB_DIR, "dropbox-learnings.db")
NEW_DB = os.path.join(DB_DIR, "learnings.db")

SCHEMA = """
CREATE TABLE IF NOT EXISTS learnings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    agent TEXT NOT NULL,
    folder TEXT,
    file_path TEXT NOT NULL,
    file_name TEXT,
    learning TEXT NOT NULL,
    processed_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS processed_files (
    file_path TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    file_size INTEGER,
    processed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    learning_count INTEGER DEFAULT 0,
    error TEXT
);

CREATE TABLE IF NOT EXISTS run_progress (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id TEXT NOT NULL,
    source TEXT NOT NULL,
    agent TEXT,
    folder TEXT,
    total_files INTEGER DEFAULT 0,
    processed INTEGER DEFAULT 0,
    skipped INTEGER DEFAULT 0,
    errors INTEGER DEFAULT 0,
    learnings_count INTEGER DEFAULT 0,
    started_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    status TEXT DEFAULT 'running'
);

CREATE INDEX IF NOT EXISTS idx_learnings_source ON learnings(source);
CREATE INDEX IF NOT EXISTS idx_learnings_agent ON learnings(agent);
CREATE INDEX IF NOT EXISTS idx_processed_source ON processed_files(source);
CREATE INDEX IF NOT EXISTS idx_run_progress_status ON run_progress(status);
"""

# Example folder-to-agent mapping. Customize for your data.
FOLDER_AGENT_MAP = {
    "/Documents/Project-A": "my-agent",
}


def migrate():
    os.makedirs(DB_DIR, exist_ok=True)

    # Create new DB with schema
    new = sqlite3.connect(NEW_DB)
    new.executescript(SCHEMA)

    if not os.path.exists(OLD_DB) or os.path.getsize(OLD_DB) == 0:
        print("No old data to migrate. Fresh DB created.")
        new.close()
        return

    old = sqlite3.connect(OLD_DB)
    rows = old.execute(
        "SELECT folder, file_path, learnings, processed_at FROM dropbox_learnings"
    ).fetchall()

    migrated_learnings = 0
    migrated_files = 0

    for folder, file_path, learnings_json, ts in rows:
        agent = FOLDER_AGENT_MAP.get(folder, "unknown")
        file_name = os.path.basename(file_path) if file_path else None

        try:
            learnings = json.loads(learnings_json) if learnings_json else []
        except json.JSONDecodeError:
            learnings = []

        for learning in learnings:
            if learning and learning.strip():
                new.execute(
                    "INSERT INTO learnings (source, agent, folder, file_path, file_name, learning, processed_at) "
                    "VALUES (?, ?, ?, ?, ?, ?, ?)",
                    ("dropbox", agent, folder, file_path, file_name, learning.strip(), ts),
                )
                migrated_learnings += 1

        new.execute(
            "INSERT OR IGNORE INTO processed_files (file_path, source, processed_at, learning_count) "
            "VALUES (?, ?, ?, ?)",
            (file_path, "dropbox", ts, len(learnings)),
        )
        migrated_files += 1

    new.commit()
    old.close()
    new.close()

    print(f"Migration complete: {migrated_files} files, {migrated_learnings} learnings")
    print(f"New DB: {NEW_DB}")


if __name__ == "__main__":
    migrate()
