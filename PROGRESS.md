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
