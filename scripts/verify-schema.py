"""
One-shot verifier: PRAGMA user_version matches expected target, and every
CREATE statement in the on-disk DB byte-matches the corresponding text in
the migration files. Exits non-zero on any mismatch.

Run after a fresh `pnpm tauri dev` / `pnpm tauri build` has created
bay.db — or with `--fresh` to build a throwaway DB from the migration
files and verify that instead (no app launch needed; suitable for CI).
Not used by the app at runtime.

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
import tempfile

EXPECTED_VERSION = 6
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
    # ONE ordered pass per file over CREATE / RENAME / DROP. Order is
    # load-bearing: migration 002 rebuilds `items` as CREATE items_new →
    # DROP items → RENAME items_new TO items. Applying all renames
    # before all drops (or vice versa) silently deletes the surviving
    # table from the expected set.
    # Triggers are matched separately and terminate at `END;`, not at
    # the first `;` — a trigger BODY contains statements, so the
    # `[^;]*` form silently truncated every trigger to its first inner
    # semicolon and then never byte-matched the stored schema.
    stmt_re = re.compile(
        r"(?P<trigger>CREATE\s+TRIGGER\s+(?:IF\s+NOT\s+EXISTS\s+)?(?P<tname>\w+)\b.*?END);"
        r"|(?P<create>CREATE\s+(?:TABLE|(?:UNIQUE\s+)?INDEX)\s+"
        r"(?:IF\s+NOT\s+EXISTS\s+)?(?P<cname>\w+)\b[^;]*);"
        r"|ALTER\s+TABLE\s+(?P<old>\w+)\s+RENAME\s+TO\s+(?P<new>\w+)"
        r"|DROP\s+(?:TABLE|INDEX)\s+(?:IF\s+EXISTS\s+)?(?P<drop>\w+)",
        re.IGNORECASE | re.DOTALL,
    )
    for path in paths:
        with open(path, "r", encoding="utf-8") as f:
            sql = f.read()
        # Strip `--` comments first: migration 003's header discusses
        # "DROP TABLE events" as the thing it deliberately does NOT do,
        # and a regex over raw text would obey the prose.
        sql = re.sub(r"--[^\n]*", "", sql)
        for m in stmt_re.finditer(sql):
            if m.group("trigger"):
                out[m.group("tname")] = m.group("trigger")
            elif m.group("create"):
                out[m.group("cname")] = m.group("create")
            elif m.group("new"):
                old, new = m.group("old"), m.group("new")
                if old in out:
                    out[new] = out.pop(old)
            elif m.group("drop"):
                out.pop(m.group("drop"), None)
    return out


def build_fresh_db(migrations_dir: str, path: str) -> None:
    """Apply every migration in order to a throwaway DB.

    Mirrors the Rust runner (`db::run_migrations`) so the schema gate is
    runnable in CI and on a dev machine that has never launched the app.
    Without this the script only ever ran against a live database, which
    is why its regex breakage went unnoticed from migration 002 to 006.
    """
    if os.path.exists(path):
        os.remove(path)
    conn = sqlite3.connect(path)
    try:
        for i, p in enumerate(sorted(glob.glob(os.path.join(migrations_dir, "*.sql"))), start=1):
            with open(p, "r", encoding="utf-8") as f:
                conn.executescript(f.read())
            conn.executescript(f"PRAGMA user_version = {i};")
        conn.commit()
    finally:
        conn.close()


def main() -> int:
    fresh = "--fresh" in sys.argv
    db_path = DB_PATH
    if fresh:
        db_path = os.path.join(tempfile.gettempdir(), "bay-verify-schema.db")
        build_fresh_db(MIGRATIONS_DIR, db_path)
        print(f"(--fresh) built a throwaway DB from migrations at {db_path}")
    elif not os.path.exists(DB_PATH):
        print(
            f"DB not found at {DB_PATH}. Launch the app first, or run with --fresh "
            f"to verify the migrations against a throwaway database.",
            file=sys.stderr,
        )
        return 2

    expected = load_expected_creates_from_all_migrations(MIGRATIONS_DIR)
    altered = load_altered_columns(MIGRATIONS_DIR)
    conn = sqlite3.connect(db_path)
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
