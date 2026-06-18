# FUTURE_WORK — Bay v0.2.0 remaining scope

> Written 2026-06-17 at the I-20 consolidation boundary. The v0.2.0
> revamp shipped the correctness layer (P0–P3 + P2e), all of Phase 4
> (I-15..I-19), and the first Phase 5 increment (I-20). This file scopes
> what's left so the next session picks up cleanly. Each item follows the
> PROMPTS.md §2 increment rhythm + archive-and-diff doctrine discipline.

## Status snapshot

| Phase | Increments | State |
|---|---|---|
| P0–P2 | spine, bug-fix, correctness layer | ✅ done |
| P2e | cold-context two-pass verification | ✅ done (2 BLOCKING bugs fixed) |
| P3 | doctrine reconciliation | ✅ done (v1.7) + this pass (v1.8) |
| P4 | I-15..I-19 (palette, C-collapse, undo, audit search, batch) | ✅ done |
| P5 | I-20 LLM re-org accept-path | ✅ done |
| P5 | I-21 recurring, I-22 streaming | ⏳ remaining |
| P6 | I-23..I-27 (sync, multi-profile, theming, plugins, mobile) | ⏳ remaining |
| P7 | release (tag v0.2.0, runbooks) | ⏳ after P5/P6 land |

Gates at this boundary: cargo 152/152, vitest 93/93, both builds clean,
warning-clean.

---

## The blocker that gates I-21 — undo transaction grouping (QUESTIONS Q01)

`undo_last_action` groups "one action" by `(ts, type)` of the most-recent
events. This is **production-correct today** because every multi-event
atomic operation Bay currently writes is *single-type*:
- swap → 2 `ITEM_MOVED`
- batch → N `ITEM_STATE_CHANGED` or N `ITEM_DELETED`

and two distinct same-type single-item commands can't land in the same
millisecond in a single-user GUI.

**I-21 breaks this assumption.** Completing a recurring item is a
*mixed-type* atomic action — `ITEM_STATE_CHANGED` (parent→done) +
`ITEM_CREATED` (next instance) + `ITEM_RECURRED` (link), one tx, one ts.
`(ts,type)` grouping would undo only one of the three, orphaning the
spawned child. So **correct recurrence-undo requires a real transaction
boundary in the log.**

### The fix: a `txn_id` column on `events`

- `migration 003_event_txn.sql`: `ALTER TABLE events ADD COLUMN txn_id
  TEXT;` (nullable; legacy rows stay NULL). Bump `user_version` 2→3. Note
  `ALTER TABLE ADD COLUMN` is DDL — it does NOT fire the migration-002
  `events_no_update`/`events_no_delete` row triggers.
- `db::write_events`: generate one `txn_id` (uuid) per call; pass to
  `events::append_event`; every draft in the call shares it.
- `db::events::append_event`: add the column to the INSERT.
- `undo_last_action`: read the last non-LLM event's `txn_id`; group by it
  (`WHERE txn_id = ?`). Fallback for NULL (pre-003 events): the single
  event by id. `parse_event_row` doesn't need to read `txn_id` (only the
  grouping query's WHERE uses it).
- Update: migration-version test (2→3), `scripts/verify-schema.py`,
  resolve QUESTIONS Q01.

This is **strictly better** than `(ts,type)`: it groups exactly one tx,
never over- or under-groups, and the existing undo tests (swap, batch,
single, create+edit-same-ms) all still pass because same-tx ⇒ same
txn_id. It touches `write_events` (critical module #1) and `events` (the
append-only core), so it warrants a careful two-pass + the operator's
awareness — hence it was deferred rather than self-authorized at the tail
of a long autonomous run.

**Do this first; then I-21 is undo-correct.**

---

## I-21 — Recurring tasks

Doctrine: on the "Cut from v1" list but named a sanctioned v2 candidate.

### Data model
- `items.recurrence TEXT NULL` (migration 003, alongside `txn_id` or a
  separate 004). Carried on `ITEM_CREATED` payload so a spawned instance
  keeps recurring; surfaced on the `Item` struct (Rust) + `Item` Zod
  (TS) so the UI can badge it. Add to `row_to_item` + the 4 item SELECTs
  + the manual `Item` literal in `commands/events.rs` (undo emit).
- Recurrence string = minimal RFC 5545 RRULE subset:
  `FREQ=DAILY|WEEKLY|MONTHLY[;INTERVAL=n]`. A dependency-free
  `domain/recurrence.rs` parses it and computes `next_after(base_ms)`:
  daily/weekly are ms offsets; monthly uses Howard Hinnant's
  days↔civil-date algorithms (`days_from_civil` / `civil_from_days`)
  with short-month day-clamping (Jan 31 + 1mo → Feb 28/29). This module
  was prototyped + unit-tested this session (parse round-trip, leap
  years, month rollover, day clamping) and backed out pending the txn_id
  decision; reconstruct it from this spec — it's ~150 lines and the date
  algorithms are standard/public.

### Events (two new types)
- `ITEM_RECURRENCE_SET` — projection event; sets/clears
  `items.recurrence`. Set via a new `set_item_recurrence` command (the
  "make this task repeat" UX).
- `ITEM_RECURRED` — relationship/audit. On marking a recurring item
  **done**, `set_item_state_inner` emits, in ONE tx: `ITEM_STATE_CHANGED`
  (parent→done) + `ITEM_CREATED` (child: same content/tier/recurrence,
  `due_at = recurrence.next_after(parent.due_at ?? now)`, rank at end of
  tier) + `ITEM_RECURRED` `{parent_id, child_id, next_due_at}`.
  `to_projection_event()` returns `None` for `ITEM_RECURRED` (the child's
  existence is carried by the `ITEM_CREATED`; the link is audit only) —
  update the firewall doc-comment so `None` reads as "no projection
  effect" (LLM advisory events + this link), the firewall (no
  `ProjectionEvent::Llm*`) still holding.

  **Alternative single-event design** (if the operator prefers to avoid
  the txn_id change): make `ITEM_RECURRED` a *projection* event whose
  `apply` does both mutations (parent→done + insert child, child data in
  the payload). Then completing a recurring item is one event → undo-safe
  under `(ts,type)` with a custom compensation (parent→active +
  soft-delete child). Trade-off: the parent's done-transition is an
  `ITEM_RECURRED` rather than an `ITEM_STATE_CHANGED`, so it's less
  uniform but needs no schema change. Pick one; the txn_id path is
  cleaner long-term and also resolves Q01.

### Cap handling
Completing a recurring **active** item is net-zero (parent leaves active,
child enters) so the child fits the parent's tier. Edge: a **blocked**
recurring item completed into a full A/B would push the child over cap —
route the child to **Inbox** (doctrine-consistent overflow) so marking
done never fails. Compute in the command's build closure.

### Frontend
- `Strip` overflow menu: "Repeat ▸ Daily / Weekly / Monthly / None" →
  `set_item_recurrence`. A 🔁 badge on recurring strips.
- The spawned child arrives via the existing `item_created` event →
  store `onItemCreated` (no new store wiring).

### Tests
recurrence math (done in the prototype), `apply_item_recurrence_set`,
create-with-recurrence, done-spawns-child (+ correct due date + cap
overflow→inbox), undo of a recurrence completion (the txn_id payoff),
firewall test for the new event type(s).

---

## I-22 — LLM streaming

`OpenAiCompatClient.chat` does a single request. Add a streaming variant
(`stream: true`, SSE) and an `analyze_progress` `streaming` stage that
emits partial observations as they arrive. Keep the ≤ token budget.
Optional/nicety; the parse path stays strict (buffer, then parse the
final JSON). No doctrine impact.

---

## Phase 6 — full v2 modernization (operator sign-off recommended)

These are genuinely multi-session and several touch doctrine — review
approach with the operator before building.

- **I-23 sync** — re-litigates "no cloud sync" (CLAUDE.md "Cut from v1").
  The event log is already a clean CRDT seed (append-only, monotonic ids,
  ts-ordered); LWW per event id. Default OFF; local-folder / WebDAV /
  self-hosted relay backends. Needs an event-log field (`origin_device`)
  — same schema-change gate as txn_id. Large; decompose to I-23a/b/c.
- **I-24 multi-profile** — separate `bay.db` + `settings.json` per
  profile dir; switch via tray/palette; hotkey routes to the active
  profile. Mostly additive; tractable.
- **I-25 theming** — requires an explicit doctrine amendment: CLAUDE.md
  "Cut from v1" lists "Dark-mode toggles, theme customization." The plan
  sanctions user themes as a v2 surface (default stays system theme; no
  icon packs). Make CSS custom properties user-overridable. Small once
  the doctrine amendment is approved.
- **I-26 plugin surface** — riskiest. Event-subscription + palette-action
  API, Tauri capability sandbox. Ship an MVP (event sub + palette action)
  and defer view-tab plugins. Charter gates this heavily.
- **I-27 mobile companion** — generalize the LAN capture page into a PWA:
  read-only board + capture + mark-done. Reuses `axum`; adds
  `GET /board`, `POST /item/<id>/state`. LAN-trust holds.

---

## P7 — release

After the P5/P6 work that's going to ship lands: regenerate
`marrow.lock`, finalize `run-metrics.jsonl`, write runbooks (verify
projection, sync-conflict recovery, disable v2 surfaces), bump README,
tag `v0.2.0`.
