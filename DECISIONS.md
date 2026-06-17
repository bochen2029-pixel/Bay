# DECISIONS.md — Bay v0.2.0 revamp run

> Architecture Decision Records. Why things are the way they are.
> Append-only. Each entry: date, status, context, decision, rationale,
> alternatives, consequences.

## ADR-001 — Adopt Heavy-tier autonomy spine for v0.2.0 revamp
**Date:** 2026-06-17
**Status:** Active
**Context:** The revamp is a ~30–40 wall-clock-hour multi-session
effort across 12 phases. Without a durability spine, compaction or
crash mid-run would lose orientation and force re-deriving state from
git history (10–20 min recovery, risk of redoing complete work).
**Decision:** Stand up the solo-enterprise-architect v7 Heavy-tier
spine: AUTONOMY_CHARTER + 9 run-state files + 6 hooks + 3 subagents +
13 skills + run-lock + attention probe + run-metrics.jsonl + marrow.lock.
**Rationale:** The methodology's durable principle — state in files,
never in head; every save point compaction-indifferent — is substrate-
independent. Bay's scale (Standard tier: ~12 Rust modules + ~16 TS
components) justifies Heavy because the revamp touches 6 critical
modules where confident-wrong output is most dangerous.
**Alternatives rejected:**
- Light/Micro spine: insufficient for the 6 critical-module two-pass
  + non-LLM oracle discipline the correctness layer demands.
- No spine (plain incremental): loses the run to the first compaction.
**Consequences:** ~1h setup tax at Phase 0; amortized for all future
Bay work. Run-state files are version-controlled.

## ADR-002 — Archive-and-diff doctrine discipline (per user choice)
**Date:** 2026-06-17
**Status:** Active
**Context:** Bay has held append-only archive discipline since v1.0
(`archive/CLAUDE_v1.0.md`...`v1.5.md`, etc.). The v0.2.0 revamp
substantially changes doctrine (new SPEC sections, new PROMPTS
increments I-15..I-27, CLAUDE "Current state" refresh).
**Decision:** Copy current `CLAUDE.md`→`archive/CLAUDE_v1.6.md`,
`SPEC.md`→`archive/SPEC_v1.5.md`, `PROMPTS.md`→`archive/PROMPTS_v1.3.md`
before editing. Bump live docs to v1.7/v1.6/v1.4. Top-of-file
blockquote notes what changed.
**Rationale:** Honors the project's own discipline (CLAUDE.md §
"Extend SPEC.md" requires explicit notes on what changed and why).
Breaking it would store debt and undermine the doctrine's authority.
**Alternatives rejected:**
- Edit live, no archive: faster; breaks discipline held since v1.0.
- Leave doctrine alone, code-only: code drifts from doctrine further.
**Consequences:** Slightly more ceremony at Phase 3; doctrine stays
auditable.

## ADR-003 — `proptest` as Cargo dev-dependency for property tests
**Date:** 2026-06-17
**Status:** Active (pre-authorized per charter §3)
**Context:** Phase 2a needs property-based tests for projection
determinism, swap atomicity, write_events rollback, get_items_at
monotonicity, rank_between bounds. These are the non-LLM oracles for
the 6 critical modules (Externality Principle).
**Decision:** Add `proptest = "1"` to `Cargo.toml`
`[dev-dependencies]`.
**Rationale:** `proptest` is the standard Rust property-test crate;
test-only (no runtime surface, no binary size impact); doesn't broaden
the runtime dependency surface. Property tests are the cheapest
non-LLM oracle after operator golden cases.
**Alternatives rejected:**
- `quickcheck`: older, less actively maintained, fewer strategies.
- Hand-rolled random-input tests: reinvents proptest poorly.
**Consequences:** Test suite slightly slower (property tests run
256+ cases each); acceptable per charter (tests <60s/unit-module).

## ADR-004 — `@tanstack/react-virtual` for I-16 C-tier virtualization
**Date:** 2026-06-17
**Status:** Active (pre-authorized per charter §3, pending I-16)
**Context:** SPEC §10.12 resolved C-tier virtualization as "no
virtualization in v1; at 100+ items, default-collapse to show first
50, click to expand." I-16 implements this. The collapse alone may
suffice, but if virtualization is also wanted (for the unbounded C
tier), a windowing lib helps.
**Decision:** Add `@tanstack/react-virtual` as a runtime dep **only
if** I-16's collapse-only approach proves insufficient at 500+ items.
Otherwise hand-roll the collapse (no new dep). Decide at I-16
implementation time.
**Rationale:** SPEC §6.2 forbids UI component libraries but
`@tanstack/react-virtual` is a primitive (headless windowing), not a
component lib. Still, minimize deps; collapse-only may be enough.
**Alternatives rejected:**
- `react-window`: older, less flexible.
- Hand-rolled windowing: fine for collapse; painful for true
  virtualization.
**Consequences:** Possibly one new runtime dep; ADR updated at I-16.
