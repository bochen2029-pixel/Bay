#!/usr/bin/env python3
"""
CI check: the test suite must actually CATCH the defects it claims to.

A passing suite proves nothing on its own — this repo learned that the
hard way. Across the v0.3 verification chain, five consecutive cold
reviews each found a real defect behind a fully green suite, and twice
a test added that same round turned out to assert less than it looked
like it did:

  * an order-independence property used a helper that PRESERVES order,
    so it compared one ordering against itself and asserted nothing;
  * the pass-2 contest policy could be INVERTED — or replaced with a
    raw UUID sort — with all 237 tests still green, because the test
    asserted that a contest happened and never who won.

Both were found by hand: break the implementation, watch the test fail,
restore. This script makes that a gate instead of a habit. Each entry
below is a mutation that SHOULD break something; if the suite still
passes, the guarding assertion is decoration and the entry names what
was lost.

Usage:
  python scripts/check-mutations.py            # all mutations
  python scripts/check-mutations.py --list     # names only
  python scripts/check-mutations.py -k today   # subset by substring

Adding a mutation: when a cold review finds a defect, add the mutation
that reintroduces it. That is the cheapest way to guarantee the class
cannot silently come back.
"""

import argparse
import os
import subprocess
import sys

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
MANIFEST = os.path.join(REPO_ROOT, "src-tauri", "Cargo.toml")

LLM = "src-tauri/src/commands/llm.rs"
DAY = "src-tauri/src/commands/day.rs"
ITEMS = "src-tauri/src/commands/items.rs"
DB = "src-tauri/src/db/mod.rs"

# Each mutation reintroduces a defect a cold review actually found.
# `why` is the finding it guards; it is printed when the mutation
# survives, because that message IS the report.
MUTATIONS = [
    {
        "name": "today-cap/reentry-guard",
        "file": DAY,
        "find": "if db::items::count_active_today(tx, &date)? + net >= TODAY_CAP {",
        "replace": "if false {",
        "why": "pass 1 BLOCKING: reactivation could put 4 active on a Today date",
    },
    {
        "name": "recurrence/blocked-reason-on-accept",
        "file": LLM,
        "find": """                    let reason = if cur.state == ItemState::Blocked {
                        cur.blocked_reason.clone()
                    } else {
                        None
                    };
                    drafts.push(state_change_draft(
                        &op.item_id,
                        cur.state,
                        ItemState::Done,
                        reason,
                    ));""",
        "replace": "                    drafts.push(state_change_draft(&op.item_id, cur.state, ItemState::Done, None));",
        "why": "pass 4 BLOCKING: accepting done on a blocked item killed Ctrl+Z permanently",
    },
    {
        "name": "today/finished-item-strip",
        "file": LLM,
        "find": ".map(|it| it.state == ItemState::Active && it.today_on.is_some())",
        "replace": ".map(|it| it.today_on.is_some())",
        "why": "pass 4 MAJOR: a finished item lost its Today membership, freeing nothing",
    },
    {
        "name": "contest/spawn-policy-inverted",
        "file": LLM,
        "find": "spawn_candidates.sort_by(|a, b| board_order(&orig, &sim, a).cmp(&board_order(&orig, &sim, b)));",
        "replace": "spawn_candidates.sort_by(|a, b| board_order(&orig, &sim, b).cmp(&board_order(&orig, &sim, a)));",
        "why": "pass 5 MAJOR: the declared contest policy was indistinguishable from its inverse",
    },
    {
        "name": "contest/today-policy-inverted",
        "file": LLM,
        "find": "today_candidates.sort_by(|a, b| board_order(&orig, &sim, b).cmp(&board_order(&orig, &sim, a)));",
        "replace": "today_candidates.sort_by(|a, b| board_order(&orig, &sim, a).cmp(&board_order(&orig, &sim, b)));",
        "why": "pass 5 MAJOR: same, on the Today door",
    },
    {
        "name": "contest/key-is-arbitrary",
        "file": LLM,
        "find": "spawn_candidates.sort_by(|a, b| board_order(&orig, &sim, a).cmp(&board_order(&orig, &sim, b)));",
        "replace": "spawn_candidates.sort();",
        "why": "pass 5 MAJOR: any deterministic key satisfied the permutation test",
    },
    {
        "name": "contest/key-reads-mutated-sim",
        "file": LLM,
        "find": "match orig.get(id).or_else(|| sim.get(id)) {",
        "replace": "match sim.get(id).or_else(|| orig.get(id)) {",
        "why": "pass 5 MAJOR: keying on the post-diff board let move-op order decide contests",
    },
    {
        "name": "recurrence/freed-slot-accounting",
        "file": ITEMS,
        "find": """    if parent.state == ItemState::Active {
        *acct.net_active.entry(parent.tier).or_insert(0) -= 1;
    }

    let rule = match recurrence_rule_of(parent) {""",
        "replace": """    let rule = match recurrence_rule_of(parent) {""",
        "why": "pass 2 MINOR: slots freed by non-recurring parents were ignored, over-routing children to Inbox",
    },
    {
        "name": "envelope/device-id-regenerates",
        "file": DB,
        "find": "INSERT OR IGNORE INTO meta (key, value) VALUES ('device_id', ?1)",
        "replace": "INSERT OR REPLACE INTO meta (key, value) VALUES ('device_id', ?1)",
        "why": "ADR-008: identity must travel with the data, not be reminted each launch",
    },
]


def run_suite() -> bool:
    """True if the suite passes."""
    proc = subprocess.run(
        ["cargo", "test", "--manifest-path", MANIFEST],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    return proc.returncode == 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--list", action="store_true", help="print mutation names and exit")
    ap.add_argument("-k", metavar="SUBSTR", help="only mutations whose name contains SUBSTR")
    args = ap.parse_args()

    selected = [m for m in MUTATIONS if not args.k or args.k in m["name"]]
    if args.list:
        for m in selected:
            print(f"{m['name']:38} {m['why']}")
        return 0
    if not selected:
        print(f"no mutation matches {args.k!r}", file=sys.stderr)
        return 2

    print("Baseline: the suite must pass before mutating.")
    if not run_suite():
        print("FAIL: baseline suite is already red — fix that first.", file=sys.stderr)
        return 2
    print("OK  baseline green\n")

    survivors = []
    for m in selected:
        path = os.path.join(REPO_ROOT, m["file"])
        with open(path, "r", encoding="utf-8") as f:
            original = f.read()
        if m["find"] not in original:
            print(f"FAIL {m['name']}: anchor not found in {m['file']} — mutation is stale")
            survivors.append((m, "stale anchor"))
            continue
        mutated = original.replace(m["find"], m["replace"], 1)
        try:
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(mutated)
            caught = not run_suite()
        finally:
            # Restore unconditionally — a crash here would leave the
            # working tree mutated, which is far worse than a failure.
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(original)
        if caught:
            print(f"OK   {m['name']}: caught")
        else:
            print(f"FAIL {m['name']}: SURVIVED — nothing tests {m['why']}")
            survivors.append((m, "survived"))

    print()
    if survivors:
        for m, how in survivors:
            print(f"  {how}: {m['name']} — {m['why']}")
        print(f"\n{len(survivors)} mutation(s) not caught by the suite")
        return 1
    print(f"All {len(selected)} mutations caught. The assertions bite.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
