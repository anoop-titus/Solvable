#!/usr/bin/env python3
"""
Learner Agent Dashboard - BTOP-like TUI
Shows real-time progress of learning pipelines
"""

import os
import sys
import time
import sqlite3
from datetime import datetime
from threading import Thread

from learner_config import get_db_path

# Try to import blessed for terminal features, fallback to basic
try:
    from blessed import Terminal
    HAS_BLESSED = True
except ImportError:
    HAS_BLESSED = False
    import subprocess

# Paths
DB_PATH = get_db_path()
INDEX_FILE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "memory", "INDEX.md")

def get_db_stats(db_path):
    """Get stats from the unified learnings database"""
    if not os.path.exists(db_path):
        return {"count": 0, "latest": "Never", "size_mb": 0}

    try:
        conn = sqlite3.connect(db_path)
        cursor = conn.cursor()

        cursor.execute("SELECT COUNT(*) FROM learnings")
        count = cursor.fetchone()[0]

        cursor.execute("SELECT MAX(processed_at) FROM learnings")
        latest = cursor.fetchone()[0] or "Never"

        conn.close()

        size_mb = os.path.getsize(db_path) / (1024 * 1024)

        return {"count": count, "latest": latest, "size_mb": size_mb}
    except Exception as e:
        return {"count": 0, "latest": f"Error: {e}", "size_mb": 0}

def get_index_stats():
    """Get index stats"""
    if not os.path.exists(INDEX_FILE):
        return {"entries": 0, "size_kb": 0}

    with open(INDEX_FILE) as f:
        content = f.read()

    entries = content.count("## ")
    size_kb = len(content) / 1024
    return {"entries": entries, "size_kb": size_kb}

def draw_bar(value, max_val, width=20):
    """Draw a progress bar"""
    if max_val == 0:
        filled = 0
    else:
        filled = int((value / max_val) * width)
    bar = "+" * filled + "-" * (width - filled)
    return f"[{bar}]"

def render(t):
    """Render the dashboard"""
    print(t.clear)
    print(t.bold + t.cyan + "=" * 66 + t.normal)
    print(t.bold + t.cyan + "  " + t.normal + t.bold + t.yellow + "LEARNER AGENT DASHBOARD" + t.normal)
    print(t.bold + t.cyan + "=" * 66 + t.normal)

    # Get stats
    db_stats = get_db_stats(DB_PATH)
    index_stats = get_index_stats()

    # Current time
    now = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    print(t.white + f"  Last Updated: {now}" + t.normal)
    print()

    # Learnings
    print(t.bold + "  -- LEARNINGS ---------------------------------------------------------")
    print(t.bold + "  | " + t.normal)
    print(t.bold + "  | " + t.normal + f"Total Learnings: {t.cyan}{db_stats['count']:,}{t.normal}")
    print(t.bold + "  | " + t.normal + f"Latest:          {t.white}{db_stats['latest']}{t.normal}")
    print(t.bold + "  | " + t.normal + f"Database Size:   {t.yellow}{db_stats['size_mb']:.2f} MB{t.normal}")
    bar = draw_bar(db_stats['count'], 1000)
    print(t.bold + "  | " + t.normal + f"Progress: {bar} {db_stats['count']}/1000")
    print(t.bold + "  | " + t.normal)
    print(t.bold + "  ----------------------------------------------------------------------")
    print()

    # Index
    print(t.bold + "  -- INDEX -------------------------------------------------------------")
    print(t.bold + "  | " + t.normal)
    print(t.bold + "  | " + t.normal + f"Indexed Entries: {t.cyan}{index_stats['entries']}{t.normal}")
    print(t.bold + "  | " + t.normal + f"Index Size:      {t.yellow}{index_stats['size_kb']:.1f} KB{t.normal}")
    print(t.bold + "  | " + t.normal)
    print(t.bold + "  ----------------------------------------------------------------------")
    print()

    print(t.white + "  [Refresh: 2s] [Quit: Ctrl+C]" + t.normal)
    print()

def render_basic():
    """Fallback basic render without blessed"""
    os.system('clear')
    db_stats = get_db_stats(DB_PATH)

    print("=" * 60)
    print("  LEARNER AGENT DASHBOARD")
    print("=" * 60)
    print(f"Time: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print()
    print(f"Total Learnings: {db_stats['count']:,} ({db_stats['size_mb']:.2f} MB)")
    print()
    print("[Refresh: 2s] [Quit: Ctrl+C]")
    print()

def main():
    if HAS_BLESSED:
        t = Terminal()
        with t.fullscreen():
            try:
                while True:
                    render(t)
                    time.sleep(2)
            except KeyboardInterrupt:
                print(t.normal + t.clear)
                print("Dashboard closed.")
    else:
        try:
            while True:
                render_basic()
                time.sleep(2)
        except KeyboardInterrupt:
            print("\nDashboard closed.")

if __name__ == "__main__":
    main()
