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

!! THIS SCRIPT EDITS THE REAL WORKING TREE, one file at a time, for the
!! whole of a run — which is minutes, not seconds. While it runs, the
!! checkout is DELIBERATELY WRONG. Do not run it concurrently with
!! anything that reads this repo: a cold-context reviewer that happens
!! to open a mutated file will report a defect that does not exist, and
!! nothing in its output would distinguish that from a real one. It
!! writes `.mutation-in-progress` (naming the live mutation) for the
!! duration, so a concurrent reader has something to check. A `git
!! clone` is safe — clones read committed objects, not the worktree.
!!
!! It also cannot clean up after SIGKILL, a CI cancellation, or a
!! harness timeout. That is not hypothetical: a 10-minute tool timeout
!! killed a run mid-mutation during the v0.3 chain and left the
!! swallowed-panic mutation sitting in `golden_runner.rs`. The
!! clean-tree refusal below is what makes that state visible, and
!! `git checkout -- <file>` is what fixes it.
"""

import argparse
import os
import re
import subprocess
import sys

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
MANIFEST = os.path.join(REPO_ROOT, "src-tauri", "Cargo.toml")
# Present only while a mutation is applied. Gitignored: it is a runtime
# flag for concurrent readers, not a tracked artifact.
MARKER = os.path.join(REPO_ROOT, ".mutation-in-progress")

LLM = "src-tauri/src/commands/llm.rs"
DAY = "src-tauri/src/commands/day.rs"
ITEMS = "src-tauri/src/commands/items.rs"
DB = "src-tauri/src/db/mod.rs"
GOLDEN_RUNNER = "src-tauri/src/golden_runner.rs"
SESSION = "src-tauri/src/commands/session.rs"

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
    {
        "name": "accept/unblock-drops-blocked-reason",
        "file": LLM,
        "find": """                    // Preserve the outgoing blocked reason so an undo can
                    // restore the blocked row (migration-002 CHECK).
                    let reason = if cur.state == ItemState::Blocked {
                        cur.blocked_reason.clone()
                    } else {
                        None
                    };""",
        "replace": "                    let reason: Option<String> = None;",
        "why": "P2e BLOCKING-1 at the unblock door — the done door's identical twin, fixed at the same time but never guarded",
    },
    {
        "name": "accept/spawned-child-inherits-today",
        "file": LLM,
        "find": "            child.today_on = None;",
        "replace": "            child.today_on = parent.today_on.clone();",
        "why": "a spawned child carrying its parent's day occupies a Today slot it holds no membership in, evicting a real reactivation",
    },
    {
        "name": "contest/id-tiebreak-removed",
        "file": LLM,
        "find": """            it.rank.clone(),
            it.id.clone(),""",
        "replace": """            it.rank.clone(),
            String::new(),""",
        "why": "SPEC §8.7: without the id the contest key is not a total order, and tied ranks fall back to the model's ops order",
    },
    {
        "name": "golden/declared-rank-failure-swallowed",
        "file": GOLDEN_RUNNER,
        "find": """    move_item_inner(pool, item.id.clone(), item.tier, Some(rank.to_string()), None)
        .unwrap_or_else(|e| panic!("[{case}] could not set declared rank {rank:?}: {e}"));""",
        "replace": """    let _ = move_item_inner(pool, item.id.clone(), item.tier, Some(rank.to_string()), None);
    let _ = case;""",
        "why": "a declared rank that cannot be applied would run the case against a board contradicting its own text — silently",
    },
    # ── The blocked-reason carry, at all four doors that write it ────
    # Pass 7's lesson made concrete: this one fix lives at four call
    # sites, and only the two a review happened to name were guarded.
    # A mutation per door, so "fixed everywhere" and "guarded
    # everywhere" cannot drift apart again.
    {
        "name": "items/single-unblock-drops-blocked-reason",
        "file": ITEMS,
        "find": """            "blocked_reason": if target_state == ItemState::Blocked {
                blocked_reason.clone()
            } else if current.state == ItemState::Blocked {
                current.blocked_reason.clone()
            } else {
                None
            },""",
        "replace": """            "blocked_reason": if target_state == ItemState::Blocked {
                blocked_reason.clone()
            } else {
                None
            },""",
        "why": "P2e BLOCKING-1 at the single-item door: undo of an unblock trips the migration-002 CHECK",
    },
    {
        "name": "items/batch-unblock-drops-blocked-reason",
        "file": ITEMS,
        "find": """                "blocked_reason": if target_state == ItemState::Blocked {
                    blocked_reason.clone()
                } else if current.state == ItemState::Blocked {
                    current.blocked_reason.clone()
                } else {
                    None
                },""",
        "replace": """                "blocked_reason": if target_state == ItemState::Blocked {
                    blocked_reason.clone()
                } else {
                    None
                },""",
        "why": "P2e BLOCKING-1 at the batch door — same fix, no guard until the pass-7 sibling audit",
    },
    {
        "name": "session/done-ending-drops-blocked-reason",
        "file": SESSION,
        "find": """                            "blocked_reason": if current.state == ItemState::Blocked {
                                current.blocked_reason.clone()
                            } else {
                                None
                            },""",
        "replace": """                            "blocked_reason": None::<String>,""",
        "why": "P2e BLOCKING-1 at the session door, reachable when an item is blocked mid-session and then finished",
    },
]


def run_suite() -> tuple[bool, list[str], str]:
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
        return proc.returncode == 0, ["<COMPILE ERROR — weak guard>"], out
    failed = re.findall(r"^\s{4}(\S+)$", out, re.MULTILINE)
    # The `failures:` block lists bare test paths; keep the plausible ones.
    names = sorted({f for f in failed if "::" in f})
    return proc.returncode == 0, names, out


def tracked_dirt() -> list[str]:
    """Paths with TRACKED modifications, which is the only dirt that matters.

    The refusal exists because this script rewrites tracked source files
    in place and restores them from memory; if a run is killed, recovery
    is `git checkout -- <file>`, which only works when there was nothing
    else uncommitted to confuse it. Untracked files cannot be clobbered
    by that, so refusing on them only made the gate unrunnable during
    normal work — which lowers the odds it is ever run.
    """
    proc = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        return ["<git status failed>"]
    return [
        line[3:]
        for line in proc.stdout.splitlines()
        if line.strip() and not line.startswith("??")
    ]


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
    dirty = tracked_dirt()
    if dirty:
        print(
            "FAIL: tracked files are modified. This script mutates files in place\n"
            "      and needs them clean so an interrupted run is recoverable with\n"
            "      `git checkout -- .` (and so its own edits are never committed).\n"
            "      Modified: " + ", ".join(dirty[:8]),
            file=sys.stderr,
        )
        return 2

    print("Baseline: the suite must pass before mutating.")
    passed, _, out = run_suite()
    if not passed:
        # Without the output the operator gets a verdict and no lead —
        # and a cargo failure here is as often environmental (a lock, a
        # transient link error) as a genuinely red suite.
        print("FAIL: baseline suite is already red — fix that first.", file=sys.stderr)
        print("\n".join(out.splitlines()[-40:]), file=sys.stderr)
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
            # Announce the wrongness while it exists. A reviewer or
            # editor that opens `m["file"]` in this window sees code
            # that is intentionally broken, and would have no way to
            # tell that from a genuine defect.
            with open(MARKER, "w", encoding="utf-8") as f:
                f.write(f"{m['name']} -> {m['file']}\n")
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(mutated)
            passed, failing, out = run_suite()
        finally:
            # Restore unconditionally.
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(original)
            if os.path.exists(MARKER):
                os.remove(MARKER)
        if not passed and not failing:
            # A red suite with no parseable test name is not evidence of
            # a guard — cargo can exit non-zero for reasons that have
            # nothing to do with the mutation (a file lock, a transient
            # link failure), and scoring that as "caught" would let a
            # genuinely unguarded line pass the gate on a flake.
            print(f"FAIL {m['name']}: INCONCLUSIVE — suite failed but named no test")
            print("\n".join(out.splitlines()[-20:]))
            survivors.append((m, "inconclusive"))
        elif not passed:
            # Every failing name, not just the alphabetically first one:
            # the gate cannot know which test is causally responsible,
            # and printing one implies a certainty it does not have.
            # That mattered — the round that added these guards used
            # this output to decide which of them were decoration.
            shown = ", ".join(failing[:4])
            extra = f" (+{len(failing) - 4} more)" if len(failing) > 4 else ""
            print(f"OK   {m['name']}: caught by {shown}{extra}")
            if failing[0].startswith("<COMPILE"):
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
