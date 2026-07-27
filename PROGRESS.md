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

## 2026-07-26 — P5a: golden RUNNER — the exists-vs-executes gap closed

- src-tauri/src/golden_runner.rs (cfg(test)): executes projection.json
  (7 cases), swap.json (6), caps.json (12) against the real *_inner
  command functions + projection on fresh in-memory DBs, in cargo test.
  Unknown op types / expectation keys PANIC — no silent skips (silent
  skips are how JOINT_WRONGs hide). rank.json already executes via the
  fixture parity suites.
- Execution design immediately surfaced 3 DEFECTIVE proposed caps
  cases (#5, #6, #8): ops/expectations contradicted their own names AND
  frozen doctrine (blocked/done do not count against caps; a failed
  transition mutates nothing). Corrected as _status:proposed edits with
  _corrected annotations; operator freeze requested on return.
- Gates: cargo 155/155 (152 + 3 runner tests), build warning-clean,
  check-golden.py OK, 4.07s test wall.

## 2026-07-26 — P5b(1): migration 003 — event envelope v2

- migrations/003_event_envelope.sql: ALTER ADD txn_id / actor / origin /
  device_id / schema_ver / prev_hash (all nullable; legacy rows valid),
  idx_events_txn, meta(key,value) table. user_version 2 -> 3. ALTER not
  rebuild — events is the source of truth; charter forbids DROP.
- write path (db::write_events_ctx): one txn_id (uuidv7) per call;
  actor human|system (LLM is NOT an actor — accepted suggestions are
  human writes with origin llm_accept:<id>); device_id from meta
  (ADR-008; seeded INSERT OR IGNORE by the migration runner); schema_ver
  1; prev_hash = SHA-256 chain threaded row-to-row INSIDE the write tx.
  sha2 = "0.10" runtime dep (ADR-008, SPEC: tag).
- Origins wired where trivially known: lan (capture), llm_accept:<id>.
- verify_event_chain: full-log walk, legacy rows tolerated only at the
  head; runs at boot on a background thread (failure = warning toast).
- Readers (get_events / get_items_at / rebuild / undo / search) select
  the full 11-column envelope; Event struct + Zod schema extended
  (optional fields; legacy rows omit them on the wire).
- verify-schema.py: v3 + column-set check for ALTERed tables.
- Tests: 161/161 (155 + 6 envelope: stamping, txn sharing + chain
  threading, tamper-detect via raw INSERT, legacy upgrade + chain
  extension, device_id stability, chain property over any write
  sequence). cargo warning-clean, pnpm build clean, vitest 93/93.

## 2026-07-26 — P5b(2): undo by txn_id — QUESTIONS Q01 closed

- undo_last_action groups by the last HUMAN non-LLM event's txn_id
  (exact transaction boundary); (ts,type) heuristic survives only for
  legacy pre-envelope rows, pinned with txn_id IS NULL so the two
  populations never co-group.
- Undo skips actor='system' txns (VISION law 6: Ctrl+Z never reverses
  a timer execution; it looks past to the last human action).
- The undo write itself carries origin undo:<txn_id> (provenance).
- Q01 -> CONFIRMED in QUESTIONS.md (operator-authorized via ADR-007).
- New tests: mixed-type-txn undo (the exact Q01 shape; precondition for
  the I-21 trio), system-txn skip, accepted-reorg undo (item events
  compensated, audit row skipped). cargo 164/164 warning-clean.

## 2026-07-26 — P5/I-21: recurring tasks (backend + frontend)

- migrations/004_recurrence.sql: items.recurrence TEXT (user_version 4).
- domain/recurrence.rs: FREQ=DAILY|WEEKLY|MONTHLY[;INTERVAL=n] parser
  (canonicalizing) + next_after with Hinnant civil-date math + short-
  month clamping (Jan 31 + 1mo -> Feb 28/29); pinned to absolute unix
  anchors, round-trip property over ~80 years. 8 tests.
- Events: ITEM_RECURRENCE_SET (projection; ProjectionEvent now 8
  variants) + ITEM_RECURRED (audit link; to_projection_event -> None —
  doc updated so None reads "no projection effect": LLM advisory + audit
  links; the firewall claim is UNCHANGED, still no LLM variants).
- set_item_recurrence command (validates + canonicalizes; NO_OP/
  INVALID_RULE). Completing a recurring item spawns the next instance
  IN THE SAME TXN (STATE_CHANGED + CREATED + RECURRED) — one undoable
  action via txn_id; batch-done spawns per item with shared accounting
  (net-active per tier + rank chaining). Cap rule: active parent frees
  its own slot; blocked/done parent completing into full A/B routes the
  child to INBOX (marking done never fails). Children reach the
  frontend via item_created (set_item_state_inner_full + BatchResult.
  spawned).
- Undo: RECURRENCE_SET compensates before/after; the trio unwinds whole
  (parent active, child soft-deleted, audit link skipped) — regression-
  tested, the txn_id payoff.
- Frontend: Zod Item.recurrence + 2 EventType strings; Strip 🔁 badge +
  Repeat daily/weekly/monthly/Stop menu; Inspector "Repeats" row + event
  rendering; AuditLog filter/colors/describe.
- Gates: cargo 180/180 warning-clean; pnpm build clean; vitest 95/95;
  store-logic smoke green.

## 2026-07-26 — P5c(1): execution core — first_step + Today/Now + day rituals (backend)

- migrations/005: items.first_step (<=140 CHECK) + items.today_on
  (local-date TEXT) + partial index. SCHEMA_VERSION now derives from
  MIGRATIONS (kills the version-pin churn class).
- Events: ITEM_FIRST_STEP_SET / TODAY_ADDED / TODAY_REMOVED{cause}
  (projection; ProjectionEvent = 11 variants) + DAY_OPENED / DAY_CLOSED
  (audit, the log's FIRST NULL-item_id events). Firewall structurally
  unchanged (still zero LLM variants).
- commands/day.rs: add/remove_from_today (cap 3 ACTIVE-only — flow cap
  mirrors the stock caps), open_day (atomic ceremony: N adds + audit
  row, one txn, origin day_open), close_day (ONE question — tomorrow''s
  first move; origin day_close), roll_day (actor SYSTEM origin day_roll
  — the one sanctioned machine write, VISION law 6; idempotent, empty
  roll writes nothing), get_day_state (today ids + ceremony flag +
  the morning hand-back of tomorrow_first, ghost-safe).
- Frontend owns "what day is it" (local tz lives in JS; ISO dates
  compare lexicographically) — roll is invoked from the frontend at
  bootstrap/midnight, backend enforces everything else.
- set_first_step (items.rs): one line, 140 chars, the activation-energy
  handle — deliberately NOT subtasks (cut list holds).
- Undo: FIRST_STEP_SET/TODAY_* compensate exactly; DAY_* audit skipped;
  Ctrl+Z looks past the system roll (regression-tested).
- Tests: 190/190 (10 new day tests incl. atomic-over-cap rollback,
  system-actor roll pinning, done-frees-slot-keeps-membership, undo-
  past-roll; +1 property: today cap under any add/remove interleaving).
  vitest 95/95; both builds clean.

## 2026-07-26 — P5c(2): sessions + FocusBar — the log finally records WORK

- migrations/006: sessions projection table (outcome/reason CHECKs;
  <=1 open session enforced by a partial UNIQUE index over a constant
  — the storage-layer Now slot).
- Events SESSION_STARTED/SESSION_ENDED (ProjectionEvent = 13 variants);
  rebuild_projection now rebuilds BOTH projection tables under one
  purity law (regression-tested).
- commands/session.rs: start (active items only; one Now slot),
  end (done co-writes ITEM_STATE_CHANGED + recurrence spawn in the
  SAME txn; progress = honest pause; interrupted requires one of the
  5-word taxonomy: meeting/person/self_switch/blocked/energy).
- Undo philosophy encoded: sessions are BEHAVIOR records — attention
  cannot be un-spent. Undo skips pure session txns entirely and, for a
  done-ending, reverts the board effect while the session row stands
  (both regression-tested).
- UI: FocusBar (elapsed + content + first step + Done/Pause/Interrupt
  with reason menu; survives restart via get_open_session at
  bootstrap); Strip hover ▶ Start + "now" badge.
- Gates: cargo 197/197 warning-clean; pnpm build clean; vitest 95/95.

## 2026-07-26 — P5c(3): Today lane + day ceremonies UI + Mirror v1

- commands/mirror.rs — get_mirror_stats: ONE log pass + SQL, NO LLM
  (VISION 3.5 inverts the dependency order: facts free, interpretation
  optional). Computes flow (created/completed/throughput/lead-time
  p50+p90/Little''s-law prediction), A-leak rate (A->C|inbox within 48h
  of entering A), avoidance (committed items with ZERO sessions — the
  procrastination metric v0.2 structurally could not answer), block map
  (reason -> count + total days, open intervals count to now), session
  stats (+ interruption taxonomy), Today honesty (planned/finished/
  expired — timezone-free by construction), and receipts (finished work
  with its journey). 8 tests incl. empty-log calm and percentile math.
- TodayLane.tsx: the lane above the board (<=3), roll_day on mount
  (frontend owns local date), day-open picker that floats last night''s
  "first move" to the top, per-row Start, day-close with its ONE
  question. MirrorView.tsx: hand-rolled figures/bars, no chart lib;
  editorial rule — it accuses only when the data is unambiguous
  (>=40% leak) and reads calm on an empty log.
- Mirror added to the view switcher + command palette.
- Tests: cargo 206/206 warning-clean; vitest 106/106 (+11: FocusBar 5,
  MirrorView 6); pnpm build clean; store-logic + check-golden green.

## 2026-07-26 — v0.3 session close: doctrine, docs, run-state

- Doctrine co-pass committed (3b0c0ea): CLAUDE v1.9 (four new laws),
  SPEC v1.8 (envelope/runner/Today/sessions/Mirror), PROMPTS v1.6 (six
  shipped increment prompts). Archived v1.8/v1.7/v1.5. SPEC_AMENDMENT
  third pass appended.
- README rewritten for v0.3 (ten principles; Today/sessions/first-step/
  repeat/Mirror usage; sharpened non-goals incl. no gamification, no
  auto-planning, no estimates; versioning history).
- FUTURE_WORK.md rewritten post-v0.3: F-01 golden freeze (operator),
  F-02 cold verifier, F-03 streaming, F-04 coach v2, then VISION v0.4/
  v0.5/Phase 6.
- REVIEW_QUEUE.md rebuilt cheapest-first with exact reverts; the
  complacency canary names the missing leg (no cold verifier yet, so
  items 5-8 are plausible not confirmed).
- TASKLIST P5a/P5b/P5c -> DONE; RUN_STATE + memory refreshed.
- Session gates: cargo 206/206, vitest 106/106, builds clean.

## 2026-07-26 — P2e-equivalent: cold-context verifier findings FIXED

Cold verifier (Opus, cold context, `de37921..HEAD`) returned **FAIL**.
Every finding is now fixed + regression-tested. This is the second time
in this repo that a cold pass caught what green tests missed.

- **BLOCKING — Today cap bypassed on the re-entry doors.** A done/
  blocked item KEEPS Today membership but frees its slot, so
  reactivation (set_item_state / batch_set_state / restore_item /
  accept-reorg) could put 4 active on one date in one click. Same class
  as the P2e restore_item bug: an entry path that skipped a cap.
  Fix: `day::today_overflow_draft` + `TodayAccounting`, called from all
  four doors. It DROPS membership (logged, cause=user) rather than
  failing the transition — because undo of a completion re-activates,
  and **undo must never fail**. 4 regression tests + the batch case.
- **MAJOR — accept-reorg done path never spawned recurrence.** A
  recurring item completed via the LLM accept-diff silently stopped
  recurring. Fix: build_recurrence_spawn now runs there too, with
  shared spawn/today accounting across the accepted batch. Test:
  `accept_reorg_done_spawns_the_recurrence_like_every_other_done_door`.
- MINOR — spawn accounting ignored slots freed by NON-recurring parents
  in the same batch (over-routed children to Inbox). Fix: record the
  freed slot before the recurrence check.
- MINOR — mirror double-counted a done→undo→done as 2 completions.
  Fix: completions are keyed by item (last one wins; an undone
  completion drops out). 2 tests.
- MINOR — receipts reported `items.updated_at` as the finish time (any
  later edit inflated days-to-done). Fix: use the logged completion ts
  (Walk.done_at was computed and never read). Test added.
- MINOR — `TodayHonesty.expired` counted FINISHED items as rolled over,
  contradicting its own doc. Fix: only unfinished work counts as
  slippage; the old test that pinned the contradiction was corrected.
- MINOR — a session whose item was soft-deleted mid-session could never
  be ended with `done` (Now slot stuck open). Fix: skip the co-write;
  debug_assert the slot always frees.
- MINOR — moving an item between Today dates emitted no TODAY_REMOVED
  for the old date (add/remove pairs didn't balance). Fixed + test.
- NOTE — undo's skip-list was duplicated in SQL and in the match arms
  with nothing pinning them. Fix: single `UNDO_SKIP_TYPES` const drives
  the SQL; a test pins the correspondence incl. the deliberate
  ITEM_RECURRED asymmetry.
- NOTE — golden runner's rebuild assertion covered `items` only; now
  covers `sessions` too.
- **verify-schema.py was silently broken since migration 002** (never
  run: it required a live DB). Three real bugs found by finally
  executing it: `CREATE UNIQUE INDEX` unmatched; CREATE/RENAME/DROP
  applied out of source order (deleting the surviving `items`); trigger
  bodies truncated at their first inner `;`. Fixed, plus a new
  `--fresh` mode that builds a throwaway DB from the migrations so the
  gate no longer depends on having launched the app. Now green: 13
  objects verified, user_version 6.

Gates: cargo **216/216** warning-clean, vitest 106/106, both builds
clean, store-logic + check-golden + verify-schema --fresh all green.

## 2026-07-27 — Second cold pass over b884b4d: FAIL, all findings fixed

The fix commit was itself cold-reviewed. It had introduced a **worse
bug than the one it fixed** — the case for reviewing fixes, not just
features.

- **BLOCKING — the accept-reorg recurrence spawn escaped the A/B cap.**
  Root cause: TWO capacity ledgers in one transaction.
  `apply_reorg_inner` reasons over a `sim` map (and cap-checks against
  it), but `build_recurrence_spawn` decided the child''s tier from the
  LIVE projection. The child existed in neither ledger, so
  `[Done(recurring_a1), Active(blocked_b1)]` with A at cap committed A
  at **6 active** against A_CAP=5.
  Fix = ONE ledger. Split `build_recurrence_spawn` into placement
  (caller-owned, because only the caller knows its own capacity view)
  and `recurrence_child_drafts` (shared, so content/dates/link cannot
  drift). apply_reorg now decides the child''s tier from `effective_
  active(orig, sim)`, ranks it from the single `tier_last_rank` map,
  and inserts it into `sim` so the final cap check counts it.
- **MAJOR — rank collision.** `tier_last_rank` (moves) and
  `spawn_acct.last_rank` (spawns) both seeded from the untouched
  projection, and `rank_between` is deterministic → two items in A with
  byte-identical ranks (no UNIQUE constraint; commits silently), and
  the next drag between them throws in `rankBetween`. Fixed by the
  single-ledger refactor; regression-tested.
- **MAJOR — spawned children never reached the UI.** `AnalyzePanel`
  closes without refetching, so an accepted recurring completion left
  the next instance in SQLite and invisible until restart.
  `apply_reorg_inner` now returns `ReorgOutcome{affected, spawned_ids}`
  and `accept_suggestion` emits `item_created` for them.
- **MAJOR — accounting from simulated state**; plus `ops` was not
  deduped, so `[Done(x), Active(x), Done(x)]` spawned TWO children from
  one item. Duplicates now rejected with BAD_ARGS.
- MINOR — `TodayAccounting` never recorded de-activations, so "finish
  X, reactivate Y" (both on one Today) dropped Y although X had freed
  the slot. Added `release()`; regression-tested.
- MINOR — receipts filtered/ordered by `updated_at` while displaying
  `done_at`. Now derived from the same completion ledger as the flow
  figures: ordered by completion, window-consistent, and a deleted item
  drops out. 2 tests.
- MINOR — `flow.completed` silently dropped completions whose creation
  event was unknown. Lead time is now `Option`; the completion counts.
- MINOR — `debug_assert!` with a `?` inside: unchecked in release and
  divergent on the error path. Promoted to a real check
  (`SESSION_STILL_OPEN`).
- NOTE — golden runner''s session snapshot omitted `reason`/`note`.
- **verify-schema.py `--fresh` was near-tautological** (it built the DB
  from the files it parsed). Added a drift check: `db/mod.rs`
  MIGRATIONS must match `migrations/` on disk and EXPECTED_VERSION.
  Verified with a negative control — hiding 006 from Rust fails loudly.
- Doctrine co-pass the verifier flagged as missing: SPEC §3.7 now
  documents the re-entry-door drop (and WHY it drops rather than
  refuses: undo must never fail) and the two-event date move; §8.7
  documents the one-ledger rule and the BAD_ARGS duplicate guard.

Gates: cargo **224/224** warning-clean, vitest 114/114, both builds
clean, store-logic + check-golden + verify-schema --fresh green.
