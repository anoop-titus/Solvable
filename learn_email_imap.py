#!/usr/bin/env python3
"""
Direct IMAP retrospective scraper for email accounts.
Connects to an IMAP server and fetches emails per account with full body text
+ attachments. Extracts learnings via LLM, stores in learnings.db.

Usage:
    python3 learn_email_imap.py                  # All configured accounts
    python3 learn_email_imap.py --account account-1
    python3 learn_email_imap.py --limit 50       # Fewer emails
"""
import argparse
import email
import imaplib
import json
import os
import re
import select
import signal
import socket
import sys
import time
import uuid
from email.header import decode_header
from email.utils import parseaddr

from learner_config import get_imap_config, get_agent_prompt, get_gws_bin
from learn_common import (
    load_env, open_db, is_processed, mark_processed, mark_error,
    insert_learnings, extract_text, extract_learnings_chunked,
    print_table_header, print_table_row, print_table_footer,
)

# ── Config ──────────────────────────────────────────────────────────────────
_imap_cfg = get_imap_config()
IMAP_HOST = _imap_cfg["host"]
IMAP_PORT = _imap_cfg["port"]
IMAP_STARTTLS = _imap_cfg.get("starttls", False)
IMAP_ACCOUNTS = _imap_cfg["accounts"]

# Folders to skip
SKIP_FOLDERS = {"Spam", "Trash", "Sent", "Drafts", "All Mail", "Starred"}

# Spam/ad indicators in From header
SPAM_FROM_PATTERNS = {"newsletter", "marketing", "promo", "unsubscribe",
                      "mailer-daemon", "bounce"}

# Attachment types to extract text from
ATTACHMENT_EXTS = {"pdf", "docx", "doc", "xlsx", "xls", "pptx", "ppt"}

GDRIVE_PATTERNS = [
    r'https://drive\.google\.com/file/d/([a-zA-Z0-9_-]+)',
    r'https://docs\.google\.com/document/d/([a-zA-Z0-9_-]+)',
    r'https://docs\.google\.com/spreadsheets/d/([a-zA-Z0-9_-]+)',
    r'https://drive\.google\.com/open\?id=([a-zA-Z0-9_-]+)',
]

GWS_BIN = get_gws_bin()


# ── Helpers ─────────────────────────────────────────────────────────────────

def decode_header_value(value):
    """Decode RFC 2047 encoded header."""
    if not value:
        return ""
    parts = decode_header(value)
    decoded = []
    for part, charset in parts:
        if isinstance(part, bytes):
            decoded.append(part.decode(charset or "utf-8", errors="replace"))
        else:
            decoded.append(part)
    return " ".join(decoded)


def extract_email_content(msg):
    """Extract body text and attachment texts from email message.

    Returns (body_text, [(filename, extracted_text), ...])
    """
    body = ""
    attachments = []

    parts = msg.walk() if msg.is_multipart() else [msg]
    for part in parts:
        ct = part.get_content_type()
        cd = str(part.get("Content-Disposition", ""))
        filename = part.get_filename()

        # Attachment with known extension
        if filename and "attachment" in cd.lower():
            ext = filename.rsplit(".", 1)[-1].lower() if "." in filename else ""
            if ext in ATTACHMENT_EXTS:
                payload = part.get_payload(decode=True)
                if payload:
                    try:
                        text = extract_text(payload, ext)
                        if text and len(text.strip()) > 20:
                            attachments.append((filename, text))
                    except Exception:
                        pass
            continue

        # Body part
        if ct == "text/plain" and not body:
            payload = part.get_payload(decode=True)
            if payload:
                charset = part.get_content_charset() or "utf-8"
                body = payload.decode(charset, errors="replace")
        elif ct == "text/html" and not body:
            payload = part.get_payload(decode=True)
            if payload:
                charset = part.get_content_charset() or "utf-8"
                html = payload.decode(charset, errors="replace")
                body = re.sub(r"<[^>]+>", " ", html)
                body = re.sub(r"\s+", " ", body).strip()

    return body, attachments


def is_spam_or_ad(msg, from_addr):
    """Check if email is spam/ad based on from address patterns."""
    from_lower = from_addr.lower()
    if any(pattern in from_lower for pattern in SPAM_FROM_PATTERNS):
        return True
    return False


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
    """Fetch Google Drive file content via gws CLI. No size cap."""
    if not GWS_BIN or not os.path.isfile(GWS_BIN):
        return None, "gws binary not found"
    try:
        import subprocess
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
            return result.stdout.strip(), None
        return None, result.stderr.strip() or f"exit code {result.returncode}"
    except Exception as e:
        return None, str(e)


def _make_prompt_builder(agent):
    """Return a callable(chunk_text) -> full prompt string for this agent."""
    prompt_prefix = get_agent_prompt(agent)

    def builder(chunk_text):
        return f"{prompt_prefix}\n\nContent:\n{chunk_text}"
    return builder


# ── IMAP Fetcher ────────────────────────────────────────────────────────────

def fetch_imap_emails(account_key, account_info, env, limit=200):
    """Connect to IMAP and fetch emails with body + attachments."""
    user = env.get(account_info["env_user"], "")
    passwd = env.get(account_info["env_pass"], "")
    if not user or not passwd:
        print(f"  Missing IMAP credentials for {account_key} ({account_info['env_user']}/{account_info['env_pass']})")
        return []

    try:
        imap = imaplib.IMAP4(IMAP_HOST, IMAP_PORT)
        if IMAP_STARTTLS:
            imap.starttls()
        imap.login(user, passwd)
    except Exception as e:
        print(f"  IMAP connection failed for {account_key}: {e}")
        return []

    emails = []
    try:
        _, folder_list = imap.list()
        folders = []
        for f in (folder_list or []):
            if isinstance(f, bytes):
                parts = f.decode("utf-8", errors="replace").split('"')
                if len(parts) >= 3:
                    folder_name = parts[-2] if parts[-1].strip() == "" else parts[-1].strip()
                else:
                    folder_name = f.decode("utf-8", errors="replace").split()[-1]
                folder_name = folder_name.strip('"').strip()
                if folder_name and folder_name not in SKIP_FOLDERS:
                    folders.append(folder_name)

        if not folders:
            folders = ["INBOX"]

        for folder in folders:
            try:
                status, _ = imap.select(folder, readonly=True)
                if status != "OK":
                    continue
            except Exception:
                continue

            _, msg_nums = imap.search(None, "ALL")
            if not msg_nums or not msg_nums[0]:
                continue

            nums = msg_nums[0].split()
            collected = 0

            for num in reversed(nums):  # ALL messages, newest first
                if collected >= limit:
                    break
                try:
                    _, data = imap.fetch(num, "(RFC822)")
                    if data and data[0] and isinstance(data[0], tuple):
                        raw = data[0][1]
                        msg = email.message_from_bytes(raw)

                        from_name, from_addr = parseaddr(msg.get("From", ""))
                        subject = decode_header_value(msg.get("Subject", ""))
                        to_raw = msg.get("To", "")
                        date_str = msg.get("Date", "")

                        if is_spam_or_ad(msg, from_addr):
                            continue  # skip spam, does NOT count toward limit

                        # Filter by To address if account has filter_to
                        filter_to = account_info.get("filter_to")
                        if filter_to and filter_to not in to_raw.lower():
                            continue  # not addressed to this account, skip

                        body, attachments = extract_email_content(msg)
                        if (not body or len(body.strip()) < 20) and not attachments:
                            continue  # skip empty, does NOT count toward limit

                        # Use filter_to for routing, fallback to account email
                        account_email = account_info["email"]

                        collected += 1
                        emails.append({
                            "msg_id": msg.get("Message-ID", num.decode()),
                            "from_name": from_name,
                            "from_addr": from_addr,
                            "to": to_raw,
                            "subject": subject,
                            "body": body[:100000] if body else "",
                            "date": date_str,
                            "account": account_email,
                            "folder": folder,
                            "attachments": attachments,
                        })
                except Exception as e:
                    print(f"    Error fetching msg {num}: {e}")
                    continue

        imap.logout()
    except Exception as e:
        print(f"  IMAP error: {e}")
        try:
            imap.logout()
        except Exception:
            pass

    return emails


# ── IMAP Connection ────────────────────────────────────────────────────────

def connect_imap(account_key, account_info, env):
    """Create and authenticate an IMAP connection. Returns imap object or None."""
    user = env.get(account_info["env_user"], "")
    passwd = env.get(account_info["env_pass"], "")
    if not user or not passwd:
        print(f"  Missing IMAP credentials for {account_key}")
        return None
    try:
        imap = imaplib.IMAP4(IMAP_HOST, IMAP_PORT)
        if IMAP_STARTTLS:
            imap.starttls()
        imap.login(user, passwd)
        return imap
    except Exception as e:
        print(f"  IMAP connection failed for {account_key}: {e}")
        return None


# ── Watch Mode (IMAP IDLE) ────────────────────────────────────────────────

_watching = True  # Global flag for graceful shutdown


def watch_for_new_emails(imap, account_key, account_info, conn, env, progress_id):
    """Enter IMAP IDLE loop, process new emails as they arrive."""
    global _watching
    openrouter_key = env.get("OPENROUTER_API_KEY", "")
    agent = account_info["agent"]
    prompt_builder = _make_prompt_builder(agent)

    # Update status to 'watching'
    conn.execute(
        "UPDATE run_progress SET status='watching', updated_at=CURRENT_TIMESTAMP WHERE id=?",
        (progress_id,),
    )
    conn.commit()

    imap.select("INBOX")
    idle_timeout = 25 * 60  # 25 min (RFC 2177 max is 29 min)

    while _watching:
        # Issue IDLE command via raw protocol
        tag = imap._new_tag().decode()
        imap.send(f"{tag} IDLE\r\n".encode())

        # Read continuation response (+)
        line = imap.readline()
        if not line.startswith(b"+"):
            print(f"  IDLE not accepted: {line}")
            break

        # Wait for server notification or timeout
        new_mail = False
        try:
            sock = imap.socket()
            sock.settimeout(idle_timeout)
            while _watching:
                ready = select.select([sock], [], [], 30)  # 30s check intervals
                if ready[0]:
                    data = imap.readline()
                    if b"EXISTS" in data:
                        new_mail = True
                        break
                    if b"BYE" in data:
                        _watching = False
                        break
                # Heartbeat: update timestamp so TUI knows we're alive
                conn.execute(
                    "UPDATE run_progress SET updated_at=CURRENT_TIMESTAMP WHERE id=?",
                    (progress_id,),
                )
                conn.commit()
        except (socket.timeout, OSError):
            pass  # Timeout -- re-issue IDLE

        # End IDLE
        try:
            imap.send(b"DONE\r\n")
            while True:
                resp = imap.readline()
                if resp.startswith(tag.encode()):
                    break
        except Exception:
            break

        if new_mail and _watching:
            _process_new_mail(imap, account_key, account_info, conn,
                              openrouter_key, prompt_builder, progress_id)
            imap.select("INBOX")


def _process_new_mail(imap, account_key, account_info, conn,
                      openrouter_key, prompt_builder, progress_id):
    """Fetch and process UNSEEN emails after IDLE notification."""
    agent = account_info["agent"]
    try:
        _, msg_nums = imap.search(None, "UNSEEN")
        if not msg_nums or not msg_nums[0]:
            return
        nums = msg_nums[0].split()
    except Exception as e:
        print(f"  Error searching UNSEEN: {e}")
        return

    for num in reversed(nums):  # newest first
        try:
            _, data = imap.fetch(num, "(RFC822)")
            if not (data and data[0] and isinstance(data[0], tuple)):
                continue
            raw = data[0][1]
            msg = email.message_from_bytes(raw)

            from_name, from_addr = parseaddr(msg.get("From", ""))
            subject = decode_header_value(msg.get("Subject", ""))
            to_raw = msg.get("To", "")
            date_str = msg.get("Date", "")
            msg_id = msg.get("Message-ID", num.decode())

            if is_spam_or_ad(msg, from_addr):
                continue

            body, attachments = extract_email_content(msg)
            if (not body or len(body.strip()) < 20) and not attachments:
                continue

            record_key = f"imap:{account_key}:{msg_id}"
            if is_processed(conn, record_key):
                continue

            content_parts = [
                f"EMAIL FROM: {from_name} <{from_addr}>\n"
                f"TO: {to_raw}\nSUBJECT: {subject}\nDATE: {date_str}\n\n"
                f"BODY:\n{body}"
            ]
            for att_name, att_text in attachments:
                content_parts.append(f"\nATTACHMENT ({att_name}):\n{att_text}")

            combined = "\n\n".join(content_parts)
            if len(combined) < 20:
                continue

            account_email = account_info["email"]

            email_learnings, _ = extract_learnings_chunked(
                combined, prompt_builder, openrouter_key,
            )
            if email_learnings:
                insert_learnings(conn, email_learnings, "imap", agent,
                                 account_email, record_key, subject)
                mark_processed(conn, record_key, "imap", len(email_learnings),
                               file_size=len(combined))
                print(f"  NEW: {from_addr} -- {subject} -> {len(email_learnings)} learnings")
            else:
                mark_processed(conn, record_key, "imap", 0, error="no learnings extracted")

            # Update progress
            conn.execute(
                "UPDATE run_progress SET learnings_count=learnings_count+?, "
                "processed=processed+1, updated_at=CURRENT_TIMESTAMP WHERE id=?",
                (len(email_learnings), progress_id),
            )
            conn.commit()
            time.sleep(0.5)
        except Exception as e:
            print(f"    Error processing new msg {num}: {e}")
            continue


# ── Main ────────────────────────────────────────────────────────────────────

def main():
    global _watching

    parser = argparse.ArgumentParser(description="IMAP email scraper + watcher")
    parser.add_argument("--account", choices=list(IMAP_ACCOUNTS.keys()),
                        help="Specific account to scrape (default: all)")
    parser.add_argument("--limit", type=int, default=200,
                        help="Max emails per account (default: 200)")
    parser.add_argument("--no-watch", action="store_true",
                        help="Exit after backlog processing (no watch mode)")
    args = parser.parse_args()

    def handle_stop(signum, frame):
        global _watching
        _watching = False
        print("\n  Received stop signal, shutting down...")

    signal.signal(signal.SIGTERM, handle_stop)
    signal.signal(signal.SIGINT, handle_stop)

    env = load_env()
    openrouter_key = env.get("OPENROUTER_API_KEY", "")
    if not openrouter_key:
        print("ERROR: OPENROUTER_API_KEY not found in .env")
        sys.exit(1)

    accounts = {args.account: IMAP_ACCOUNTS[args.account]} if args.account else IMAP_ACCOUNTS
    conn = open_db()
    run_id = str(uuid.uuid4())

    print("=== IMAP Email Scraper + Watcher ===")
    print(f"Run ID: {run_id}")
    print(f"Accounts: {', '.join(accounts.keys())}")
    print(f"Limit: {args.limit} emails per account")
    print(f"Watch mode: {'off' if args.no_watch else 'on'}\n")

    total_learnings = 0
    last_progress_id = None
    last_account_key = None
    last_account_info = None

    for account_key, account_info in accounts.items():
        agent = account_info["agent"]
        print(f"\n--- {account_key} ({account_info['email']}) -> {agent} ---")

        # Phase 1: Backlog processing
        emails = fetch_imap_emails(account_key, account_info, env, limit=args.limit)
        print(f"  Fetched {len(emails)} emails (after spam filtering)")

        # Create run progress
        conn.execute(
            "INSERT INTO run_progress (run_id, source, agent, folder, total_files, pid, status) "
            "VALUES (?, 'imap', ?, ?, ?, ?, 'running')",
            (run_id, agent, account_key, len(emails) if emails else 0, os.getpid()),
        )
        conn.commit()
        progress_id = conn.execute("SELECT last_insert_rowid()").fetchone()[0]
        last_progress_id = progress_id
        last_account_key = account_key
        last_account_info = account_info

        if not emails:
            continue

        processed = 0
        skipped = 0
        errors = 0
        learnings_count = 0

        prompt_builder = _make_prompt_builder(agent)

        print_table_header(location_label="Sender -- Subject", sticky=True)
        for i, em in enumerate(emails):
            if not _watching and not args.no_watch:
                break

            record_key = f"imap:{account_key}:{em['msg_id']}"
            progress = f"{i+1}/{len(emails)}"
            sender_subj = f"{em['from_addr']} -- {em['subject']}"

            if is_processed(conn, record_key):
                skipped += 1
                continue

            # Build content
            content_parts = [
                f"EMAIL FROM: {em['from_name']} <{em['from_addr']}>\n"
                f"TO: {em['to']}\nSUBJECT: {em['subject']}\nDATE: {em['date']}\n\n"
                f"BODY:\n{em['body']}"
            ]

            for att_name, att_text in em.get("attachments", []):
                content_parts.append(f"\nATTACHMENT ({att_name}):\n{att_text}")

            # Check for GDrive links
            gdrive_ids = extract_gdrive_ids(em["body"])
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
                    gdrive_learnings, _ = extract_learnings_chunked(
                        gdoc_content, prompt_builder, openrouter_key,
                    )
                    if gdrive_learnings:
                        insert_learnings(conn, gdrive_learnings, "gdrive", agent,
                                         em["account"], file_id, f"gdrive-{file_id[:12]}")
                        learnings_count += len(gdrive_learnings)
                    mark_processed(conn, gdrive_key, "gdrive", len(gdrive_learnings),
                                   file_size=len(gdoc_content))
                    time.sleep(0.5)

            combined = "\n\n".join(content_parts)
            content_size = len(combined)
            if content_size < 20:
                mark_processed(conn, record_key, "imap", 0, error="body too short")
                skipped += 1
                continue

            def _chunk_cb(chunk_i, chunk_total, chunk_learnings,
                          _progress=progress, _sender_subj=sender_subj, _size=content_size):
                print_table_row(_progress, record_key[:20], _sender_subj,
                                f"{chunk_i}/{chunk_total}", str(len(chunk_learnings)), _size)

            email_learnings, num_chunks = extract_learnings_chunked(
                combined, prompt_builder, openrouter_key, on_chunk=_chunk_cb,
            )

            if email_learnings:
                insert_learnings(conn, email_learnings, "imap", agent,
                                 em["account"], record_key, em["subject"])
                mark_processed(conn, record_key, "imap", len(email_learnings),
                               file_size=content_size)
                processed += 1
                learnings_count += len(email_learnings)
                if num_chunks == 1:
                    print_table_row(progress, record_key[:20], sender_subj,
                                    "", str(len(email_learnings)), content_size)
            else:
                mark_processed(conn, record_key, "imap", 0, error="no learnings extracted")
                errors += 1
                if num_chunks == 1:
                    print_table_row(progress, record_key[:20], sender_subj,
                                    "", "ERR", content_size)

            conn.execute(
                "UPDATE run_progress SET processed=?, skipped=?, errors=?, learnings_count=?, "
                "updated_at=CURRENT_TIMESTAMP WHERE id=?",
                (processed, skipped, errors, learnings_count, progress_id),
            )
            conn.commit()
            time.sleep(0.5)

        print_table_footer()

        print(f"\n  {account_key}: processed={processed}, skipped={skipped}, errors={errors}, learnings={learnings_count}")
        total_learnings += learnings_count

    print(f"\n=== Backlog done: {total_learnings} total new learnings ===")

    # Phase 2: Watch mode
    if not args.no_watch and _watching and last_account_key:
        print(f"\n=== Entering watch mode for {last_account_key} ===")
        imap = connect_imap(last_account_key, last_account_info, env)
        if imap:
            try:
                watch_for_new_emails(imap, last_account_key, last_account_info,
                                     conn, env, last_progress_id)
            except Exception as e:
                print(f"  Watch mode error: {e}")
            finally:
                try:
                    imap.logout()
                except Exception:
                    pass
                conn.execute(
                    "UPDATE run_progress SET status='completed', updated_at=CURRENT_TIMESTAMP WHERE id=?",
                    (last_progress_id,),
                )
                conn.commit()
        else:
            print("  Could not reconnect for watch mode")
    else:
        if last_progress_id:
            conn.execute(
                "UPDATE run_progress SET status='completed', updated_at=CURRENT_TIMESTAMP WHERE id=?",
                (last_progress_id,),
            )
            conn.commit()

    conn.close()


if __name__ == "__main__":
    main()
