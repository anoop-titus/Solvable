#!/usr/bin/env python3
"""
Dropbox Learning — Complete folder learning with LLM extraction.
Reads all text files from Dropbox folders, extracts insights via LLM,
stores individual learnings in unified SQLite DB. No raw content stored.
"""
import argparse
import json
import os
import sqlite3
import sys
import time
import uuid

import requests

from learner_config import get_db_path, get_model, get_dropbox_folders, get_agent_prompt
from learn_common import (
    load_env, open_db as _open_db, is_processed, mark_processed, mark_error,
    insert_learnings, extract_text, extract_learnings_chunked,
    print_table_header, print_table_row, print_table_footer,
)

# --- Config ---
DB_PATH = get_db_path()
MODEL = get_model()
CHUNK_SIZE = 100_000       # chars per LLM chunk (~100K)
MAX_FILE_SIZE = 100_000_000  # skip files > 100MB

FOLDERS = get_dropbox_folders()

TEXT_EXTS = {
    "txt", "md", "csv", "json", "py", "js", "ts", "tsx", "jsx", "html", "xml",
    "yaml", "yml", "sql", "sh", "log", "tsv", "conf", "config", "toml", "ini",
    "cfg", "rtf", "env", "gitignore", "dockerfile", "makefile",
}

DOC_EXTS = {"pdf", "docx", "xlsx", "pptx", "doc", "xls", "ppt", "odt", "ods", "odp"}

# Only skip video and audio -- everything else gets attempted
SKIP_EXTS = {
    # Video
    "mp4", "avi", "mov", "mkv", "wmv", "flv", "webm", "m4v", "mpg", "mpeg", "3gp",
    # Audio
    "mp3", "wav", "flac", "aac", "ogg", "wma", "m4a", "opus", "aiff",
}

SKIP_DIRS = {
    "node_modules", ".git", "venv", ".venv", "site-packages", "__pycache__",
    "Pods", ".next", "build", ".cache", ".bin", "bower_components", "vendor",
    ".expo", ".gradle", "DerivedData", "dist", "dist-info",
}


def open_db():
    """Open unified learnings DB and mark stale runs as crashed."""
    conn = _open_db()
    conn.execute(
        "UPDATE run_progress SET status = 'crashed', updated_at = CURRENT_TIMESTAMP "
        "WHERE status = 'running'"
    )
    conn.commit()
    return conn


def get_token(env):
    """Get fresh Dropbox access token via refresh token."""
    r = requests.post(
        "https://api.dropboxapi.com/oauth2/token",
        data={
            "grant_type": "refresh_token",
            "refresh_token": env.get("DROPBOX_REFRESH_TOKEN"),
            "client_id": env.get("DROPBOX_APP_KEY"),
            "client_secret": env.get("DROPBOX_APP_SECRET"),
        },
        timeout=30,
    )
    r.raise_for_status()
    return r.json()["access_token"]


def list_files_paginated(token, path):
    """List ALL files in a Dropbox folder with full pagination."""
    headers = {"Authorization": f"Bearer {token}", "Content-Type": "application/json"}
    all_files = []

    r = requests.post(
        "https://api.dropboxapi.com/2/files/list_folder",
        headers=headers,
        json={"path": path, "recursive": True, "limit": 2000},
        timeout=60,
    )
    if r.status_code != 200:
        print(f"    ERROR listing {path}: {r.status_code} {r.text[:200]}")
        return []

    data = r.json()
    all_files.extend(e for e in data.get("entries", []) if e.get(".tag") == "file")

    while data.get("has_more"):
        cursor = data["cursor"]
        r = requests.post(
            "https://api.dropboxapi.com/2/files/list_folder/continue",
            headers=headers,
            json={"cursor": cursor},
            timeout=60,
        )
        if r.status_code != 200:
            print(f"    ERROR continuing pagination: {r.status_code}")
            break
        data = r.json()
        all_files.extend(e for e in data.get("entries", []) if e.get(".tag") == "file")

    return all_files


def should_skip_path(path_lower):
    """Skip dev artifact directories."""
    parts = path_lower.split("/")
    return any(part in SKIP_DIRS for part in parts)


def get_ext(filename):
    """Extract file extension, lowercase."""
    if "." in filename:
        return filename.rsplit(".", 1)[-1].lower()
    return ""


def download_file(token, path):
    """Download file from Dropbox, return raw bytes or None."""
    try:
        r = requests.post(
            "https://content.dropboxapi.com/2/files/download",
            headers={
                "Authorization": f"Bearer {token}",
                "Dropbox-API-Arg": json.dumps({"path": path}),
            },
            timeout=120,
        )
        if r.status_code == 200:
            return r.content
    except Exception as e:
        print(f"    Download error for {path}: {e}")
    return None


def store_learnings(conn, source, agent, folder, file_path, file_name, learnings, file_size=0):
    """Insert individual learnings and mark file as processed."""
    insert_learnings(conn, learnings, source, agent, folder, file_path, file_name)
    mark_processed(conn, file_path, source, len(learnings), file_size=file_size)


def update_progress(conn, progress_id, **kwargs):
    """Update run_progress row."""
    sets = ", ".join(f"{k} = ?" for k in kwargs)
    vals = list(kwargs.values())
    vals.append(progress_id)
    conn.execute(
        f"UPDATE run_progress SET {sets}, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
        vals,
    )
    conn.commit()


def _make_prompt_builder(folder, agent):
    """Return a callable(chunk_text) -> full prompt string for this folder/agent."""
    context = get_agent_prompt(agent)

    def builder(chunk_text):
        return f"""{context}

Below is content from a file in Dropbox folder: {folder}

Extract 3-8 specific, actionable insights. Include names, dates, amounts, terms where present.
Return ONLY a JSON array: ["insight 1", "insight 2", ...]

---
{chunk_text}"""
    return builder


def process_folder(token, folder_info, conn, env):
    """Process all text files in a Dropbox folder."""
    agent = folder_info["agent"]
    folder = folder_info["path"]
    run_id = str(uuid.uuid4())
    openrouter_key = env.get("OPENROUTER_API_KEY", "")

    print(f"\n{'='*60}")
    print(f"  Agent: {agent}")
    print(f"  Folder: {folder}")
    print(f"{'='*60}")

    print("  Listing files from Dropbox (this may take a minute)...")
    all_files = list_files_paginated(token, folder)
    if not all_files:
        print("  No files found.")
        return 0
    print(f"  API returned {len(all_files)} files")

    # Filter: skip dev artifacts and known binary formats
    supported_files = []
    for f in all_files:
        path = f.get("path_lower", "")
        if should_skip_path(path):
            continue
        ext = get_ext(f.get("name", ""))
        if ext not in SKIP_EXTS:
            supported_files.append(f)

    already_done = sum(1 for f in supported_files if is_processed(conn, f.get("path_lower", "")))
    remaining = len(supported_files) - already_done
    print(f"  Total files: {len(all_files)}, supported: {len(supported_files)}, already done: {already_done}, remaining: {remaining}")

    if remaining == 0:
        print("  All supported files already processed. Nothing to do.")
        return 0

    # Priority: business docs first, source code last
    DIR_PRIORITY = [
        ("tax/", 0), ("tax_", 0), ("/contracts/", 0), ("/banking/", 0), ("/investment/", 0),
        ("/operations/", 0), ("/legal/", 0), ("/finance/", 0), ("/insurance/", 0),
        ("/compliance/", 0), ("/accounting/", 0), ("/invoices/", 0), ("/irs/", 0),
        ("/admin/", 1), ("/hr/", 1), ("/marketing/", 1), ("/partnerships/", 1),
        ("/investors/", 1), ("/reports/", 1), ("/proposals/", 1),
        ("/medical/", 2), ("/r&d/", 2), ("/clinical/", 2), ("/research/", 2),
        ("/ongoing projects/", 2), ("/patents/", 2),
        ("/ios/", 9), ("/_codebase/", 9), ("/git/", 9), ("/tech/", 9),
        ("/appdev/", 9), ("/firebase/", 9), ("/website/", 9), ("/forums/", 9),
    ]

    def _file_priority(f):
        path = f.get("path_lower", "").lower()
        ext = get_ext(f.get("name", ""))
        for pattern, pri in DIR_PRIORITY:
            if pattern in path:
                return pri
        if ext in DOC_EXTS:
            return 3
        if ext in TEXT_EXTS:
            return 5
        return 4

    supported_files.sort(key=_file_priority)

    # Create run_progress entry
    conn.execute(
        "INSERT INTO run_progress (run_id, source, agent, folder, total_files, status) "
        "VALUES (?, 'dropbox', ?, ?, ?, 'running')",
        (run_id, agent, folder, len(supported_files)),
    )
    conn.commit()
    progress_id = conn.execute("SELECT last_insert_rowid()").fetchone()[0]

    processed = 0
    skipped = 0
    errors = 0
    learnings_total = 0
    consecutive_failures = 0
    MAX_CONSECUTIVE_FAILURES = 5

    prompt_builder = _make_prompt_builder(folder, agent)

    print_table_header(sticky=True)
    try:
        for i, f in enumerate(supported_files):
            path = f["path_lower"]
            name = f.get("name", os.path.basename(path))
            size = f.get("size", 0)
            location = os.path.dirname(path)
            progress = f"{i+1}/{len(supported_files)}"

            if is_processed(conn, path):
                skipped += 1
                continue

            if size > MAX_FILE_SIZE:
                mark_error(conn, path, "dropbox", f"skipped: {size} bytes > {MAX_FILE_SIZE}")
                skipped += 1
                continue

            raw = download_file(token, path)
            if not raw:
                mark_error(conn, path, "dropbox", "download failed")
                skipped += 1
                continue

            ext = get_ext(name)
            try:
                content = extract_text(raw, ext)
            except Exception as e:
                mark_error(conn, path, "dropbox", f"extraction error: {e}")
                skipped += 1
                continue
            raw = None  # free memory

            if not content or len(content) < 20:
                mark_error(conn, path, "dropbox", "empty or too short after extraction")
                skipped += 1
                continue

            # Verbose chunk callback
            def _chunk_cb(chunk_i, chunk_total, chunk_learnings,
                          _progress=progress, _name=name, _location=location, _size=size):
                print_table_row(_progress, _name, _location,
                                f"{chunk_i}/{chunk_total}", str(len(chunk_learnings)), _size)

            learnings, num_chunks = extract_learnings_chunked(
                content, prompt_builder, openrouter_key, on_chunk=_chunk_cb,
            )

            if learnings:
                store_learnings(conn, "dropbox", agent, folder, path, name, learnings, file_size=size)
                consecutive_failures = 0
                learnings_total += len(learnings)
                processed += 1
                if num_chunks == 1:
                    print_table_row(progress, name, location, "", str(len(learnings)), size)
            else:
                mark_error(conn, path, "dropbox", "LLM: no learnings extracted")
                errors += 1
                consecutive_failures += 1
                if num_chunks == 1:
                    print_table_row(progress, name, location, "", "ERR", size)

            # Circuit breaker
            if consecutive_failures >= MAX_CONSECUTIVE_FAILURES:
                print_table_footer()
                print(f"\n  CIRCUIT BREAKER: {consecutive_failures} consecutive failures.")
                print(f"  Pausing 5 minutes before retrying...")
                time.sleep(300)
                consecutive_failures = 0
                print_table_header(sticky=True)

            update_progress(
                conn, progress_id,
                processed=processed, skipped=skipped, errors=errors,
                learnings_count=learnings_total,
            )
            time.sleep(0.5)

        final_status = "completed"
    except (KeyboardInterrupt, SystemExit):
        final_status = "interrupted"
        print("\n  Interrupted! Progress saved -- will resume on next run.")
    except Exception as e:
        final_status = "crashed"
        print(f"\n  ERROR: {e}")
    print_table_footer()

    update_progress(
        conn, progress_id,
        processed=processed, skipped=skipped, errors=errors,
        learnings_count=learnings_total, status=final_status,
    )

    print(f"\n  Results: {processed} processed, {skipped} skipped, {errors} errors")
    print(f"  Learnings extracted: {learnings_total}")
    return learnings_total


def main():
    parser = argparse.ArgumentParser(description="Dropbox Learning")
    folder_choices = [f["agent"] for f in FOLDERS] + ["all"] if FOLDERS else ["all"]
    parser.add_argument(
        "--folder",
        default="all",
        help="Which folder to process by agent name (default: all)",
    )
    args = parser.parse_args()

    print("=== Dropbox Learning ===")
    print(f"Database: {DB_PATH}")
    print(f"Model: {MODEL}")

    env = load_env()
    token = get_token(env)
    conn = open_db()

    if args.folder == "all":
        folders_to_process = FOLDERS
    else:
        folders_to_process = [f for f in FOLDERS if f["agent"] == args.folder]
        if not folders_to_process:
            print(f"No folder configured for agent '{args.folder}'")
            sys.exit(1)

    grand_total = 0
    try:
        for folder_info in folders_to_process:
            grand_total += process_folder(token, folder_info, conn, env)
    except KeyboardInterrupt:
        print("\n\nShutting down gracefully. All progress saved.")

    print(f"\n{'='*60}")
    print(f"  COMPLETE: {grand_total} new learnings extracted")
    for folder_info in FOLDERS:
        count = conn.execute(
            "SELECT COUNT(*) FROM learnings WHERE agent = ?",
            (folder_info["agent"],),
        ).fetchone()[0]
        print(f"  {folder_info['agent']}: {count} total learnings")
    print(f"{'='*60}")

    conn.close()


if __name__ == "__main__":
    main()
