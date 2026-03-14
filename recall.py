#!/usr/bin/env python3
"""
recall.py -- Agent knowledge recall from learnings.db

Query the learner database for domain knowledge specific to a given agent.
Read-only access. Safe to call concurrently with the learning daemon.

Usage:
    python3 recall.py --agent my-agent --query "deployment strategy"
    python3 recall.py --agent my-agent --all --limit 50
    python3 recall.py --agent my-agent --stats
    python3 recall.py --agent my-agent --query "budget" --json
"""

import argparse
import json
import os
import re
import sqlite3
import sys

from learner_config import get_configured_agents

DB_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "db", "learnings.db")

STOPWORDS = frozenset({
    'the', 'and', 'for', 'with', 'that', 'this', 'from', 'are', 'was',
    'has', 'have', 'been', 'will', 'can', 'could', 'would', 'should',
    'about', 'into', 'what', 'when', 'where', 'which', 'who', 'how',
    'not', 'but', 'its', 'our', 'their', 'does', 'did', 'also', 'than',
    'then', 'just', 'more', 'some', 'other', 'any', 'all', 'each',
})

VALID_AGENTS = tuple(get_configured_agents()) if get_configured_agents() else None


def connect():
    if not os.path.exists(DB_PATH):
        print(f"Error: Database not found at {DB_PATH}", file=sys.stderr)
        sys.exit(1)
    conn = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
    conn.row_factory = sqlite3.Row
    return conn


def extract_keywords(query):
    words = re.findall(r'\w+', query)
    return [w.lower() for w in words if len(w) >= 3 and w.lower() not in STOPWORDS]


def recall_learnings(agent, query, limit=20):
    conn = connect()
    keywords = extract_keywords(query)

    if not keywords:
        rows = conn.execute(
            "SELECT learning, source, folder, file_name FROM learnings "
            "WHERE agent = ? ORDER BY id DESC LIMIT ?",
            (agent, limit)
        ).fetchall()
        conn.close()
        return [dict(r) | {"score": 0} for r in rows]

    score_parts = []
    params = []
    for kw in keywords:
        score_parts.append("(CASE WHEN LOWER(learning) LIKE '%' || ? || '%' THEN 1 ELSE 0 END)")
        params.append(kw)

    score_expr = " + ".join(score_parts)

    params.append(agent)
    params.append(limit)

    sql = f"""
        SELECT * FROM (
            SELECT learning, source, folder, file_name,
                   ({score_expr}) as score
            FROM learnings
            WHERE agent = ?
        ) WHERE score > 0
        ORDER BY score DESC
        LIMIT ?
    """

    rows = conn.execute(sql, params).fetchall()
    conn.close()
    return [dict(r) for r in rows]


def recall_all(agent, limit=100):
    conn = connect()
    rows = conn.execute(
        "SELECT learning, source, folder, file_name, processed_at "
        "FROM learnings WHERE agent = ? ORDER BY id DESC LIMIT ?",
        (agent, limit)
    ).fetchall()
    conn.close()
    return [dict(r) for r in rows]


def recall_stats(agent):
    conn = connect()
    row = conn.execute(
        "SELECT COUNT(*) as total, COUNT(DISTINCT folder) as folders, "
        "COUNT(DISTINCT source) as sources, MAX(processed_at) as latest "
        "FROM learnings WHERE agent = ?",
        (agent,)
    ).fetchone()
    conn.close()
    return dict(row)


def main():
    parser = argparse.ArgumentParser(description="Recall agent learnings from learnings.db")
    parser.add_argument("--agent", required=True,
                        help="Agent to recall for")
    parser.add_argument("--query", type=str, default="",
                        help="Search query (keywords extracted automatically)")
    parser.add_argument("--limit", type=int, default=20,
                        help="Max results (default: 20)")
    parser.add_argument("--all", action="store_true",
                        help="Return all learnings (ignores --query)")
    parser.add_argument("--stats", action="store_true",
                        help="Show knowledge stats only")
    parser.add_argument("--json", action="store_true",
                        help="Output as JSON")

    args = parser.parse_args()

    if args.stats:
        stats = recall_stats(args.agent)
        if args.json:
            print(json.dumps(stats, default=str))
        else:
            print(f"Agent:     {args.agent}")
            print(f"Learnings: {stats['total']}")
            print(f"Folders:   {stats['folders']}")
            print(f"Sources:   {stats['sources']}")
            print(f"Latest:    {stats['latest']}")
        return

    if args.all:
        results = recall_all(args.agent, args.limit)
    elif args.query:
        results = recall_learnings(args.agent, args.query, args.limit)
    else:
        parser.error("--query or --all required (or use --stats)")

    if args.json:
        print(json.dumps(results, default=str))
    else:
        if not results:
            print(f"No learnings found for {args.agent}" +
                  (f" matching '{args.query}'" if args.query else ""))
            return

        print(f"Recalled {len(results)} learnings for {args.agent}:\n")
        for i, r in enumerate(results, 1):
            score = r.get('score', '-')
            source = r.get('source', '?')
            print(f"  [{i}] ({source}) {r['learning']}")
            if r.get('file_name'):
                print(f"      file: {r['file_name']}")
            print()


if __name__ == "__main__":
    main()
