#!/usr/bin/env python3
"""
CI check: golden-case integrity for the 6 critical modules.

Two checks:
  1. Every critical-module golden file exists and has >=1 case.
     (AUTONOMY_CHARTER 9c: critical modules require a non-LLM oracle;
     golden cases are the cheapest such oracle.)
  2. No diff touches a contracts/golden/*.json file with _status
     "frozen" without a "SPEC:" tag in the commit message. The agent
     may author golden cases as proposals (_status: proposed) freely;
     once frozen, they're operator-owned.

Run as a pre-commit / pre-merge check. Exits non-zero on any failure.

Usage:
  python scripts/check-golden.py                  # check 1 only
  python scripts/check-golden.py --commit <sha>   # also check 2 against a commit
"""

import json
import os
import subprocess
import sys

REPO_ROOT = os.path.join(os.path.dirname(__file__), "..")
GOLDEN_DIR = os.path.join(REPO_ROOT, "contracts", "golden")

# AUTONOMY_CHARTER 9c critical modules. Each must have a golden file
# with >=1 case. (rank.json mirrors scripts/rank-fixtures.json which
# has 42 cases; the others are authored directly.)
CRITICAL_GOLDEN = {
    "projection.json": "db::items::apply_event_to_projection + rebuild_projection_inner",
    "swap.json": "commands::items::swap_move_inner",
    "caps.json": "commands::items::{create,move,set_item_state,swap_move}_inner",
    "rank.json": "domain::rank::rank_between",
    # Added v0.3: the Today overlay had no golden coverage, and a cold
    # review traced a BLOCKING cap bypass to exactly that absence — the
    # law was in doctrine, enforced in code, and asserted nowhere an
    # operator owned.
    "today.json": "commands::day::{add_to_today,open_day,roll_day,today_overflow_draft}",
}


def check_files_exist_and_have_cases() -> list[str]:
    """Check 1: every critical golden file exists and has >=1 case."""
    failures = []
    for fname, module in CRITICAL_GOLDEN.items():
        path = os.path.join(GOLDEN_DIR, fname)
        if not os.path.exists(path):
            failures.append(f"MISSING: {fname} (module: {module})")
            continue
        with open(path, "r", encoding="utf-8") as f:
            try:
                data = json.load(f)
            except json.JSONDecodeError as e:
                failures.append(f"INVALID JSON: {fname}: {e}")
                continue
        # rank.json mirrors scripts/rank-fixtures.json (42 cases there).
        if fname == "rank.json":
            canonical = os.path.join(REPO_ROOT, "scripts", "rank-fixtures.json")
            if not os.path.exists(canonical):
                failures.append(f"MISSING canonical source: scripts/rank-fixtures.json")
            else:
                with open(canonical, "r", encoding="utf-8") as f:
                    canon = json.load(f)
                cases = canon.get("cases", [])
                if len(cases) < 1:
                    failures.append(f"EMPTY: scripts/rank-fixtures.json has 0 cases")
                else:
                    print(f"OK  {fname} ({len(cases)} cases in scripts/rank-fixtures.json)")
            continue
        # Other files: count cases directly.
        cases = data.get("cases", [])
        if len(cases) < 1:
            failures.append(f"EMPTY: {fname} has 0 cases (module: {module})")
        else:
            status = data.get("_status", "proposed")
            print(f"OK  {fname} ({len(cases)} cases, _status: {status})")
    return failures


def check_no_frozen_edits_without_spec_tag(commit_sha: str) -> list[str]:
    """Check 2: no diff touches a frozen golden file without SPEC: tag."""
    failures = []
    # Get the files changed in the commit.
    try:
        out = subprocess.run(
            ["git", "show", "--no-patch", "--format=%s%n%b", commit_sha],
            cwd=REPO_ROOT, capture_output=True, text=True, check=True,
        ).stdout
    except subprocess.CalledProcessError as e:
        return [f"git show failed: {e}"]

    has_spec_tag = "SPEC:" in out
    try:
        changed = subprocess.run(
            ["git", "show", "--name-only", "--pretty=format:", commit_sha],
            cwd=REPO_ROOT, capture_output=True, text=True, check=True,
        ).stdout.strip().splitlines()
    except subprocess.CalledProcessError as e:
        return [f"git show --name-only failed: {e}"]

    for path in changed:
        if not path.startswith("contracts/golden/") or not path.endswith(".json"):
            continue
        full = os.path.join(REPO_ROOT, path)
        if not os.path.exists(full):
            continue  # file deleted in this commit; skip
        with open(full, "r", encoding="utf-8") as f:
            try:
                data = json.load(f)
            except json.JSONDecodeError:
                continue
        status = data.get("_status", "proposed")
        if status == "frozen" and not has_spec_tag:
            failures.append(
                f"FORBIDDEN: commit {commit_sha[:8]} touches frozen golden "
                f"file {path} without a SPEC: tag in the commit message. "
                f"Either add SPEC: + log a SPEC_AMENDMENT.md, or revert the edit."
            )
    return failures


def main() -> int:
    failures = check_files_exist_and_have_cases()

    commit_sha = sys.argv[2] if len(sys.argv) > 2 and sys.argv[1] == "--commit" else None
    if commit_sha:
        failures.extend(check_no_frozen_edits_without_spec_tag(commit_sha))

    if failures:
        print("\n" + "\n".join(f"FAIL: {f}" for f in failures))
        print(f"\n{len(failures)} failure(s)")
        return 1
    print("\nAll golden-case checks passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
