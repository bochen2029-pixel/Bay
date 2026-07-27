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

## ADR-005 — SPEC §5.1 `bootstrap` return shape: amend SPEC, not code
**Date:** 2026-06-17
**Status:** Active
**Context:** SPEC §5.1 (v1.5) said `bootstrap` returns
`{items, settings, lanCapture}`. The actual code (`lib.rs:49-52`)
returns `{items, settings}` — no `lanCapture` field. The frontend
`BootstrapResult` Zod schema also omits `lanCapture`. The frontend
calls `get_lan_capture_status` separately when it needs LAN capture
state (e.g., Settings view). So `lanCapture` in bootstrap was
spec'd but never implemented, and the separate command covers the
need.
**Decision:** Amend SPEC §5.1 to match code: `bootstrap` returns
`{items, settings}`. Do NOT add `lanCapture` to the bootstrap return
— the separate `get_lan_capture_status` command is the right surface
(LAN capture state is only needed in Settings, not at app start).
**Rationale:** Code is canonical when doc conflicts (charter §3
pre-authorization). The separate-command pattern is cleaner (bootstrap
stays minimal; LAN capture state is fetched on-demand). Adding
`lanCapture` to bootstrap would couple app-start to LAN server state,
which is undesirable (the server may not be running at bootstrap).
**Alternatives rejected:**
- Add `lanCapture` to bootstrap return: couples app-start to LAN
  server state; redundant with `get_lan_capture_status`.
- Leave SPEC drifting: stores debt; the spec's authority erodes.
**Consequences:** SPEC §5.1 reconciled with code. Frontend unchanged
(it already matches the code). No behavior change.

## ADR-006 — Type-level LLM firewall via `ProjectionEvent` (Phase 2d)
**Date:** 2026-06-17
**Status:** Active
**Context:** The LLM firewall (CLAUDE.md "LLM firewalled out of the
decision path") was enforced by an explicit `Ok(())` match arm for
the three `LlmSuggestion*` `EventType` variants in
`apply_event_to_projection`. A future edit could accidentally add
projection logic to an LLM arm, or a new LLM event type could slip
past the exhaustiveness check.
**Decision:** Introduce `ProjectionEvent` enum (7 item-event variants
only; deliberately no LLM variants). `EventType::to_projection_event()`
returns `Option<ProjectionEvent>` (None for LLM events).
`apply_event_to_projection` dispatches on `ProjectionEvent`, not
`EventType`. The firewall is now "type system won't let you."
**Rationale:** Compile-time contract enforcement wherever the type
system permits (solo-enterprise-architect v7). The firewall's policy
lives in one place (`to_projection_event`); everywhere else, the types
carry it. Adding projection logic for an LLM event now requires adding
a `ProjectionEvent` variant, which the compiler flags at every match.
**Alternatives rejected:**
- Keep the `Ok(())` match arm: convention, not type-level; vulnerable
  to future edits.
- Sealed trait: Rust's sealed-trait pattern is heavier; an enum with
  exhaustive match is the right primitive for 7 variants.
**Consequences:** `apply_event_to_projection` signature unchanged
(still takes `&Event`); the conversion happens inside. All existing
tests pass (behavior identical). 3 new tests pin the firewall.

## ADR-007 — Operator directive 2026-07-26: resume run, proceed at best recommendation
**Date:** 2026-07-26
**Status:** Active
**Context:** The run paused cleanly at the I-20 boundary (STOP_ACK) with
two decisions deferred to operator review (REVIEW_QUEUE #1 prompt
evolution; #10 txn_id/I-21 deferral, QUESTIONS Q01). This session, the
operator received VISION.md (first-principles remake brainstorm, incl.
§8 priority: golden runner → envelope 003 → undo-by-txn_id → I-21 →
execution core → doctrine co-pass) and directed: "proceed at your best
recommendation; most ambitious and most aggressive while maintaining
highest quality."
**Decision:**
(a) Directive read as operator sign-off for the recommended path,
including the `events` schema change that Q01 explicitly gated on
operator review. STOP_ACK deleted; run resumed under run-lock
`2026-07-26-T00-00`.
(b) REVIEW_QUEUE #1 RATIFIED: the system prompt's "observe → observe +
optionally propose" evolution stands as the doctrine-preserved v2
surface, not a firewall change.
(c) Migration 003 ships the **full envelope** (txn_id, actor, origin,
device_id, schema_ver, prev_hash), not txn_id alone: all columns
additive + nullable (legacy rows valid); actor/origin are consumed
within this run (undo grouping, system day-roll, provenance);
device_id/schema_ver/prev_hash prepare continuity (VISION §3.0, T1)
at near-zero marginal cost now vs. a second schema pass later.
(d) VISION tension items built this run: T1 (envelope), T2 (day-roll as
`actor: system`, Today-membership only), T3 (Today overlay, cap 3), T6
(sessions/rituals/Mirror), T7 (I-21), T9 (bankruptcy batch-archive if
reached). NOT built, still operator-gated: T4 (LLM Today draft), T5
(ICS overlay), T8 (sync).
**Alternatives rejected:** txn_id-only migration (re-opens the events
schema again next increment); ignoring the directive's ambition and
shipping only I-21 (under-delivers the explicit ask).
**Consequences:** Doctrine co-pass this run must encode the new
surfaces (CLAUDE v1.9 / SPEC v1.8 / PROMPTS v1.6); REVIEW_QUEUE rebuilt
for return review; every increment stays gate-green and committed
separately with exact reverts.

## ADR-008 — `sha2` runtime dep + meta-table device identity (migration 003)
**Date:** 2026-07-26
**Status:** Active
**Context:** The envelope's `prev_hash` chain needs SHA-256 inside the
write path; `device_id` needs a durable home reachable inside the write
transaction without threading state through every command signature.
**Decision:** (1) Add `sha2 = "0.10"` (RustCrypto) as a RUNTIME
dependency — charter §3 requires an ADR + `SPEC:` commit tag for any
new dep; both provided. (2) `device_id` lives in a new
`meta(key, value)` table inside `bay.db`, seeded `INSERT OR IGNORE` by
the migration runner: identity travels with the data it stamps and is
readable inside the same transaction that writes events.
**Alternatives rejected:** hand-rolled SHA-256 (never roll your own
crypto primitives); `device_id` in settings.json (splits identity from
data; unreachable inside the write tx without API churn across every
`*_inner` signature). Known trade-off: wholesale-copying `bay.db` to a
second machine carries the id until regenerated — acceptable and
documented pre-sync.
**Consequences:** every event written from v0.3 on is chained +
attributed; `verify_event_chain` runs at boot in a background thread
(failure = toast, never a boot block); migration 003 uses ALTER ADD
COLUMN (never a rebuild of `events` — charter §5 "never DROP tables");
`scripts/verify-schema.py` gains a column-set check for ALTERed tables.
