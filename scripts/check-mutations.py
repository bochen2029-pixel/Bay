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
import re
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
        "name": "contest/tier-component-ignored",
        "file": LLM,
        "find": """            match it.tier {
                Tier::A => 0,
                Tier::B => 1,
                Tier::C => 2,
                Tier::Inbox => 3,
            },""",
        "replace": "            0u8,",
        "why": "pass 6 MAJOR: the tier byte of the contest key contributed nothing observable",
    },
    {
        "name": "contest/tier-component-inverted",
        "file": LLM,
        "find": """                Tier::A => 0,
                Tier::B => 1,
                Tier::C => 2,
                Tier::Inbox => 3,""",
        "replace": """                Tier::A => 3,
                Tier::B => 2,
                Tier::C => 1,
                Tier::Inbox => 0,""",
        "why": "pass 6 MAJOR: an Inbox item could outrank an A item for a Today slot",
    },
    {
        "name": "contest/today-door-key-reads-sim",
        "file": LLM,
        "find": "today_candidates.sort_by(|a, b| board_order(&orig, &sim, b).cmp(&board_order(&orig, &sim, a)));",
        "replace": "today_candidates.sort_by(|a, b| board_order(&sim, &orig, b).cmp(&board_order(&sim, &orig, a)));",
        "why": "pass 6 MAJOR: the Today door's pre-diff keying was only pinned through the spawn door",
    },
    {
        "name": "golden/declared-rank-ignored",
        "file": "src-tauri/src/golden_runner.rs",
        "find": """    let Some(rank) = op["rank"].as_str() else { return };""",
        "replace": """    let Some(rank) = op["rank"].as_str() else { return };
    if true { return; }""",
        "why": "pass 6 MINOR: golden cases would again describe the inverse of the board they build",
    },
    {
        "name": "accept/completed-not-cleared-by-later-active",
        "file": LLM,
        "find": "                    completed.remove(&op.item_id); // done-then-active: not a completion",
        "replace": "                    // completed.remove(&op.item_id);",
        "why": "pass 6 blind spot: [done x, active x] would spawn a child while x ends active",
    },
    {
        "name": "accept/spawn-on-accept-door",
        "file": LLM,
        "find": "            let rule = match crate::commands::items::recurrence_rule_of(&parent) {",
        "replace": "            let rule = match None::<crate::domain::Recurrence> {",
        "why": "pass 1 MAJOR: the accept-diff completed recurring items without spawning the next instance",
    },
    {
        "name": "accept/cap-ignores-spawned-child",
        "file": LLM,
        "find": "                Some(cap) if effective_active(tx, &orig, &sim, parent.tier)? >= cap => Tier::Inbox,",
        "replace": "                Some(cap) if db::items::count_active_in_tier(tx, parent.tier)? > cap => Tier::Inbox,",
        "why": "pass 2 BLOCKING: placement read the live projection while the cap check read the simulation — A committed at 6/5",
    },
    {
        "name": "envelope/device-id-regenerates",
        "file": DB,
        "find": "INSERT OR IGNORE INTO meta (key, value) VALUES ('device_id', ?1)",
        "replace": "INSERT OR REPLACE INTO meta (key, value) VALUES ('device_id', ?1)",
        "why": "ADR-008: identity must travel with the data, not be reminted each launch",
    },
]


def run_suite() -> tuple[bool, list[str]]:
    """(passed, names of tests that FAILED).

    The failing test names matter: a mutation "caught" by an unrelated
    test is not really guarded, and a mutation that only fails to
    COMPILE is a weak guard — it proves the code changed, not that any
    assertion noticed. Reporting the guard makes both visible.
    """
    proc = subprocess.run(
        ["cargo", "test", "--manifest-path", MANIFEST],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    out = proc.stdout + proc.stderr
    if "error[E" in out or "could not compile" in out:
        return proc.returncode == 0, ["<COMPILE ERROR — weak guard>"]
    failed = re.findall(r"^\s{4}(\S+)$", out, re.MULTILINE)
    # The `failures:` block lists bare test paths; keep the plausible ones.
    names = sorted({f for f in failed if "::" in f})
    return proc.returncode == 0, names


def working_tree_is_clean() -> bool:
    proc = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    return proc.returncode == 0 and not proc.stdout.strip()


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

    # This script edits the real working tree. `try/finally` restores on
    # exceptions and Ctrl-C, but NOT on SIGKILL, a CI cancellation, or a
    # crash — and a mutated tree left behind is far worse than a failed
    # run. Requiring a clean tree makes that state detectable: if a run
    # is killed, the next one refuses and `git checkout .` is the fix.
    if not working_tree_is_clean():
        print(
            "FAIL: working tree is dirty. This script mutates files in place and\n"
            "      needs a clean tree so an interrupted run is recoverable with\n"
            "      `git checkout -- .` (and so its own edits are never committed).",
            file=sys.stderr,
        )
        return 2

    print("Baseline: the suite must pass before mutating.")
    passed, _ = run_suite()
    if not passed:
        print("FAIL: baseline suite is already red — fix that first.", file=sys.stderr)
        return 2
    print("OK  baseline green\n")

    survivors = []
    for m in selected:
        path = os.path.join(REPO_ROOT, m["file"])
        with open(path, "r", encoding="utf-8", newline="") as f:
            original = f.read()
        if m["find"] not in original:
            print(f"FAIL {m['name']}: anchor not found in {m['file']} — mutation is stale")
            survivors.append((m, "stale anchor"))
            continue
        if original.count(m["find"]) > 1:
            print(f"FAIL {m['name']}: anchor is ambiguous ({original.count(m['find'])} matches)")
            survivors.append((m, "ambiguous anchor"))
            continue
        mutated = original.replace(m["find"], m["replace"], 1)
        try:
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(mutated)
            passed, failing = run_suite()
        finally:
            # Restore unconditionally.
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(original)
        if not passed:
            guard = failing[0] if failing else "(unnamed)"
            extra = f" +{len(failing) - 1}" if len(failing) > 1 else ""
            print(f"OK   {m['name']}: caught by {guard}{extra}")
            if failing and failing[0].startswith("<COMPILE"):
                print(f"     ^ weak guard: no assertion observed the change")
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
