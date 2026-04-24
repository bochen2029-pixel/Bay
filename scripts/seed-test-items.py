"""
Inject a known set of ITEM_CREATED events + items rows into the on-disk
Bay DB for manual smoke-testing of the read path. Wipes existing rows
first. Not part of the app runtime; one-shot scaffolding.
"""

import json
import os
import sqlite3
import sys
import time
import uuid

DB_PATH = os.path.join(
    os.environ["USERPROFILE"],
    "AppData",
    "Roaming",
    "com.bay.desktop",
    "bay.db",
)

SEED = [
    ("inbox", "a", "first inbox capture"),
    ("inbox", "h", "second inbox capture"),
    ("inbox", "p", "third inbox capture"),
    ("A", "m", "alpha focus"),
    ("B", "m", "bravo parked"),
]


def main() -> int:
    if not os.path.exists(DB_PATH):
        print(f"DB not found at {DB_PATH}. Launch the app once first.", file=sys.stderr)
        return 2

    conn = sqlite3.connect(DB_PATH)
    try:
        conn.execute("DELETE FROM items")
        conn.execute("DELETE FROM events")
        # Reset AUTOINCREMENT counter so new events get id=1,2,3,...
        conn.execute("DELETE FROM sqlite_sequence WHERE name='events'")
        ts = int(time.time() * 1000)
        for i, (tier, rank, content) in enumerate(SEED):
            item_id = str(uuid.uuid4())
            payload = json.dumps(
                {
                    "content": content,
                    "tier": tier,
                    "rank": rank,
                    "start_at": None,
                    "due_at": None,
                }
            )
            conn.execute(
                "INSERT INTO events (ts, type, item_id, payload) VALUES (?, 'ITEM_CREATED', ?, ?)",
                (ts + i, item_id, payload),
            )
            conn.execute(
                "INSERT INTO items (id, content, tier, rank, state, blocked_reason, start_at, due_at, created_at, updated_at, deleted) "
                "VALUES (?, ?, ?, ?, 'active', NULL, NULL, NULL, ?, ?, 0)",
                (item_id, content, tier, rank, ts + i, ts + i),
            )
        conn.commit()
    finally:
        conn.close()

    print(f"seeded {len(SEED)} items into {DB_PATH}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
