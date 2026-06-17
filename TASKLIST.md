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
    status: IN_PROGRESS
    dependency: [P0]
    blocks: []
    fan_out: 0
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 3h
    critical: false
    oracle: [characterization]
  - id: P2a
    description: "Property tests: proptest for rank_between, projection determinism, swap atomicity, write_events rollback, get_items_at"
    status: NOT_STARTED
    dependency: [P0]
    blocks: [P2e]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 3h
    critical: true
    oracle: [property]
  - id: P2b
    description: "DB-enforced invariants: migration 002_invariants.sql (CHECKs + append-only trigger), user_version→2, verify-schema.py"
    status: NOT_STARTED
    dependency: [P0]
    blocks: [P2e]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 1h
    critical: true
    oracle: [property, runtime]
  - id: P2c
    description: "Operator golden cases: contracts/golden/{projection,swap,rank,caps}.json + CI/hook check"
    status: NOT_STARTED
    dependency: [P0]
    blocks: [P2e]
    fan_out: 1
    verifiability: REQUIRES_HUMAN_REVIEW  # operator freezes them
    cost_estimate: 2h
    critical: true
    oracle: [golden]
  - id: P2d
    description: "Type-level LLM firewall: ProjectionEvent sealed enum"
    status: NOT_STARTED
    dependency: [P0]
    blocks: [P2e]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 2h
    critical: true
    oracle: [property, golden]
  - id: P2e
    description: "Two-pass verification on all 6 critical modules (cold-context verifier subagent + non-LLM oracle gate)"
    status: NOT_STARTED
    dependency: [P2a, P2b, P2c, P2d]
    blocks: [P3]
    fan_out: 1
    verifiability: REQUIRES_HUMAN_REVIEW
    cost_estimate: 2h
    critical: true
    oracle: [golden, property, runtime]
  - id: P3
    description: "Doctrine reconciliation: archive v1.6/v1.5/v1.3, bump to v1.7/v1.6/v1.4, reconcile SPEC §5.1/§6/§6.2/§10.12 drift, add I-15..I-27 prompts"
    status: NOT_STARTED
    dependency: [P2e]
    blocks: [P4]
    fan_out: 1
    verifiability: REQUIRES_HUMAN_REVIEW
    cost_estimate: 2h
    critical: false
    oracle: []
  - id: P4
    description: "Above-and-beyond UX: I-15 palette, I-16 C-tier virtualization, I-17 undo/redo, I-18 audit-log search, I-19 batch ops"
    status: NOT_STARTED
    dependency: [P3]
    blocks: [P5]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 6h
    critical: false
    oracle: [characterization]
  - id: P5
    description: "Selective v2: I-20 LLM re-org diffs, I-21 recurring tasks, I-22 LLM streaming"
    status: NOT_STARTED
    dependency: [P4]
    blocks: [P6]
    fan_out: 1
    verifiability: AUTONOMOUSLY_VERIFIABLE
    cost_estimate: 8h
    critical: false
    oracle: [characterization]
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
