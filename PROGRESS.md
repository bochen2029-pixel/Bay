# PROGRESS.md — Bay v0.2.0 revamp run

> Append-only narrative ledger. Every 15–30 min + every save point.
> Last 5 entries give post-compaction continuity. Read by bootstrap.

## 2026-06-17T00:00:00Z — Run start

- Read methodology refs (solo-enterprise-architect v6+v7, v6 treatise,
  v7 patch, chunker README). Extracted transferable disciplines.
- Did brownfield reverse-arch pass earlier: 5 explore agents read
  every tracked file; personally read all 7 required docs
  (index.html, LICENSE, package.json, PROMPTS.md, README.md, SPEC.md,
  CLAUDE.md) + the methodology references.
- Identified 3 problem classes: (1) correctness asserted not enforced
  (no property tests, no DB triggers, no golden cases beyond rank);
  (2) doctrine drift (SPEC §6/§6.2/§5.1/§10.12); (3) "above and
  beyond" mostly absent (no palette, no undo/redo, no audit search,
  LLM accept-path is no-op).
- User chose: Full v2 modernization + Heavy overnight-capable +
  archive-and-diff doctrine handling.
- Wrote master plan (8 phases, 30–40 wall-clock hours). Approved.
- Phase 0 in progress: wrote AUTONOMY_CHARTER.md, RUN_STATE.md,
  TASKLIST.md, DECISIONS.md (4 ADRs), QUESTIONS.md, BLOCKERS.md,
  VERIFIED.md, this file. Next: run-metrics.jsonl, .claude/ scaffold,
  .run-lock, git tag, baseline test run, /preflight.

## 2026-06-17T00:30:00Z — Phase 0 close (baseline green)

- Wrote run-metrics.jsonl (initial record), .run-lock (heartbeat),
  marrow.lock (run-start substrate snapshot), .gitignore additions
  (.run-lock + .chunks/ ephemeral).
- Wrote .claude/ harness: 6 hooks (pre-compact-brief,
  session-start-bootstrap, stop-completion-gate, post-edit-tests,
  pre-arch-edit, subagent-validate), 3 subagent defs (implementer,
  verifier, test-author), 13 skills (bootstrap, save, verify,
  close-run, stuck, speculate as core; checkpoint, runway, calibrate,
  audit, refactor-impact, cold-start-check, inspect as on-demand ref).
- Ran baseline reality check (bootstrap step 5): all four gates GREEN.
  cargo build warning-clean (57s); cargo test 91/91; pnpm build clean
  (384 modules, 1.28s); pnpm test 85/85; store-logic 55/55.
  verify-schema.py deferred (needs live bay.db; Phase 2b).
- Phase 0 save point: committing spine + tagging
  autonomous-run-2026-06-17-start. Next: Phase 1 (bug-fix pass).

## 2026-06-17T00:45:00Z — Phase 1 close (1 real bug fixed)

- Phase 1 audit: of 8 candidate "bugs" in the plan, only 1 was a real
  code bug. v0.1.1's cleanup pass was thorough.
- FIXED: llm/prompt.rs hardcoded `/ 5` and `/ 12` replaced with
  A_CAP/B_CAP constants. cargo build clean; cargo test llm 18/18.
- Confirmed NOT bugs (documented design choices / correct behavior):
  restore_item reusing item_created (documented at items.rs:462-466),
  set_item_state done-to-active at cap (correct; items.rs:329-343),
  store onItemUpdated idempotency (correct; same-ms double-edit
  structurally impossible), HotkeyInput listener cleanup (correct React).
- Deferred to Phase 3 (SPEC drift, not code bug): bootstrap return
  shape {items, settings} vs SPEC 5.1 {items, settings, lanCapture}.
- Sweep clean: no dbg!/println! debug / console.log / TODO/FIXME/XXX/
  HACK in src. The eprintln!/console.error calls are operational
  diagnostics (correct idiom).
- Phase 1 save point: committing the prompt.rs fix. Next: Phase 2a
  (property tests — the load-bearing correctness layer).

## 2026-06-17T01:30:00Z — Phase 2a close (15 property tests, non-LLM oracle)

- Added proptest = "1" as Cargo dev-dep (ADR-003).
- 15 property tests across the 6 critical modules:
  - rank_between: 8 (bounds, monotone front/end/midpoint, no-trailing-zero)
  - apply_event_to_projection + rebuild: 2 (THE projection-determinism
    property; get_items_at(now)==live)
  - swap_move + cap enforcement: 4 (cap A/B/inbox-C; swap atomicity +
    active-count preservation)
  - write_events rollback: 1 (any failing position -> full rollback)
- cargo test 106/106 (up from 91). cargo build warning-clean.
- The proptest! macro's fn-form had edge cases (zero-arg tests, doc
  comments); used the closure form proptest!(|...| {...}) inside plain
  #[test] fns — unambiguous, compiles reliably across proptest versions.
- Phase 2a save point: committing. Next: Phase 2b (DB-enforced
  invariants — migration 002 with CHECKs + append-only trigger).

## 2026-06-17T02:00:00Z — Phase 2b close (DB-enforced invariants)

- Migration 002_invariants.sql: events_no_update + events_no_delete
  triggers (append-only now runtime-enforced) + items CHECK
  constraints (content length, rank non-empty, deleted boolean,
  blocked=>reason). Table-rebuild pattern for items (ALTER TABLE
  ADD CONSTRAINT doesn't exist in SQLite).
- Wired into db/mod.rs MIGRATIONS const; user_version now 1->2.
- 4 new tests prove the trigger blocks UPDATE/DELETE, allows INSERT,
  and CHECKs reject invalid rows (deleted=2, empty content, empty
  rank, blocked-without-reason).
- verify-schema.py rewritten: loads expected CREATEs from ALL
  migrations (later overrides earlier), expects v2, includes triggers.
- cargo test 110/110 (up from 106). cargo build warning-clean.
- Subtlety: migration SQL must NOT contain BEGIN/COMMIT — the runner
  in db/mod.rs already wraps each migration in its own tx. Caught by
  "cannot start a transaction within a transaction" error on first
  run; fixed by removing the inner BEGIN/COMMIT.
- Phase 2b save point: committing. Next: Phase 2c (operator golden
  cases — contracts/golden/*.json).

## 2026-06-17T03:30:00Z — Phase 3 close (doctrine reconciliation)

- Archived v1.6/v1.5/v1.3 to archive/CLAUDE_v1.6.md, SPEC_v1.5.md,
  PROMPTS_v1.3.md.
- Bumped: CLAUDE.md v1.7, SPEC.md v1.6, PROMPTS.md v1.4.
- SPEC drift reconciled:
  - 5.1 bootstrap return shape: {items,settings,lanCapture} ->
    {items,settings} (code is canonical; ADR-005).
  - 6 module layout: reconciled with actual flatter tree (bootstrap
    in lib.rs; swap in commands/items.rs; capture as one mod.rs +
    capture.html; no error.rs/tracing.rs/settings_file.rs).
  - 6.2 frontend deps: removed @tauri-apps/plugin-global-shortcut
    (Rust-side; surfaces as quick_capture_requested event); added
    @tauri-apps/plugin-dialog (was missing from spec).
- New SPEC sections: 4.4 DB-enforced invariants, 4.5 Golden cases,
  4.6 ProjectionEvent type-level LLM firewall, 11 Property tests.
- New 9 subsection: Post-v1.1 delivered (v0.2.0 correctness layer).
- CLAUDE.md Current state refreshed to v0.2.0-in-progress.
- DECISIONS.md: ADR-005 (bootstrap shape), ADR-006 (ProjectionEvent
  firewall).
- SPEC_AMENDMENT.md left in place for operator review of the doctrine
  bump (per charter §5 protocol).
- Phase 3 save point: committing. Next: Phase 4 (above-and-beyond UX
  — I-15 command palette first).

## 2026-06-17T04:00:00Z — Phase 4 I-15 command palette

- New component: src/components/CommandPalette.tsx. Opens on
  Cmd/Ctrl+K (global document listener) or the ⌘K button in TopBar.
- Fuzzy-search across: navigate (board/calendar/timetravel/archive/
  settings), create (quick-capture to inbox; add to A/B/C switches to
  board), actions (run analyze, open archive), jump-to-item (top 50
  items by tier, selecting sets selectedItemId + switches to board).
- Reuses existing store actions + invoke paths; NO new write paths,
  NO new event types, NO new backend commands. LLM firewall + caps
  hold (create routes through create_item; restore through restore_item).
- Arrow-key navigation, Enter to run, Esc to close, mouse hover.
- pnpm build clean (385 modules); pnpm test 85/85.
- I-15 save point: committing. Next: I-16 (C-tier virtualization).

## 2026-06-17T04:15:00Z — Phase 4 I-16 C-tier collapse

- BayColumn (App.tsx): C-tier collapse per SPEC §10.12. At >50 visible
  items in C, default-collapse to first 50 + "Show all N items (M
  more)" button. Inbox/A/B excluded (bounded by caps/triage). Collapsed
  state is local useState (session-scoped, not persisted — matches the
  done-reveal pattern).
- The SortableContext still gets the full visibleIds list (so drag
  works on the collapsed tier); only the rendered strips are sliced.
- .bay-show-all CSS added (dashed border, matches .bay-show-done style).
- pnpm build clean (385 modules); pnpm test 85/85.
- I-16 save point: committing. Next: I-17 (undo/redo stack).

## 2026-06-17T04:45:00Z — Phase 4 I-17 undo (Ctrl+Z)

- New backend command: undo_last_action (commands/events.rs). Finds the
  most recent non-LLM event; if it's an ITEM_MOVED and the event before
  it shares ts + is also ITEM_MOVED, treats them as a swap pair (undo
  both). Otherwise undoes the single most-recent event. Compensation
  per type: CREATED->DELETED, EDITED->edit-back, MOVED->move-back,
  STATE_CHANGED->state-back, DATE_SET->date-back, DELETED->RESTORED,
  RESTORED->DELETED. LLM events skipped (advisory-only).
- Compensating drafts appended in one write_events tx (atomic). LIFO
  order so swap pairs unwind correctly.
- The undo IS itself an event in the log (auditable; redo = undo the
  undo, not yet wired as separate command).
- Frontend: Ctrl/Cmd+Z triggers undo_last_action. Doesn't intercept
  when typing in input/textarea (native undo preserved). Backend emits
  item_updated/item_deleted; TauriEventBridge refreshes store.
- Design subtlety resolved: "action" = single event OR swap pair (two
  same-ts ITEM_MOVED). NOT "all same-ts events" — that over-grouped
  fast create+edit sharing a ms. The swap-pair detection (ITEM_MOVED
  preceded by same-ts ITEM_MOVED) is the only legitimate multi-event
  action; all other commands use write_event (singular).
- 8 new undo tests (create/edit/move/state/delete/LLM-skip/audit/
  nothing-to-undo). cargo test 121/121. pnpm build clean. pnpm test
  85/85.
- I-17 save point: committing. Next: I-18 (audit-log search).

## 2026-06-17T05:15:00Z — Phase 4 I-18 audit-log search

- New backend command: search_events (commands/events.rs). Pure-Rust
  filter: case-insensitive substring on payload JSON + event_type/item/
  date/limit filters. FTS5 deferred (heavier migration; pure-Rust fine
  for single-user local-first logs in the thousands).
- New frontend view: AuditLogView. Search input (Enter to search),
  event-type select, item-id input, result list (id/ts/type/item/
  payload summary). Wired into App.tsx as the "audit" view + view
  switcher + command palette.
- 6 new search tests (substring/event-type/item-id/case-insensitive/
  empty-query/limit). cargo test 127/127. pnpm build clean. pnpm test
  85/85.
- I-18 save point: committing. Next: I-19 (batch operations).

## 2026-06-17T18:30:00Z — Run resumed (substrate handoff) + I-19 close

- **Substrate handoff:** the prior substrate (GLM-5.2) exhausted its
  quota mid-I-19, having written the batch backend (batch_set_state /
  batch_delete in commands/items.rs) but leaving it uncommitted,
  unregistered, untested, and with no frontend. Run resumed by Claude
  (Opus). Charter honored as operator intent; this entry is the audit
  trail of the handoff.
- **Bug found + fixed (batch cap enforcement):** the uncommitted
  batch_set_state_inner checked count_active_in_tier inside the
  write_events build closure, but write_events builds ALL drafts before
  applying any — so every item saw the pre-batch count and a batch that
  activates more items than free slots would overflow the cap. Fixed
  with incremental projected counters (active_a/active_b). Regression
  test batch_set_state_to_active_rolls_back_on_cap_overflow pins it.
- **affected_ids accuracy:** both inner fns now derive affected_ids from
  the returned events (NO_OP items produce no event → excluded), rather
  than echoing the raw input list.
- **Commands registered** in lib.rs invoke_handler (batch_set_state,
  batch_delete).
- **Undo generalized for batch-undo:** undo_last_action grouped only a
  single event or a swap-pair (two same-ts ITEM_MOVED), so a batch of N
  same-ts events would undo only one. Generalized to group the
  most-recent events sharing (ts, type) — subsumes the swap-pair special
  case, delivers batch-undo, and avoids over-grouping create+edit (those
  differ in type). Added undo_batch_delete / undo_batch_set_state /
  undo_swap tests (the swap-undo path was previously untested).
- **Frontend:** multi-select in the store (selectedIds + lastSelectedId,
  toggleSelected/selectRangeTo/clearSelected, cleanup on delete);
  per-strip checkbox with shift-click range; BatchActionBar (mark done /
  mark active / two-step-confirm delete; CAP_EXCEEDED surfaced; Esc
  clears). Wired into Board. New CSS (.strip-select, .strip.is-selected,
  .batch-bar*). Batch-delete suppresses the single-item undo toast (the
  whole batch is Ctrl+Z-undoable).
- Tests: cargo 139/139 (+12), vitest 90/90 (+5 BatchActionBar), build
  clean both sides, store-logic smoke green.
- I-19 save point: committing. Phase 4 (I-15..I-19) complete. Next: P2e
  (cold-context two-pass verification — the prior run dispatched it but
  the verifiers never returned; the non-LLM oracle gate is green).

## 2026-06-17T19:00:00Z — P2e two-pass verification COMPLETE (2 BLOCKING bugs fixed)

- Re-dispatched 2 cold-context `verifier` subagents (the prior run's
  never returned). They found 2 BLOCKING bugs the 143-test suite missed
  — exactly the second-pass payoff:
  - **BLOCKING-1:** undo of an unblock (blocked→active) crashed the
    migration-002 CHECK (set state=blocked with null reason). Fixed:
    set_item_state_inner / batch_set_state_inner now preserve the
    outgoing blocked_reason in the event payload. (Pre-existing since
    I-17; masked because the only undo-state test used active→done.)
  - **BLOCKING-2:** restore_item had no cap check — archive-restoring an
    active item into a full A/B exceeded the cap (JOINT_WRONG vs
    caps.json case 12). Fixed: restore_item_inner cap-gates active
    restores. (Pre-existing since v0.1.x.)
- Both fixed + regression-tested (4 new tests). Corrected the misleading
  undo cap doc-comment (undo-of-delete is cap-safe by construction).
- STRUCTURAL: (ts,type) undo grouping over-groups two distinct same-type
  same-ms actions — production-unreachable (single-user GUI); logged as
  QUESTIONS Q01 with the txn_id fix deferred to operator (schema change).
- COSMETIC: added dedup_preserving_order at the batch boundaries.
- Gap surfaced: check-golden.py only checks existence, doesn't EXECUTE
  golden cases (why the restore JOINT_WRONG slipped). Specific cases now
  pinned by Rust tests; a generic golden runner is a P2c follow-up.
- Also fixed a pre-existing unused-var warning in an I-18 test.
- cargo 143/143 (+4), warning-clean; vitest 90/90; builds clean.
- P2e save point: committing. P2e DONE. Next: Phase 5 (I-20 LLM re-org
  accept-path, I-21 recurring tasks, I-22 LLM streaming).

## 2026-06-17T20:00:00Z — Phase 5 I-20 LLM re-org accept-path (doctrine capstone)

- The schema field LLM_SUGGESTION_ACCEPTED.resulting_event_ids has been
  [] since v1; the doctrine preserved it for exactly this. Now wired.
- Backend:
  - parse.rs: parse_analysis extracts an optional `proposals` array
    (move / done / active) alongside observations; validates item_ids ∈
    known + valid to_tier (drops bad ones). 4 parse tests.
  - llm.rs analyze: returns + logs proposals in the suggestion event.
  - llm.rs accept_suggestion(ops): with ops, apply_reorg_inner applies
    the human-accepted diff as ONE atomic write_events tx — ITEM_MOVED /
    ITEM_STATE_CHANGED events + the LLM_SUGGESTION_ACCEPTED event with
    resulting_event_ids populated. Cap enforced on FINAL A/B active
    counts (so a net-zero demote+promote swap is allowed; a true
    overflow rolls back). resulting_event_ids predicted via MAX(id)+k
    (safe: events is append-only, no id gaps). 5 accept-reorg tests
    (atomic apply, resulting_event_ids match actual events, cap rollback,
    net-zero swap succeeds, unknown suggestion → EVENT_NOT_FOUND).
  - prompt.rs: SYSTEM_PROMPT now invites optional proposals (the v2
    surface) while keeping the firewall framing (suggestions only).
- Frontend: AnalyzePanel renders proposals as a selectable diff
  (checkboxes, per-op rationale); adaptive primary button — "Apply N
  changes" (accept with selected ops) when any selected, else "Mark
  reviewed" (observations-only accept); CAP_EXCEEDED surfaced inline.
  3 AnalyzePanel vitest specs.
- Firewall INTACT: LLM proposes (parsed, never applied), human selects +
  accepts, deterministic tier writes under cap enforcement. ProjectionEvent
  firewall still blocks LLM events from the projection.
- cargo 152/152 (+9), warning-clean; vitest 93/93 (+3); builds clean.
- I-20 save point: committing. Next: I-21 (recurring tasks).

## 2026-06-17T20:30:00Z — Session-end consolidation (clean checkpoint)

- **Decision: stop feature work at the I-20 boundary; consolidate.**
  Began I-21 (wrote + unit-tested a dependency-free recurrence module),
  then realized recurring-task completion is a MIXED-TYPE atomic action
  (STATE_CHANGED + CREATED + RECURRED) whose correct undo needs a
  transaction-id on `events` — the schema change deferred to operator
  review in QUESTIONS Q01. Building I-21 on that deferred foundation
  would ship broken recurrence-undo or force an autonomous schema change
  to the append-only core at the tail of a long run. Per the charter
  ("a 2-hour run stopping at the right decision beats an 8-hour run on a
  wrong foundation") + the reversibility gate, stopped clean. Backed out
  the untracked recurrence.rs (design preserved in FUTURE_WORK.md).
- Consolidation deliverables:
  - Doctrine archive-and-diff: archived CLAUDE v1.7/SPEC v1.6/PROMPTS
    v1.4; bumped to v1.8/v1.7/v1.5. CLAUDE "Current state" rewritten;
    SPEC/PROMPTS reconciled (resulting_event_ids populated; LLM scope
    observe→propose; batch ops; I-15..I-20 increment prompts).
  - README v0.2.0 features (palette, undo, batch, audit, re-org).
  - FUTURE_WORK.md (I-21 full design + the txn_id dependency; I-22;
    Phase 6 with doctrine notes; P7 release).
  - REVIEW_QUEUE.md (operator accept/reject queue, cheapest-to-verify
    first, revert commands ready, complacency canary).
  - memory project_bay_state.md + MEMORY.md index updated.
- Run NOT fully closed: .run-lock stays; no v0.2.0 tag (I-21+ remain).
  This is a session checkpoint + handoff, not a release.
- Final gates: cargo 152/152 warning-clean, vitest 93/93, pnpm build
  clean, store-logic smoke green.

## 2026-06-17T21:00:00Z — Hotfix: `tauri dev` couldn't pick a binary

- Operator ran `pnpm tauri dev` → exit 101: "`cargo run` could not
  determine which binary to run. available binaries: bay,
  rank_fixture_gen." Root cause: the crate has 2 bins (the `bay` app +
  the `rank_fixture_gen` dev tool from src/bin/, added in the v0.1.1
  cleanup) and no `default-run`, so `tauri dev`'s bare `cargo run` is
  ambiguous. Latent since the 2nd bin landed; surfaced on first GUI run.
- Fix: `default-run = "bay"` in `[package]` (src-tauri/Cargo.toml) — the
  exact key cargo's error recommends. `cargo run --bin rank_fixture_gen`
  still works explicitly.
- Verified: `cargo metadata` → `default_run: "bay"`; `cargo build --bin
  bay` clean. The ambiguity is now structurally impossible.

## 2026-07-26 — Run resumed under operator directive (v0.3 execution run)

- Operator: "proceed at your best recommendation — most ambitious and
  most aggressive while maintaining highest quality." ADR-007 records
  the dispositions: REVIEW_QUEUE #1 (prompt observe→propose) RATIFIED;
  envelope migration 003 AUTHORIZED (supersedes the txn_id-only plan;
  Q01 closes when it lands); VISION T1/T2/T3/T6/T7/T9 in scope this
  run; T4 (LLM Today draft) / T5 (ICS) / T8 (sync) remain gated.
- VISION.md committed — non-doctrine first-principles design source
  (three loops, ten laws, event taxonomy v2, tension ledger, phasing).
- STOP_ACK deleted (its own instruction for resume); .run-lock
  refreshed (run 2026-07-26-T00-00, substrate Claude Fable 5).
- TASKLIST: P5a golden runner / P5b envelope 003 + undo-txn / P5c
  execution core added.
- Bootstrap reality check: cargo 152/152 cold, tree clean at 709be5d.
