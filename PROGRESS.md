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

## 2026-07-27 — contracts/golden/today.json: the missing externality

The cold review''s diagnosis of the BLOCKING bug was structural, not
incidental: the Today law was in doctrine (CLAUDE §7/§10), enforced in
code, and **asserted nowhere an operator owned** — so no externality
could catch a bypass. 13 proposed cases now cover it: the cap, the
progress-visibility rule (done keeps membership, frees the slot), all
three re-entry doors, open_day atomicity + idempotence, the date-move
balance, the roll''s system-actor provenance, and both undo invariants.

Crucially the file ships EXECUTED, not merely present — adding an
unrun golden file would have recreated the exact pattern behind every
defect this run. golden_runner gained a today.json executor (same
panic-on-unknown-key discipline) and check-golden.py now requires it.

**Negative control run:** disabling the overflow guard makes the case
"REACTIVATION into a full day drops membership rather than failing"
fail by name; restoring it passes. The assertion bites.

Gates: cargo 225/225 warning-clean; check-golden 5 files / 80 cases.

## 2026-07-27 — Third cold pass: order-dependence found and structurally removed

Pass 3 (subject: the pass-2 fix `8f4592e` + today.json) returned FAIL:
3 MAJOR, 5 MINOR, 3 NOTE. The BLOCKING cap escape from pass 2 IS fixed
and no commit can exceed A/B — but the fix had made the accept path
**order-dependent**: derived effects were resolved incrementally inside
the op loop, so an op not yet visited read as a no-op.

- **MAJOR (JOINT_WRONG vs SPEC §8.7)** — a recurrence spawn could abort
  the whole accept with CAP_EXCEEDED, where SPEC says it must overflow
  to Inbox and NEVER fail. Worse: my own regression test enshrined the
  wrong behavior, against the SPEC line the commit message cited.
  Reversing the two ops — the identical accepted set — committed.
- **MAJOR** — `TodayAccounting::release` only worked when the
  completion op preceded the reactivation; reversed, it dropped a
  Today slot the human never asked to lose.
- **MAJOR** — the duplicate-op guard I added rejected a coherent model
  output ("unblock it and demote it"), failing the whole accept and
  discarding every other op; and its stated justification did not hold
  (HashMap keys already prevented the double-count it described).

Fix: **two passes.** Pass 1 applies what the human accepted and records
what ends newly done/active; the cap check runs on those ops alone.
Pass 2 resolves the implications — spawns, Today overflow — from the
FINISHED simulation. The outcome is now a function of the op SET, not
its order, and a derived effect can never fail a legal diff. The
duplicate guard is gone (completions are a set, resolved once).

Also fixed: receipts no longer truncate before the deleted filter (10
deletions could empty the panel — law 9); receipt ties break by id
(HashMap order was nondeterministic); the post-commit SESSION_STILL_OPEN
error removed (unreachable by the unique index, and returning Err after
a successful write broke the command convention and stranded the focus
bar); `recurrence_child_drafts` returns the child id instead of making
callers recover it by draft position; the simulated child is modelled
faithfully (dates/first_step/reason) rather than cloned wholesale;
SPEC §5.1 + §8.7 updated.

**A test of mine turned out to be decoration.** The order-independence
property used `proptest::sample::subsequence`, which PRESERVES order —
it compared [0,1,2] against itself and asserted nothing. The negative
control caught it (it passed when it should have failed). Replaced with
an exhaustive 3!-permutation test over a board that exercises both
derived effects, and re-verified by injecting the real defect: it now
fails under it and passes when restored.

Gates: cargo **229/229** warning-clean; vitest 114/114; both builds
clean; store-logic, check-golden (5 files), verify-schema --fresh green.

## 2026-07-27 — Reachability audit: two commands shipped unreachable

Prompted by finding that `set_first_step` had no UI at all, I swept
every command in `generate_handler!` against `invoke("…")` call sites in
`src/`. Two were unreachable from the app:

- **`set_first_step`** — registered, tested, and DISPLAYED in three
  places (strip, Today lane, focus bar), with the Mirror reporting "no
  first step" as an avoidance signal — and no way to set one. The
  activation-energy handle VISION calls the central lever, shipped
  inert. Added "Set first step…" to the overflow menu with an inline
  one-line input, and the step now renders on the strip itself.
- **`add_to_today`** — the only route onto Today was the day-open
  picker, so "put THIS one on today", the natural gesture while looking
  at the board, had no affordance. Added a menu toggle that sends the
  LOCAL date and surfaces TODAY_FULL inline rather than to console.

(`get_settings` is also uninvoked, but legitimately so: settings arrive
via `bootstrap`. Left as available API, same call as ADR-005.)

This is the same failure shape as the run''s other defects — something
that exists but is never exercised — one layer up: a command that
exists but is never *invoked*. Worth keeping the sweep as a standing
check.

Gates: vitest 118/118, build clean.

- Made the sweep permanent: `scripts/check-reachability.py` parses
  `generate_handler!` and requires an `invoke("…")` call site in `src/`
  for every command, with a short justified allowlist (`get_settings`
  only). Negative-control verified: renaming the `set_first_step` call
  site makes it fail by name. A one-off audit would have been another
  check that ran once.

## 2026-07-27 — F-04 coach v2: the LLM can finally see behaviour

Until now `compress` shipped only board TOPOLOGY, so the sharpest
observation the coach could make was "this item is old" — while the
Mirror, from the same log, could say "you have never started it". The
firewall was never the limit; the context was.

- compression.rs gains: sessions in window + total minutes, outcomes by
  kind, what broke focus (the interruption taxonomy), Today
  planned/finished/rolled-over, and `never_started` — committed (A/B)
  active items with ZERO sessions EVER. Deliberately NOT windowed: "you
  have never started this" must not weaken to "not lately".
- prompt.rs renders an ATTENTION block and a COMMITTED BUT NEVER
  STARTED block, and the system prompt now instructs: prefer an
  observation grounded in what the user DID over how the board LOOKS;
  treat zero sessions as evidence of avoidance with a cause (too large,
  too vague, no first step) rather than laziness; and **report, do not
  exhort** — no praise, no encouragement, no streaks or scores.
- Firewall untouched: this is strictly more CONTEXT, not more
  authority. The LLM still proposes; the human still accepts; the
  deterministic tier still writes.
- 4 tests, including one that pins SPEC §8.3`s <=2500-token budget on a
  full board (A+B at cap, 40 C, 40 inbox) so the new blocks cannot
  quietly blow the prompt size.

Gates: cargo 233/233 warning-clean.

## 2026-07-27 — file-backed DB coverage (what the app actually runs)

Every DB test to date used `:memory:` with `max_size(1)`. The shipped
app opens a FILE with WAL and an 8-connection pool — and those differ
exactly where this schema is sensitive: the chain tail is read on a
pooled connection that may not be the one that wrote the previous row,
and migrations run against a durable file rather than a fresh page
cache. Nothing covered that combination.

- `file_backed_pool_migrates_writes_and_keeps_the_chain_intact`:
  real `open_pool`, interleaved single/batched writes across pooled
  connections, chain verified; then REOPENED from disk and verified
  again at user_version 6.
- `device_id_survives_reopening_a_file_database`: ADR-008 says identity
  travels with the data; if it regenerated per launch every restart
  would look like a new device to a future sync, and no in-memory test
  could see it. Negative-control verified (INSERT OR IGNORE -> OR
  REPLACE fails both device tests by name).

Gates: cargo 235/235 warning-clean.

## 2026-07-27 — Fourth cold pass: BLOCKING undo bug + ordering, one layer down

Pass 4 (subject: the pass-3 fix `0562957`) returned FAIL: 1 BLOCKING,
3 MAJOR. Chain is 4 for 4.

- **BLOCKING (llm.rs) — accepting `done` on a BLOCKED item permanently
  killed Ctrl+Z.** The accept path was the LAST done-door still
  dropping the outgoing `blocked_reason`; undo then wrote
  `state='blocked'` with a null reason, tripped the migration-002
  CHECK, and rolled back. Because undo keeps targeting the same
  transaction, it stayed dead until the user did something else
  undoable. Pre-existing since I-20 — but removing the duplicate-op
  guard in the previous commit ADDED a second route to it. One-line
  fix, mirroring items.rs/session.rs.
- **MAJOR x3, all order-dependence one layer down from where pass 3
  fixed it**: (a) with TWO recurring completions contending for one
  free A slot, the ops array decided which child overflowed; (b) with
  TWO reactivations contending for one Today slot, the ops array
  decided which lost it; (c) an item reactivated AND completed in the
  same diff stayed in `reactivated`, so pass 2 stripped a FINISHED
  item''s Today membership — freeing nothing (done items aren''t
  counted) and contradicting golden `today.json` case 3. That last one
  is a JOINT_WRONG the golden runner cannot catch, because it never
  drives `accept_suggestion`.

Fix: pass 2 now iterates by **board position** (`board_order`: tier,
rank, id), never by the ops array — the higher-ranked parent''s child
takes the free slot; the lower-ranked reactivation gives up the Today
slot. Both are answers the human can predict from their own board.
`reactivated` is filtered to items that actually END active.

**My permutation test''s blind spot contained all three MAJORs**: its
scenario had exactly one spawn and one reactivation — the configuration
where no winner has to be picked. Rebuilt with TWO of each contending
for one slot apiece (24 permutations), `rank` added to the fingerprint,
and sanity assertions that the contests actually occur. My own sanity
check then caught that the first rebuild didn''t create contention.

SPEC §8.7 narrowed honestly: order-independence covers commit/fail and
tier/state/Today/spawn placement; the relative RANK of several items
moved into one tier follows the human''s review order.

Negative controls on all three fixes. One of them PASSED first time —
the Today test didn''t depend on the filter it was meant to pin, so it
was rebuilt to the verifier''s actual probe shape and now fails without
the fix.

Gates: cargo **237/237** warning-clean; vitest 118/118; golden 5 files;
verify-schema --fresh; reachability 39/39.

## 2026-07-27 — golden runner now reaches the accept-diff door

Pass 4 raised a JOINT_WRONG that was really a structural gap: golden
`today.json` case 3 STATES the rule the accept path broke ("a finished
Today item keeps its membership"), and the case passed the whole time —
because the runner never drives `accept_suggestion`. The externality
existed; nothing pointed it at that door.

- golden_runner gains an `accept_reorg` op (seeds a suggestion, calls
  `apply_reorg_inner` with the ReorgOp wire shape) plus
  `expect_item_state` / `expect_item_blocked_reason`.
- today.json gains 2 ACCEPT-DIFF cases: a finished item keeps its
  membership while the genuinely-competing item yields the slot; and
  completing a BLOCKED item through the diff leaves undo working.
- Negative-controlled BOTH ways: reverting either pass-4 fix makes the
  corresponding golden case fail. These are the first golden cases in
  the repo that assert against the LLM accept path.

Gates: cargo 237/237 warning-clean; check-golden 5 files / 15 today
cases.

## 2026-07-27 — Fifth cold pass: the ordering KEY, and an unpinned policy

Pass 5 (subject: `a0f4775` + `1e7467d`) returned FAIL: 2 MAJOR, no
BLOCKING. Chain 5 for 5, severity still falling.

- **MAJOR — `board_order` read the MUTATED simulation.** `next_rank`
  hands out end-of-tier ranks in ops order, so two items moved into one
  tier had a relative rank the MODEL chose — and if they also contended
  for a slot, the model chose the winner. The same defect class one
  layer down again: pass 3 fixed iteration order, pass 4 fixed
  derived-effect ordering, and the ordering KEY was still ops-derived.
  Fix: key on `orig`, the pre-diff board — which is also the board the
  human was looking at when they read the diff, so the rule is
  predictable and not merely deterministic.
- **MAJOR — the whole `board_order` policy was unpinned.** The verifier
  demonstrated three mutations that left the ENTIRE suite green:
  flip the spawn sort, flip the Today sort, or replace the key with a
  raw UUID sort. My permutation test asserted only THAT a contest
  happened, never WHO won — and a cross-permutation comparison is
  satisfied by any deterministic key. So the policy SPEC now codifies
  could have been inverted, undetectably.
  Fix: two outcome-pinning tests that assert the higher-ranked parent`s
  child takes the free slot and the lower-ranked reactivation yields —
  with the winner deliberately listed FIRST in the diff, so ops order
  would produce the opposite answer. **Re-ran the verifier`s exact
  three mutations: all three now fail.**
- MINOR — golden cases declared `rank` values the runner discarded, and
  since `create_item_inner` places top-of-tier, the declared board was
  the exact INVERSE of the real one. Harmless while rank decided
  nothing; not harmless now that it decides contests. The runner
  honours declared ranks; the case text is now true.
- SPEC §8.7: resolved a contradiction the verifier caught between "a
  function of the accepted set" and "several ops on one item apply in
  order". The precise claim is **per item, in sequence; across items,
  as a set.**

Gates: cargo **239/239** warning-clean; vitest 118/118; golden 5 files;
verify-schema --fresh; reachability 39/39; store-logic.

## 2026-07-27 — scripts/check-mutations.py: negative controls as a gate

The chain`s deepest lesson is not any one bug: it is that my tests were
weak in the same DIRECTION as my attention, because I wrote both the
fix and the test. Twice a test turned out to assert less than it looked
like it did — a property that compared one ordering against itself, and
a contest policy that could be inverted with all 237 tests green. Both
were found by hand: break it, watch the test fail, restore.

That habit is now a gate. `check-mutations.py` carries 9 mutations,
each reintroducing a defect a cold review actually found, and requires
the suite to catch every one. `why` names the finding it guards, so a
survivor reports what was lost rather than just a red line.

**It found a real gap on its first run**: `recurrence/freed-slot-
accounting` SURVIVED — I fixed that pass-2 MINOR and never wrote a test
for it, so the bug could have walked straight back in. Test added
(`batch_spawn_counts_slots_freed_by_non_recurring_completions`); all 9
mutations now caught.

Standing rule going forward: when a cold review finds a defect, add the
mutation that reintroduces it. That is the cheapest guarantee the class
cannot silently return.

Gates: cargo 240/240 warning-clean; mutations 9/9 caught.

## 2026-07-27 — Sixth cold pass: the code was right, the guards were not

Pass 6 returned FAIL (3 MAJOR, 4 MINOR, no BLOCKING) with a verdict
worth quoting: **"No incorrect behaviour was found in the shipped code.
Every MAJOR is a hole in the guard."** First round where the
implementation held and only the safety net had gaps.

- **MAJOR — my own fix silently disarmed an operator-owned golden
  case.** Making today.json`s declared ranks REAL (right in itself)
  flipped case 13`s board: the finished item became the BEST-ranked
  contender, so the eviction loop reached the other item first and
  never evaluated it. The case passed either way. Fixed by ranking the
  completed item WORST, with the case text explaining why; re-verified
  by negative control (it now fails under the strip mutation).
  Lesson recorded: after changing how an oracle EXECUTES, re-run its
  negative control — a fix can quietly remove a case`s ability to fail.
- **MAJOR — the TIER byte of the contest key was decoration.** Making
  it constant, or inverting it, left 240 tests green: every contest
  fixture put both contenders in ONE tier. New cross-tier test, with
  the A item deliberately given the WORSE rank so only tier can produce
  the expected answer.
- **MAJOR — the Today door`s pre-diff keying was pinned only through
  the spawn door.** Changing just the Today sort survived. Now pinned
  independently with a fixture where the two rules disagree.
- MINORs: projection.json`s 20 declared ranks were still fiction (now
  honoured); SPEC §3.7 named a `TodayAccounting::release` that does not
  exist and claimed the accept path uses `today_overflow_draft` — it
  deliberately does not, and the two doors` divergence is now
  documented; think-aloud residue in a fixture comment removed.
- Gate hardened: refuses to run on a dirty tree (it edits in place; an
  interrupted run must be recoverable), rejects ambiguous anchors, and
  reports WHICH test caught each mutation — flagging compile-only
  catches as weak guards. Grew to 16 mutations, adding entries for the
  pass-1 and pass-2 defects that had none.

**Then the gate found three of THIS round`s new guards to be
decoration** — the cross-tier fixture (both contenders first-in-tier,
so they shared a rank and the ID tiebreak decided), the declared-rank
semantics (no case`s outcome depended on them), and `[done x, active
x]` (which must spawn nothing). All three closed.

Gates: cargo **243/243** warning-clean; vitest 118/118; **mutations
16/16 caught, each by a named test, none by compile error**; golden 5
files / 16 today cases; verify-schema --fresh; reachability 39/39.

## 2026-07-27 — Seventh cold pass: no behavioural finding, and a pattern named

Pass 7: **PASS with 3 MAJOR — "no incorrect behaviour was found in the
shipped code."** Two rounds running the verifier could not make the
code misbehave. But all three MAJORs shared one shape, and it is the
most useful thing this chain has produced:

> **The bug was fixed at every door at once. The guards were only ever
> added at the door the report happened to name.**

- The accept door`s `Done` arm had a test, a golden case AND a gate
  mutation for its blocked-reason carry. Its identical twin one match
  arm down — `Active` — had none. Dropping the same line there
  reproduces P2e BLOCKING-1 through the other door: undo of an unblock
  trips the migration-002 CHECK and kills Ctrl+Z for that transaction.
- `board_order`'s `tier` byte got two mutations last round. Its `id`
  tiebreak — same tuple, same SPEC sentence — got none. Ranks are
  per-tier sequences with no UNIQUE constraint, so two items can tie;
  without the tiebreak the stable sort falls back to the ops array and
  the model decides the contest. Both existing order-independence tests
  use distinct ranks and miss it entirely.
- `child.today_on = None` sat under a comment arguing it did not matter
  ("invisible today; only tier+state are read") while
  `effective_active_today` read it out of the same simulation one loop
  later. An inherited day evicts a real reactivation.

All three closed with a test AND a mutation, verified 1:1: each new
mutation is caught by exactly the one new test written for it.

**The gate audited itself and lost.** Its which-test-caught report —
the output used last round to condemn three guards as decoration —
printed only the alphabetically first failing test, hiding the re-armed
golden runner behind a "+1". And any non-zero cargo exit scored as
"caught", so a flake could certify an unguarded line. Now: every
failing test reported, a red suite naming none is INCONCLUSIVE,
baseline output printed on failure, and the dirty-tree refusal narrowed
to TRACKED files (refusing on untracked scratch made it unrunnable
during normal work, which is the surest way to make a gate unused).

Also pinned: `cause: "user"` on the accept door (Mirror counts
"expired" as roll-over slippage and feeds it to the coach, so
mislabelling inflates a statistic the user is judged on), and
`apply_declared_rank`'s fail-loud, which was an equivalent mutant.

Gates: cargo **247/247** warning-clean; vitest 118/118; **mutations
20/20**; golden 5 files / 16 today cases; verify-schema --fresh;
reachability 39/39.

## 2026-07-27 — Eighth cold pass, and the audit that met it halfway

Pass 8: **FAIL on 2 MAJOR, and again "no incorrect behaviour in the
shipped code"** — three rounds running. Both MAJORs were the same two
doors the session had *already fixed independently* while the pass was
in flight: `session.rs:157` and `items.rs:993`, the two remaining
unguarded doors of the P2e BLOCKING-1 blocked-reason class. The pass
verified them at 14b1111; they were closed at 47d9393.

Two different methods, same two doors. The verifier read the diff cold;
the session applied pass 7`s own lesson forward — after fixing a
defect, grep for its SIBLINGS and guard each — and audited all four
doors that write the blocked-reason carry. The batch door and the
session door had no guard at all. (The single-item door turned out to
be guarded twice, by an undo test in events.rs I had missed by grepping
only items.rs; my commit message overstated its weakness.)

So the pattern pass 7 named repeated *inside the commit that named it*:
fixed at five doors, guarded at three. It is now a standing rule.

The audit also produced the structural tell. The Today re-entry cap is
the same shape of rule and has never drifted — because three doors call
ONE shared helper, so one mutation breaks all three. The blocked-reason
carry is written out inline at four doors. **Duplicated logic needs a
guard per door; shared logic needs one.** Filed as F-05: extracting the
helper is the real fix, deliberately deferred, because seven passes of
evidence say changes made late in a chain are where defects hide.

Pass 8`s two MINORs, both closed here with a test and a mutation:
- `effective_active_today` filtered `today_on == Some(date)`; dropping
  the date comparison survived the whole suite. Two live dates coexist
  whenever a day is planned ahead or the app is open across midnight,
  and the mutation then evicts a legal third member of one day because
  another day`s member counted against it.
- the accept door`s already-done skip was unguarded: without it,
  accepting `done` on an item finished BEFORE the diff appends a
  done->done event and spawns a DUPLICATE recurrence child.

And it caught an assertion of mine that was structurally unfalsifiable:
`ItemCreatedPayload` has no `today_on` field, so a spawned child`s
stored membership is unconditionally NULL. Kept as a tripwire, but
labelled, with the neighbouring assertion named as the one that bites.

Also closed: the pass-6 gap that had stayed open two rounds — the
exhaustive permutation test contained no `move` op. Now 24 orderings of
[move x, move y, done x, done y] including the interleavings, asserting
who wins, with the fixture built so id order and rank order disagree.
Excluding `rank` from its fingerprint is deliberate and is filed as
**QUESTIONS Q02**: intra-tier placement still follows the model`s
listing order, which is a real order-dependence inside a path whose
stated invariant is order-independence.

Two process errors of mine, both fixed rather than filed:
- I ran the in-place mutation gate against the live repo **while a cold
  verifier was reading it**. A reviewer opening a mutated file reports a
  defect that does not exist, and nothing in its output distinguishes
  that from a real one. The gate now writes `.mutation-in-progress`, and
  I run it in a scratch clone.
- A 10-minute tool timeout killed a gate run mid-mutation and left
  broken code in the tree — exactly what the clean-tree refusal exists
  to make visible, which it did.

Gates: cargo **252/252** warning-clean; vitest 118/118; golden 5 files;
verify-schema --fresh; reachability 39/39; store-logic. Mutations at 27
(from 20), running in a clone.
