"""
One-shot verifier: PRAGMA user_version matches expected target, and every
CREATE statement in the on-disk DB byte-matches the corresponding text in
the migration files. Exits non-zero on any mismatch.

Run after a fresh `pnpm tauri dev` / `pnpm tauri build` has created
bay.db. Not used by the app at runtime.

v0.2.0: now loads expected CREATEs from ALL migration files in order
(001, 002, ...), with later migrations overriding earlier ones —
matching the actual migration semantics. Migration 002 rebuilds the
`items` table with added CHECK constraints and adds two triggers
(events_no_update, events_no_delete), so the live schema reflects the
union of all migrations, not just 001.
"""

import glob
import os
import re
import sqlite3
import sys

EXPECTED_VERSION = 2
DB_PATH = os.path.join(
    os.environ["USERPROFILE"],
    "AppData",
    "Roaming",
    "com.bay.desktop",
    "bay.db",
)
MIGRATIONS_DIR = os.path.join(
    os.path.dirname(__file__), "..", "migrations"
)


def load_expected_creates_from_all_migrations(migrations_dir: str) -> dict[str, str]:
    """Load CREATE statements from every migration file in numeric order.

    Later migrations override earlier ones for the same object name
    (e.g. migration 002's rebuilt `items` table replaces 001's). This
    mirrors how the migration runner applies them: each migration's
    CREATEs/ALTERs land in sequence, and the final schema reflects the
    union with overrides.
    """
    out: dict[str, str] = {}
    paths = sorted(glob.glob(os.path.join(migrations_dir, "*.sql")))
    for path in paths:
        with open(path, "r", encoding="utf-8") as f:
            sql = f.read()
        # Match CREATE TABLE, CREATE INDEX, and CREATE TRIGGER.
        # Each ends at its matching semicolon.
        for match in re.finditer(
            r"(CREATE\s+(?:TABLE|INDEX|TRIGGER)\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)\b[^;]*)(;)",
            sql,
            re.IGNORECASE | re.DOTALL,
        ):
            stmt = match.group(1)
            name = match.group(2)
            out[name] = stmt
    return out


def main() -> int:
    if not os.path.exists(DB_PATH):
        print(f"DB not found at {DB_PATH}. Launch the app first.", file=sys.stderr)
        return 2

    expected = load_expected_creates_from_all_migrations(MIGRATIONS_DIR)
    conn = sqlite3.connect(DB_PATH)
    try:
        version = conn.execute("PRAGMA user_version").fetchone()[0]
        if version != EXPECTED_VERSION:
            print(f"user_version mismatch: got {version}, want {EXPECTED_VERSION}")
            return 1

        rows = conn.execute(
            "SELECT name, sql FROM sqlite_schema "
            "WHERE type IN ('table','index','trigger') AND name NOT LIKE 'sqlite_%' "
            "ORDER BY name"
        ).fetchall()
    finally:
        conn.close()

    actual = {name: sql for name, sql in rows if sql is not None}

    if set(actual) != set(expected):
        print("schema object-set mismatch")
        print(f"  actual:   {sorted(actual)}")
        print(f"  expected: {sorted(expected)}")
        missing = set(expected) - set(actual)
        extra = set(actual) - set(expected)
        if missing:
            print(f"  missing from DB: {sorted(missing)}")
        if extra:
            print(f"  extra in DB:     {sorted(extra)}")
        return 1

    failures = 0
    for name in sorted(expected):
        want = expected[name].rstrip()
        got = actual[name].rstrip()
        if want == got:
            print(f"OK  {name}")
            continue
        print(f"MISMATCH {name}")
        print("  expected:")
        for ln in want.splitlines():
            print(f"    {ln}")
        print("  actual:")
        for ln in got.splitlines():
            print(f"    {ln}")
        failures += 1

    if failures:
        print(f"\n{failures} mismatch(es)")
        return 1

    print(f"\nuser_version={version}, {len(expected)} objects verified byte-identical")
    return 0


if __name__ == "__main__":
    sys.exit(main())
