"""
One-shot verifier: PRAGMA user_version matches expected target, and every
CREATE statement in the on-disk DB byte-matches the corresponding text in
migrations/001_initial.sql. Exits non-zero on any mismatch.

Run after a fresh `pnpm tauri dev` / `pnpm tauri build` has created
bay.db. Not used by the app at runtime.
"""

import os
import re
import sqlite3
import sys

EXPECTED_VERSION = 1
DB_PATH = os.path.join(
    os.environ["USERPROFILE"],
    "AppData",
    "Roaming",
    "com.bay.desktop",
    "bay.db",
)
MIGRATION_PATH = os.path.join(
    os.path.dirname(__file__), "..", "migrations", "001_initial.sql"
)


def load_expected_creates(path: str) -> dict[str, str]:
    with open(path, "r", encoding="utf-8") as f:
        sql = f.read()
    # Strip leading section comments so we're left with just CREATE stmts.
    # Each CREATE ends at its matching semicolon at column 0 or inline.
    out: dict[str, str] = {}
    for match in re.finditer(
        r"(CREATE\s+(?:TABLE|INDEX)\s+(\w+)\b[^;]*)(;)",
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

    expected = load_expected_creates(MIGRATION_PATH)
    conn = sqlite3.connect(DB_PATH)
    try:
        version = conn.execute("PRAGMA user_version").fetchone()[0]
        if version != EXPECTED_VERSION:
            print(f"user_version mismatch: got {version}, want {EXPECTED_VERSION}")
            return 1

        rows = conn.execute(
            "SELECT name, sql FROM sqlite_schema "
            "WHERE type IN ('table','index') AND name NOT LIKE 'sqlite_%' "
            "ORDER BY name"
        ).fetchall()
    finally:
        conn.close()

    actual = {name: sql for name, sql in rows}

    if set(actual) != set(expected):
        print("schema object-set mismatch")
        print(f"  actual:   {sorted(actual)}")
        print(f"  expected: {sorted(expected)}")
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
