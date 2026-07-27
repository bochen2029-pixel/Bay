# SPEC_AMENDMENT — Bay v0.2.0 doctrine reconciliation (Phase 3)

> Per AUTONOMY_CHARTER §5 + the `pre-arch-edit.sh` hook, edits to
> doctrine docs (CLAUDE.md, SPEC.md, PROMPTS.md) require either this
> SPEC_AMENDMENT.md at repo root OR a `SPEC:` commit tag. This file
> documents the v0.2.0 changes so the hook allows the edits. Remove
> this file after the operator reviews the v0.2.0 doctrine bump.

## What changed (v0.1.1 → v0.2.0-in-progress)

### Phase 1 (bug-fix)
- `llm/prompt.rs`: hardcoded `/ 5` and `/ 12` → `A_CAP`/`B_CAP` constants.

### Phase 2 (correctness layer)
- **2a Property tests**: 15 property tests added across the 6 critical
  modules (rank bounds/monotone/no-trailing-zero; projection determinism;
  swap atomicity + active-count preservation; cap enforcement A/B/inbox-C;
  write_events rollback any-failing-position; get_items_at(now)==live).
  `proptest = "1"` added as Cargo dev-dep (ADR-003).
- **2b DB-enforced invariants**: migration `002_invariants.sql` adds
  `events_no_update` + `events_no_delete` triggers (append-only now
  runtime-enforced) + items CHECK constraints (content 1..4096, rank
  non-empty, deleted IN (0,1), blocked=>reason). `PRAGMA user_version`
  1→2. `verify-schema.py` rewritten to load all migrations + expect v2.
- **2c Operator golden cases**: `contracts/golden/{projection,swap,rank,
  caps}.json` (all _status:proposed). `scripts/check-golden.py` CI check.
- **2d Type-level LLM firewall**: new `ProjectionEvent` enum (7 item-
  event variants only); `apply_event_to_projection` dispatches on it,
  not `EventType`. LLM events return `None` from `to_projection_event()`
  and structurally cannot reach the projection. The firewall is now
  "type system won't let you," not "match arm returns Ok(())".

### SPEC drift to reconcile (Phase 3)
- **§5.1 `bootstrap` return shape**: SPEC says `{items, settings,
  lanCapture}`; code returns `{items, settings}`. Frontend
  `BootstrapResult` matches code. DECISION: amend SPEC to match code
  (the frontend already calls `get_lan_capture_status` separately when
  needed; `lanCapture` in bootstrap is redundant). ADR in DECISIONS.md.
- **§6 module layout**: lists 9 files that don't exist (bootstrap.rs,
  swap.rs, db/projection.rs, capture/server.rs, capture/html.rs,
  capture/ip.rs, settings_file.rs, error.rs, tracing.rs). Actual tree
  is flatter (bootstrap in lib.rs; swap in commands/items.rs; capture
  as one capture/mod.rs + capture.html; settings in settings.rs).
  Reconcile §6 to match reality.
- **§6.2 frontend deps**: lists `@tauri-apps/plugin-global-shortcut`
  which is NOT in package.json (hotkey is Rust-side via
  `tauri-plugin-global-shortcut` in Cargo.toml; surfaces to JS only as
  `quick_capture_requested` event). Remove from §6.2.
- **§10.12 C virtualization**: resolved but unimplemented in v1. Phase
  4 (I-16) implements it. Update §10.12 to reflect the implementation
  or keep deferred per the increment plan.

### New SPEC sections to add
- **§4.4 DB-enforced invariants** (migration 002: triggers + CHECKs).
- **§4.5 Golden cases** (operator-owned ground truth; contracts/golden/).
- **§11 Property tests** (proptest; the non-LLM oracle).
- **§4.6 ProjectionEvent — type-level LLM firewall** (Phase 2d).
- **§9 I-15..I-27** increments (Phase 4/5/6 features — added as the
  phases ship; for now, the §9 "Post-v1.0 delivered" subsection gains
  a v0.2.0 entry).

### Version bumps
- CLAUDE.md: v1.6 → v1.7
- SPEC.md: v1.5 → v1.6
- PROMPTS.md: v1.3 → v1.4

### PROMPTS.md §2 additions (added as phases ship)
- I-15: Command palette (Phase 4)
- I-16: C-tier virtualization (Phase 4)
- I-17: Undo/redo stack (Phase 4)
- I-18: Audit-log search (Phase 4)
- I-19: Batch operations (Phase 4)
- I-20: LLM re-org proposals (Phase 5)
- I-21: Recurring tasks (Phase 5)
- I-22: LLM streaming (Phase 5)
- I-23..I-27: sync, multi-profile, theming, plugin surface, mobile (Phase 6)

Each increment prompt added when its phase ships, with the same
scope/out-of-scope/demo/verify structure as I-01..I-14.

---

## Second pass (v1.8 / v1.7 / v1.5) — Phase 4 + P2e + I-20

> Appended at the session-end consolidation after the run resumed
> (prior substrate hit quota). Reconciles the docs with what shipped
> AFTER the Phase 3 bump. Full operator accept/reject queue is in
> `REVIEW_QUEUE.md`.

### What changed (now reflected in the docs)
- **Phase 4 (I-15..I-19)** shipped: command palette, C-tier collapse
  (>50), undo (Ctrl+Z, batch-aware via `(ts,type)` action grouping),
  audit-log search, batch operations (atomic `batch_set_state` /
  `batch_delete`; no new event types).
- **P2e** fixed two BLOCKING bugs: undo-of-unblock now preserves the
  outgoing `blocked_reason` (ITEM_STATE_CHANGED payload semantics
  clarified — reason carried when `blocked` is on either side);
  `restore_item` is now cap-gated (JOINT_WRONG vs `caps.json` #12).
- **I-20** LLM re-org proposals: `analyze` may return `proposals`;
  `accept_suggestion(ops)` applies a human-accepted diff atomically and
  populates `LLM_SUGGESTION_ACCEPTED.resulting_event_ids`. The system
  prompt evolved "observe only" → "observe + optionally propose." The
  firewall is unchanged (LLM never writes).

### Doc edits made this pass
- CLAUDE.md → v1.8 (Current state rewritten).
- SPEC.md → v1.7 (§2.10, §3.5 batch ops, §3.6 undo, §4.3
  resulting_event_ids + ITEM_STATE_CHANGED blocked_reason, §5.1 IPC
  table + ReorgOp, §8.2 prompt, §8.7 re-org accept path, §9 delivered,
  §10.5/§10.12 amendment notes).
- PROMPTS.md → v1.5 (full I-15..I-20 increment prompts; I-21..I-27 stay
  a forward-reference to `FUTURE_WORK.md`).

### Operator decisions to ratify (see REVIEW_QUEUE.md)
- System prompt "observe → observe+propose" reads as the doctrine's
  preserved v2 re-org surface, not a firewall change. Confirm.
- I-21 (recurring) + the `events.txn_id` schema change (QUESTIONS Q01)
  were DEFERRED to operator review rather than self-authorized.

Remove this file once the operator has reviewed both passes (Phase 3 +
this one).

---

## Third pass (v1.9 / v1.8 / v1.6) — the v0.3 "Execution" run

> Appended 2026-07-26. Authorized by the operator directive recorded in
> DECISIONS ADR-007 ("proceed at your best recommendation — most
> ambitious, highest quality"), scoped by VISION.md §8.

### What shipped (now reflected in the docs)
- **Golden RUNNER**: contracts/golden cases EXECUTE under cargo test
  (SPEC §4.7). Three defective *proposed* caps cases (#5/#6/#8)
  corrected in place with `_corrected` notes — **operator freeze still
  pending**; they were never frozen, so this is a proposal edit, not a
  charter violation.
- **Migration 003 event envelope v2** (SPEC §4.0): txn_id, actor,
  origin, device_id, schema_ver, prev_hash + meta table + SHA-256 chain
  + boot verification. New runtime dep `sha2` (ADR-008, SPEC: tag).
- **QUESTIONS Q01 CLOSED**: undo groups by txn_id (SPEC §3.6); legacy
  (ts,type) fallback fenced by `txn_id IS NULL`; system + session
  transactions are not undo targets.
- **I-21 recurring** (migration 004): RRULE subset, spawn-on-done in
  one transaction, Inbox overflow for blocked-parent completions.
- **Execution core** (migrations 005/006): first_step, Today overlay
  (cap 3), day open/close/roll, focus sessions (second projection
  table), FocusBar + Today lane.
- **Mirror v1** (SPEC §12): deterministic statistics, zero LLM.

### Doctrine edits made this pass
- CLAUDE.md → v1.9: **four new principles** (§7 caps bind flow; §8
  cheap starts + never interrupts; §9 deterministic non-shaming mirror;
  §10 system acts only on a human-set timer). Data model updated
  (envelope columns, recurrence/first_step/today_on, sessions table).
  Event table extended. Cut-list: recurring struck through as promoted
  and shipped; four sharpened refusals added (gamification,
  auto-planning, estimates, subtasks-again). Current state rewritten;
  prior v0.2.0 state preserved below it.
- SPEC.md → v1.8: new §4.0 envelope, §4.7 golden runner, §3.7 Today +
  rituals, §3.8 sessions, §12 Mirror; §3.6 undo regrounded on txn_id;
  §4.3 nine new payloads; §4.6 firewall None-arm now two classes; §5.1
  fourteen command rows; §6 module tree; §9 delivered log.
- PROMPTS.md → v1.6: six shipped increment prompts (V-01, V-02, I-21,
  V-03, V-04, V-05); forward reference narrowed to I-22..I-27;
  principle count six → ten.

### Operator decisions still outstanding
1. **Freeze the corrected golden cases** (contracts/golden/caps.json
   #5/#6/#8) — or amend them differently if my doctrine reading is
   wrong. This is the highest-value review item: they are operator
   ground truth.
2. **Ratify the two new doctrine laws** as written (§7 flow caps, §10
   system-actor timer). Both are implemented; both are reversible.
3. VISION §7 items NOT built and still gated: T4 (LLM Today-draft), T5
   (calendar ICS read), T8 (sync as log replication).

Remove this file once the operator has reviewed all three passes.
