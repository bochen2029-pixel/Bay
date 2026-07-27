---
run_id: 2026-06-17-T00-00
operator_brief: "Revamp Bay v0.1.1 → v0.2.0: fix bugs, mechanically enforce invariants, reconcile doctrine, expand across I-15..I-27."
critical_modules:
  - db::write_events
  - db::items::apply_event_to_projection
  - commands::items::swap_move_inner
  - domain::rank::rank_between
  - commands::events::get_items_at_inner
  - commands::events::rebuild_projection_inner
items:
  - id: P0
    description: "Autonomy spine: charter, run-state files, hooks, skills, agents, run-lock, git tag, marrow.lock"
    status: DONE
    dependency: []
    blocks: [P1, P2a, P2b, P2c, P2d, P3]
    fan_out: 6
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 1h
    cost_actual: 0.5h
    critical: false  # infrastructure, not domain-critical
    oracle: []
    notes: "Baseline all-green: cargo build clean, cargo test 91/91, pnpm build clean, pnpm test 85/85, store-logic 55/55. Tag autonomous-run-2026-06-17-start placed."
  - id: P1
    description: "Bug-fix pass: restore_item event semantics, prompt.rs hardcoded caps, bootstrap return shape, set_item_state at cap, sweep"
    status: DONE
    dependency: [P0]
    blocks: []
    fan_out: 0
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 3h
    cost_actual: 0.25h
    critical: false
    oracle: [characterization]
    notes: "1 real bug fixed (prompt.rs magic numbers -> A_CAP/B_CAP). 4 confirmed-not-bugs (documented design choices / correct behavior). 1 deferred to Phase 3 (SPEC drift). Sweep clean. cargo test 91/91."
  - id: P2a
    description: "Property tests: proptest for rank_between, projection determinism, swap atomicity, write_events rollback, get_items_at"
    status: DONE
    dependency: [P0]
    blocks: [P2e]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 3h
    cost_actual: 0.75h
    critical: true
    oracle: [property]
    notes: "15 property tests added across 4 modules: rank (8), events (2: rebuild determinism + get_items_at now), items (4: cap A/B/inbox-C + swap atomicity), db (1: write_events rollback any position). cargo test 106/106 (up from 91). cargo build warning-clean."
  - id: P2b
    description: "DB-enforced invariants: migration 002_invariants.sql (CHECKs + append-only trigger), user_version→2, verify-schema.py"
    status: DONE
    dependency: [P0]
    blocks: [P2e]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 1h
    cost_actual: 0.5h
    critical: true
    oracle: [property, runtime]
    notes: "Migration 002 adds events_no_update + events_no_delete triggers (append-only now runtime-enforced, not just prose) + items CHECK constraints (deleted IN (0,1), content 1..4096, rank non-empty, blocked=>reason). 4 new tests prove the trigger blocks UPDATE/DELETE and allows INSERT, and CHECKs reject invalid rows. user_version=2. verify-schema.py updated to load all migrations + expect v2. cargo test 110/110."
  - id: P2c
    description: "Operator golden cases: contracts/golden/{projection,swap,rank,caps}.json + CI/hook check"
    status: DONE
    dependency: [P0]
    blocks: [P2e]
    fan_out: 1
    verifiability: REQUIRES_HUMAN_REVIEW  # operator freezes them
    cost_estimate: 2h
    cost_actual: 0.4h
    critical: true
    oracle: [golden]
    notes: "4 golden files: projection (7 cases), swap (6), caps (12), rank (42, mirrors scripts/rank-fixtures.json). All _status:proposed (operator freezes on review). scripts/check-golden.py CI check passes: verifies every critical module has >=1 case + no frozen-file edit without SPEC: tag. contracts/golden/README.md documents the JOINT_WRONG detector + lifecycle."
  - id: P2d
    description: "Type-level LLM firewall: ProjectionEvent sealed enum"
    status: DONE
    dependency: [P0]
    blocks: [P2e]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 2h
    cost_actual: 0.4h
    critical: true
    oracle: [property, golden]
    notes: "Added ProjectionEvent enum (7 item-event variants only) + EventType::to_projection_event() returning Option<ProjectionEvent> (None for LLM events). apply_event_to_projection now dispatches on ProjectionEvent, not EventType — LLM events structurally cannot reach the projection match arms. 3 new tests pin the firewall. cargo test 113/113, cargo build warning-clean. The LLM firewall is now 'type system wont let you' not 'match arm returns Ok(())'."
  - id: P2e
    description: "Two-pass verification on all 6 critical modules (cold-context verifier subagent + non-LLM oracle gate)"
    status: DONE
    dependency: [P2a, P2b, P2c, P2d]
    blocks: [P3]
    fan_out: 1
    verifiability: REQUIRES_HUMAN_REVIEW
    cost_estimate: 2h
    cost_actual: 0.7h
    critical: true
    oracle: [golden, property, runtime]
    notes: "Re-dispatched 2 cold-context verifiers (prior ones never returned). Found 2 BLOCKING bugs the 143-test suite missed: (1) undo-of-unblock crashed the migration-002 blocked_reason CHECK; (2) restore_item had no cap check (JOINT_WRONG vs caps.json #12). Both fixed + regression-tested. STRUCTURAL: (ts,type) undo over-grouping → QUESTIONS Q01 (production-safe; txn_id fix operator-gated). Gap: check-golden.py doesn't execute cases (P2c follow-up). cargo 143/143, warning-clean."
  - id: P3
    description: "Doctrine reconciliation: archive v1.6/v1.5/v1.3, bump to v1.7/v1.6/v1.4, reconcile SPEC §5.1/§6/§6.2/§10.12 drift, add I-15..I-27 prompts"
    status: DONE
    dependency: [P2e]
    blocks: [P4]
    fan_out: 1
    verifiability: REQUIRES_HUMAN_REVIEW
    cost_estimate: 2h
    cost_actual: 1.5h
    critical: false
    oracle: []
    notes: "Committed 4078c8b. Archived v1.6/v1.5/v1.3; bumped CLAUDE v1.7/SPEC v1.6/PROMPTS v1.4. SPEC §5.1/§6/§6.2 reconciled; new §4.4/§4.5/§4.6/§11. PROMPTS I-15..I-27 are forward-ref stubs (full prompts added as each phase ships)."
  - id: P4
    description: "Above-and-beyond UX: I-15 palette, I-16 C-tier virtualization, I-17 undo/redo, I-18 audit-log search, I-19 batch ops"
    status: DONE
    dependency: [P3]
    blocks: [P5]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 6h
    cost_actual: 3h
    critical: false
    oracle: [characterization]
    notes: "I-15 palette (c2d4278), I-16 C-collapse (c4260e6), I-17 undo (c45f400), I-18 audit search (6307b7c) committed by prior run. I-19 batch ops completed on resume: cap bug fixed + regression test, undo generalized to batch-undo, frontend multi-select + BatchActionBar. cargo 139/139, vitest 90/90."
  - id: P5
    description: "Selective v2: I-20 LLM re-org diffs, I-21 recurring tasks, I-22 LLM streaming"
    status: IN_PROGRESS
    dependency: [P4]
    blocks: [P6]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 8h
    cost_actual: 2.5h
    critical: false
    oracle: [characterization]
    notes: "I-20 DONE (cbad5b2). I-21 DONE (82f7372, v0.3): recurring tasks shipped once migration 003 txn_id made the mixed-type completion undoable as one action; Q01 CONFIRMED. I-22 (streaming) still not started — see FUTURE_WORK F-03."
  - id: P5a
    description: "Golden RUNNER: execute contracts/golden/{projection,swap,caps}.json in cargo test (closes the exists-but-never-executed gap behind the P2e JOINT_WRONG); fix defective proposed cases found by execution"
    status: DONE
    dependency: []
    blocks: [P5b]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 2h
    critical: true
    oracle: [golden]
  - id: P5b
    description: "Migration 003 event envelope v2 (txn_id, actor, origin, device_id, schema_ver, prev_hash) + write-path population + undo grouped by txn_id (closes QUESTIONS Q01) — operator-authorized via ADR-007"
    status: DONE
    dependency: [P5a]
    blocks: [P5, P5c]
    fan_out: 2
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 3h
    critical: true
    oracle: [property, golden, runtime]
  - id: P5c
    description: "VISION v0.3 execution core: first_step + Today/Now overlay (cap 3) + day roll (actor system) + sessions/focus mode + day open/close + Mirror v1 (deterministic stats) — ADR-007 (d)"
    status: DONE
    dependency: [P5b]
    blocks: [P7]
    fan_out: 3
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 8h
    critical: true
    oracle: [property, characterization]
  - id: P6
    description: "Full v2 modernization: I-23 sync, I-24 multi-profile, I-25 theming, I-26 plugin surface, I-27 mobile companion"
    status: NOT_STARTED
    dependency: [P5]
    blocks: [P7]
    fan_out: 1
    verifiability: REQUIRES_HUMAN_REVIEW
    cost_estimate: 12h
    critical: false
    oracle: []
  - id: P7
    description: "Release: marrow.lock, run-metrics final, /close-run queue, README, runbooks, tag v0.2.0"
    status: NOT_STARTED
    dependency: [P6]
    blocks: []
    fan_out: 0
    verifiability: REQUIRES_HUMAN_REVIEW
    cost_estimate: 2h
    critical: false
    oracle: []
---
