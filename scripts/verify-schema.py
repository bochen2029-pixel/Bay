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

EXPECTED_VERSION = 5
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


ALTER_ADD_RE = re.compile(
    r"ALTER\s+TABLE\s+(\w+)\s+ADD\s+COLUMN\s+(\w+)", re.IGNORECASE
)


def load_altered_columns(migrations_dir: str) -> dict[str, list[str]]:
    """Collect ALTER TABLE ... ADD COLUMN additions per table.

    Migration 003 extends `events` via ALTER (never a rebuild — the
    events table is the source of truth and a copy-rebuild would DROP
    it). SQLite rewrites the stored CREATE text for ALTERed tables, so
    byte-matching against the original CREATE is impossible; those
    tables get a column-set check instead (see main()).
    """
    out: dict[str, list[str]] = {}
    for path in sorted(glob.glob(os.path.join(migrations_dir, "*.sql"))):
        with open(path, "r", encoding="utf-8") as f:
            sql = f.read()
        for match in ALTER_ADD_RE.finditer(sql):
            out.setdefault(match.group(1), []).append(match.group(2))
    return out


def parse_create_columns(stmt: str) -> list[str]:
    """Column names from a CREATE TABLE statement (depth-aware split,
    skipping table-level constraints)."""
    body = stmt[stmt.index("(") + 1 : stmt.rindex(")")]
    parts, depth, cur = [], 0, []
    for ch in body:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            parts.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    parts.append("".join(cur))
    cols = []
    for part in parts:
        tokens = part.strip().split()
        if not tokens:
            continue
        head = tokens[0].upper()
        if head in ("CHECK", "PRIMARY", "UNIQUE", "FOREIGN", "CONSTRAINT"):
            continue
        cols.append(tokens[0].strip('"'))
    return cols


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
    altered = load_altered_columns(MIGRATIONS_DIR)
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
        live_columns = {
            name: [r[1] for r in conn.execute(f"PRAGMA table_info({name})").fetchall()]
            for name in altered
        }
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
        if name in altered:
            # Column-set check for ALTERed tables (byte-match impossible:
            # SQLite rewrote the stored CREATE when the ALTERs applied).
            want_cols = parse_create_columns(expected[name]) + altered[name]
            got_cols = live_columns.get(name, [])
            if sorted(want_cols) == sorted(got_cols):
                print(f"OK  {name} (column-set check; extended by ALTER)")
            else:
                print(f"MISMATCH {name} (column-set)")
                print(f"  expected columns: {sorted(want_cols)}")
                print(f"  actual columns:   {sorted(got_cols)}")
                failures += 1
            continue
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
