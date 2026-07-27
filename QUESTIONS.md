# QUESTIONS.md — Bay v0.2.0 revamp run

> Decisions made under uncertainty, with reasonable default applied,
> flagged for operator review on return. Append-only. Each entry has
> a lifecycle: OPEN → (CONFIRMED | UNWOUND | ABANDONED).

## Q01 — undo action grouping: (ts, type) heuristic vs. a transaction id

**Status:** CONFIRMED → resolved 2026-07-26. The operator authorized the
schema change via the 2026-07-26 directive (DECISIONS ADR-007);
migration 003 added `events.txn_id` (one uuid per `write_events`
transaction) and `undo_last_action` now groups by `txn_id` exactly —
the `(ts, type)` heuristic survives only as the fallback for legacy
pre-envelope rows (`txn_id IS NULL`, pinned so legacy and enveloped
rows can never co-group). Undo additionally skips `actor = 'system'`
transactions (VISION law 6). Mixed-type transactions — the case this
question existed for — now undo as one action (regression-tested;
I-21's recurrence trio builds on this).

**Context.** `undo_last_action` must treat a multi-event atomic operation
(swap = 2 ITEM_MOVED; batch = N ITEM_STATE_CHANGED / ITEM_DELETED) as ONE
undoable action, while a single command undoes exactly one event. The
event log records no transaction boundary, so "which events form one
action" must be inferred.

**Decision (default applied).** Group the most-recent non-LLM events that
share `(ts, type)`. Every atomic op writes its events in one
`write_events` tx with a single shared ts and a single type, so this
groups swaps and batches correctly. Keying on type as well as ts avoids
over-grouping a fast create+edit that share a ms (different types).

**Known limitation.** Two *distinct same-type* commands that land in the
SAME millisecond (e.g. two separate mark-done clicks) would be grouped
into one undo. This is **unreachable in production**: Bay is a single-user
GUI; two same-type commands can't be issued within 1 ms by hand, and the
only same-ms same-type producers are the atomic ops themselves (which ARE
one action). It only manifests in microsecond-fast automated test
sequences (handled in the tests by asserting no-crash rather than exact
revert count).

**The exact fix (operator-gated).** Add a `txn_id` column to `events`
(migration 003), stamp it once per `write_events` call, and group undo by
`txn_id`. This is precise (no heuristic) but is a schema change to the
append-only core table — flagged in AUTONOMY_CHARTER's foundational-blocker
list ("new event-log field → bank clean, ask"). Deferred to operator
review rather than self-authorized, since the heuristic is production-correct
and the schema change is the kind the charter asks me not to make casually.

**What would change it:** if a programmatic/scripted write path is ever
added (breaking the single-user-GUI assumption), or batch sizes/automation
make sub-ms same-type actions reachable, switch to `txn_id`.

---

## Question lifecycle (v7)

Every speculation resolves to one of:
- **CONFIRMED** → promote to DECISIONS.md ADR; remove inline
  `SPECULATION:` tag, add DECISION ref.
- **UNWOUND** → `git revert` the choice; note why here.
- **ABANDONED** → delete the code path; close this entry.

The continuous verifier flags speculations older than 5 modules and
assigns a target lifecycle state.
