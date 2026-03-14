#!/usr/bin/env python3
"""
Email Learning Agent -- reads emails from Airtable, follows Google Drive links,
extracts learnings via OpenRouter LLM, stores in unified SQLite DB.
"""
import os
import re
import json
import time
import uuid
import subprocess
import argparse

import requests

from learner_config import get_gws_bin, get_airtable_config, get_agent_prompt
from learn_common import (
    load_env, open_db, is_processed, mark_processed, mark_error,
    insert_learnings, extract_learnings_chunked,
    print_table_header, print_table_row, print_table_footer,
)

# ── Config ──────────────────────────────────────────────────────────────────

GWS_BIN = get_gws_bin()

_airtable_cfg = get_airtable_config()
ACCOUNT_MAP = _airtable_cfg["account_map"]

GDRIVE_PATTERNS = [
    r'https://drive\.google\.com/file/d/([a-zA-Z0-9_-]+)',
    r'https://docs\.google\.com/document/d/([a-zA-Z0-9_-]+)',
    r'https://docs\.google\.com/spreadsheets/d/([a-zA-Z0-9_-]+)',
    r'https://drive\.google\.com/open\?id=([a-zA-Z0-9_-]+)',
]

EXPORT_MIMES = {
    "document": True,
    "spreadsheets": True,
}


# ── Database helpers ───────────────────────────────────────────────────────

def create_run(conn, run_id, source, agent, folder):
    conn.execute(
        "INSERT INTO run_progress (run_id, source, agent, folder) VALUES (?, ?, ?, ?)",
        (run_id, source, agent, folder),
    )
    conn.commit()
    return conn.execute("SELECT last_insert_rowid()").fetchone()[0]


def update_run(conn, row_id, total=None, processed=None, skipped=None,
               errors=None, learnings_count=None, status=None):
    sets = ["updated_at = CURRENT_TIMESTAMP"]
    vals = []
    for col, val in [("total_files", total), ("processed", processed),
                     ("skipped", skipped), ("errors", errors),
                     ("learnings_count", learnings_count), ("status", status)]:
        if val is not None:
            sets.append(f"{col} = ?")
            vals.append(val)
    vals.append(row_id)
    conn.execute(f"UPDATE run_progress SET {', '.join(sets)} WHERE id = ?", vals)
    conn.commit()


# ── Airtable ────────────────────────────────────────────────────────────────

def fetch_emails(api_key, base_id, table_id, sender_filter=None):
    """Fetch all email records from Airtable with pagination."""
    url = f"https://api.airtable.com/v0/{base_id}/{table_id}"
    headers = {"Authorization": f"Bearer {api_key}"}
    all_records = []
    params = {"pageSize": 100}

    if sender_filter and len(sender_filter) == 1:
        params["filterByFormula"] = f'account="{sender_filter[0]}"'
    elif sender_filter and len(sender_filter) > 1:
        ors = ",".join(f'account="{s}"' for s in sender_filter)
        params["filterByFormula"] = f"OR({ors})"

    while True:
        r = requests.get(url, headers=headers, params=params, timeout=30)
        r.raise_for_status()
        data = r.json()
        records = data.get("records", [])
        all_records.extend(records)
        offset = data.get("offset")
        if not offset:
            break
        params["offset"] = offset

    return all_records


# ── Google Drive ────────────────────────────────────────────────────────────

def extract_gdrive_ids(text):
    results = []
    seen = set()
    for pattern in GDRIVE_PATTERNS:
        for match in re.finditer(pattern, text):
            file_id = match.group(1)
            if file_id not in seen:
                seen.add(file_id)
                url = match.group(0)
                needs_export = any(k in url for k in ("docs.google.com/document", "docs.google.com/spreadsheets"))
                results.append((file_id, needs_export))
    return results


def fetch_gdrive_content(file_id, needs_export):
    """Fetch Google Drive file content via gws CLI. Returns (content, error)."""
    if not GWS_BIN or not os.path.isfile(GWS_BIN):
        return None, "gws binary not found"

    try:
        if needs_export:
            params = json.dumps({"fileId": file_id, "mimeType": "text/plain"})
            result = subprocess.run(
                [GWS_BIN, "drive", "files", "export", "--params", params],
                capture_output=True, text=True, timeout=60,
            )
        else:
            params = json.dumps({"fileId": file_id, "alt": "media"})
            result = subprocess.run(
                [GWS_BIN, "drive", "files", "get", "--params", params],
                capture_output=True, text=True, timeout=60,
            )

        if result.returncode == 0 and result.stdout.strip():
            output = result.stdout.strip()
            try:
                data = json.loads(output)
                if "bytes" in data and "mimeType" in data:
                    mime = data.get("mimeType", "")
                    if "text" not in mime and "json" not in mime and "xml" not in mime:
                        return None, f"binary file ({mime})"
                if isinstance(data, str):
                    return data, None
            except json.JSONDecodeError:
                return output, None
            return output[:40000], None
        err = result.stderr.strip() or f"exit code {result.returncode}"
        return None, err
    except subprocess.TimeoutExpired:
        return None, "timeout"
    except Exception as e:
        return None, str(e)


# ── Prompt Builder ─────────────────────────────────────────────────────────

def _make_prompt_builder(agent):
    prompt_prefix = get_agent_prompt(agent)

    def builder(chunk_text):
        return f"{prompt_prefix}\n\nContent:\n{chunk_text}"
    return builder


# ── Main ────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Learn from Airtable emails")
    sender_groups = get_airtable_config().get("sender_groups", {})
    sender_choices = list(sender_groups.keys()) + ["all"]
    parser.add_argument(
        "--sender",
        choices=sender_choices if sender_choices else None,
        help="Filter to a single sender group (default: all)",
    )
    args = parser.parse_args()

    if args.sender and args.sender != "all":
        senders = sender_groups.get(args.sender, [])
    else:
        senders = list(ACCOUNT_MAP.keys())

    env = load_env()
    airtable_key = env["AIRTABLE_API_KEY"]
    base_id = env["AIRTABLE_BASE_ID"]
    table_id = env["AIRTABLE_EMAIL_TABLE_ID"]
    openrouter_key = env["OPENROUTER_API_KEY"]

    run_id = str(uuid.uuid4())
    print(f"=== Email Learning Agent ===")
    print(f"Run ID: {run_id}")
    print(f"Senders: {', '.join(senders)}")

    print("\nFetching emails from Airtable...")
    records = fetch_emails(airtable_key, base_id, table_id, sender_filter=senders)
    print(f"Fetched {len(records)} email records")

    if not records:
        print("No emails to process.")
        return

    conn = open_db()

    run_rows = {}
    for account in senders:
        agent = ACCOUNT_MAP.get(account, "unknown")
        row_id = create_run(conn, run_id, "email", agent, account)
        run_rows[account] = {
            "row_id": row_id,
            "total": 0,
            "processed": 0,
            "skipped": 0,
            "errors": 0,
            "learnings": 0,
        }

    for rec in records:
        account = rec.get("fields", {}).get("account", "")
        if account in run_rows:
            run_rows[account]["total"] += 1

    for account_key, info in run_rows.items():
        update_run(conn, info["row_id"], total=info["total"])

    print_table_header(location_label="Sender -- Subject", sticky=True)
    for i, rec in enumerate(records):
        fields = rec.get("fields", {})
        record_id = rec["id"]
        account = fields.get("account", "")
        from_addr = fields.get("from_address", "")
        from_name = fields.get("from_name", "")
        subject = fields.get("subject", "(no subject)")
        body = fields.get("body_preview", "") or ""

        if account not in ACCOUNT_MAP:
            continue
        agent = ACCOUNT_MAP[account]
        info = run_rows[account]
        progress = f"{i+1}/{len(records)}"
        sender_subj = f"{from_addr} -- {subject}"

        if is_processed(conn, record_id):
            info["skipped"] += 1
            update_run(conn, info["row_id"], skipped=info["skipped"])
            continue

        content_parts = []
        if body.strip():
            content_parts.append(f"EMAIL FROM: {from_name} <{from_addr}>\nSUBJECT: {subject}\n\nBODY:\n{body}")

        gdrive_ids = extract_gdrive_ids(body)
        gdrive_learnings_total = 0
        prompt_builder = _make_prompt_builder(agent)

        for file_id, needs_export in gdrive_ids:
            gdrive_key = f"gdrive:{file_id}"
            if is_processed(conn, gdrive_key):
                continue

            gdoc_content, gdoc_err = fetch_gdrive_content(file_id, needs_export)

            if gdoc_err:
                mark_processed(conn, gdrive_key, "gdrive", 0, error=gdoc_err)
                continue

            if gdoc_content:
                content_parts.append(f"\nATTACHED GOOGLE DRIVE DOCUMENT ({file_id}):\n{gdoc_content}")

                gdrive_learnings, gdrive_chunks = extract_learnings_chunked(
                    gdoc_content, prompt_builder, openrouter_key,
                )
                if gdrive_learnings:
                    insert_learnings(conn, gdrive_learnings, "gdrive", agent, account,
                                     file_id, f"gdrive-{file_id[:12]}")
                    gdrive_learnings_total += len(gdrive_learnings)
                    chunks_str = str(gdrive_chunks) if gdrive_chunks > 1 else ""
                    print_table_row(progress, f"gdrive:{file_id[:16]}", "(Google Drive doc)",
                                    chunks_str, str(len(gdrive_learnings)), len(gdoc_content))

                mark_processed(conn, gdrive_key, "gdrive", len(gdrive_learnings),
                               file_size=len(gdoc_content))
                time.sleep(0.5)

        combined_content = "\n\n".join(content_parts)
        content_size = len(combined_content)
        if content_size < 20:
            mark_processed(conn, record_id, "email", 0, error="body too short")
            info["skipped"] += 1
            update_run(conn, info["row_id"], skipped=info["skipped"])
            continue

        # Verbose chunk callback
        def _chunk_cb(chunk_i, chunk_total, chunk_learnings,
                      _progress=progress, _record_id=record_id, _sender_subj=sender_subj, _size=content_size):
            print_table_row(_progress, _record_id[:20], _sender_subj,
                            f"{chunk_i}/{chunk_total}", str(len(chunk_learnings)), _size)

        email_learnings, email_chunks = extract_learnings_chunked(
            combined_content, prompt_builder, openrouter_key, on_chunk=_chunk_cb,
        )

        if email_learnings:
            insert_learnings(conn, email_learnings, "email", agent, account,
                             record_id, subject)
            total_for_record = len(email_learnings) + gdrive_learnings_total
            mark_processed(conn, record_id, "email", total_for_record,
                           file_size=content_size)
            info["processed"] += 1
            info["learnings"] += total_for_record
            if email_chunks == 1:
                print_table_row(progress, record_id[:20], sender_subj,
                                "", str(total_for_record), content_size)
        else:
            mark_processed(conn, record_id, "email", gdrive_learnings_total,
                           error="no learnings extracted" if gdrive_learnings_total == 0 else None)
            info["processed"] += 1
            info["learnings"] += gdrive_learnings_total
            if gdrive_learnings_total == 0:
                info["errors"] += 1
            if email_chunks == 1:
                print_table_row(progress, record_id[:20], sender_subj,
                                "",
                                "ERR" if gdrive_learnings_total == 0 else str(gdrive_learnings_total),
                                content_size)

        update_run(
            conn, info["row_id"],
            processed=info["processed"],
            errors=info["errors"],
            learnings_count=info["learnings"],
        )
        time.sleep(0.5)
    print_table_footer()

    for account_key, info in run_rows.items():
        update_run(conn, info["row_id"], status="completed")

    print(f"\n{'=' * 50}")
    print(f"=== DONE ===")
    total_learnings = 0
    for account_key, info in run_rows.items():
        agent = ACCOUNT_MAP.get(account_key, "unknown")
        print(f"  {agent} ({account_key}):")
        print(f"    Total: {info['total']}, Processed: {info['processed']}, "
              f"Skipped: {info['skipped']}, Errors: {info['errors']}, "
              f"Learnings: {info['learnings']}")
        total_learnings += info["learnings"]
    print(f"  Total learnings: {total_learnings}")

    count = conn.execute("SELECT COUNT(*) FROM learnings WHERE source IN ('email','gdrive')").fetchone()[0]
    print(f"  DB total (email+gdrive): {count}")
    conn.close()


if __name__ == "__main__":
    main()
