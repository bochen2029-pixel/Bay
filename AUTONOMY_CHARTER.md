# AUTONOMY_CHARTER — Bay v0.2.0 revamp run

> v1.0 — 2026-06-17. Operator-authored (Bo Chen). Read-only to the
> agent except via the `SPEC:` + operator-action protocol (§5).
> Imported by CLAUDE.md. The single highest-leverage file in the run.

## 0. Run identity

- **Run ID:** `2026-06-17-T00:00` (revamp run)
- **Operator brief:** Revamp Bay from v0.1.1 → v0.2.0 per the approved
  master plan: fix bugs, mechanically enforce the four load-bearing
  invariants (events append-only; projection pure; swap atomic; caps
  active-only), reconcile doctrine drift, then expand across four
  feature tiers (I-15..I-27). Hold the six principles and the
  "Cut from v1" list intact throughout.
- **Substrate:** ZCode agent (builtin:zai/GLM-5.2), Claude Code v2.1+-
  class harness, Max-tier equivalent.
- **Posture:** Heavy overnight-capable autonomy spine. Atomic save
  points every increment + every 25–30 min. Compaction-indifferent at
  every save point.
- **Rollback floor:** git tag `autonomous-run-2026-06-17-start`
  (placed at Phase 0 close). Operator-only `git reset --hard` to here
  is the recovery of last resort.

## 1. Run boundaries

**Modify within (whitelist):**
- `src/` — frontend (React + TS)
- `src-tauri/src/` — Rust backend
- `src-tauri/Cargo.toml` — deps (per §3 rules)
- `migrations/` — **new migration files only** (e.g. `002_*.sql`,
  `003_*.sql`); `001_initial.sql` is `NEVER_MODIFY`
- `contracts/` — new directory for typed contracts + golden cases
- `scripts/` — dev utilities
- `package.json`, `tsconfig.json`, `vite.config.ts` — config
- `CLAUDE.md`, `SPEC.md`, `PROMPTS.md`, `README.md` — doctrine/docs
  via the archive-and-diff `SPEC:` protocol (§5)
- `.claude/` — hooks, skills, agents (the harness IS part of the
  system)
- New top-level run-state files (RUN_STATE, TASKLIST, DECISIONS,
  QUESTIONS, BLOCKERS, VERIFIED, COMPACTION_BRIEF, PROGRESS,
  AUTONOMY_CHARTER, marrow.lock, run-metrics.jsonl, .run-lock)

**NEVER_MODIFY (blacklist — whitelist wins on conflict):**
- `archive/` — historical doctrine versions; append-only archive
- `migrations/001_initial.sql` — locked v1 schema; new migrations add,
  never edit
- `LICENSE` — MIT, frozen
- `scripts/rank-fixtures.json` — operator-owned parity fixture
  (par­ity tests read it; regen only via
  `src-tauri/src/bin/rank_fixture_gen.rs` and only with `SPEC:` tag)
- `contracts/golden/*.json` `GOLDEN` blocks — operator-owned ground
  truth once frozen (§3 of charter / Phase 2c of plan)
- `AUTONOMY_CHARTER.md` itself — operator-owned
- `.git/` — git internals

## 2. Quota posture

`QUOTA_BINDING: false` — Max-tier-equivalent; operator owns quota.
Agent tracks consumption for audit (run-metrics.jsonl) but does not
slow down to preserve quota.

## 3. Pre-authorized decisions

Cases. If matched, execute per the standing instruction; no QUESTION
needed.

- **Lint/format fails:** fix lint, proceed.
- **Missing authorized dev-dep:** add to `Cargo.toml` `[dev-dependencies]`
  or `package.json` `devDependencies`, run install, proceed. Authorized
  for this run: `proptest` (Rust, property tests). Any other new dep
  requires a DECISIONS.md ADR + `SPEC:` tag.
- **Missing authorized runtime dep:** `@tanstack/react-virtual` for
  I-16 (C-tier virtualization, SPEC §10.12-resolved). Requires an ADR
  in DECISIONS.md but pre-authorized.
- **Test fails:** diagnose; fix if obvious within 3 attempts; else mark
  SUSPECT, file BLOCKER, advance.
- **Migration needed on dev DB:** write a new numbered migration file
  (`002_*.sql`+), run via the existing migration runner. Never edit
  `001_initial.sql`. Never touch non-dev DBs (none exist in this
  single-user local-first app, but the rule holds).
- **Doc conflicts with code:** code is canonical; update doc, commit
  with `DECISION:` tag, proceed. Exception: doctrine docs
  (CLAUDE/SPEC/PROMPTS) require the archive-and-diff `SPEC:` protocol
  (§5).
- **Refactor within a single module:** allowed (no contract changes,
  no cross-module impact). Tag commit `refactor:`.
- **Create new files within scope dirs:** allowed.
- **Run tests, format code, regenerate rank fixtures:** allowed.
- **Add characterization/property/golden-case tests:** allowed and
  encouraged.
- **Commit work in progress:** allowed and encouraged (`WIP:` prefix
  for non-save-point commits; atomic save points use the PROMPTS.md §4
  template).
- **Update run-state files (RUN_STATE/TASKLIST/PROGRESS/etc.):**
  required at every save point.

## 4. Default-and-log policy

When a decision is needed that is **not** pre-authorized (§3) and
**not** hard-prohibited (§5): apply the obvious default, log in
DECISIONS.md as an ADR, tag commit `DECISION:`, proceed.

Default = most reversible option when ambiguous.

## 5. Hard prohibitions (inviolable regardless of reasoning chain)

- **Force-push** to `main` or any protected branch.
- **Local destructive git:** `git reset --hard`, `git clean -fd/-fx`,
  `git checkout -- <path>`, `git restore` on uncommitted/unmerged work;
  `git worktree remove --force` with uncommitted changes;
  `git branch -D` on an unmerged `auto/` branch;
  `git rebase`/`filter-branch`/reflog-expire on shared history.
- **Any `rm -rf` / `Remove-Item -Recurse -Force`** outside `.chunks/`
  and explicitly-ephemeral dirs.
- **Any `UPDATE events SET ...` or `DELETE FROM events`** — the event
  log is append-only. This is CLAUDE.md doctrine and (after Phase 2b)
  a SQLite trigger-enforced runtime truth. The only write path is
  `db::write_events` → `events::append_event` (INSERT only).
- **Bypass `db::write_events`** for any projection mutation. Every
  `items` write goes through `apply_event_to_projection` inside a
  `write_events` transaction.
- **Modify CI/CD, secrets, `.env*`, credential storage.** (Bay has no
  CI/CD yet, but the rule holds if added.)
- **Irreversible ops on non-dev DBs** (DROP, force-delete,
  destructive migration). Bay has only the dev DB; still, never DROP
  tables — add migrations.
- **Open external accounts / paid services** beyond what's already
  authorized (the optional LLM endpoint, already user-configured).
- **Edit `AUTONOMY_CHARTER.md`** (operator-owned).
- **Edit `archive/*`** (append-only history).
- **Edit `migrations/001_initial.sql`** (locked; new migrations only).
- **Edit `contracts/golden/*.json` `GOLDEN` blocks** once frozen
  (operator-owned ground truth; requires `SPEC:` + operator action).
- **Edit `scripts/rank-fixtures.json`** except via
  `rank_fixture_gen.rs` + `SPEC:` tag.
- **Add auto-tiering, "smart sort," or any LLM write path.** The LLM
  firewall is absolute. I-20 (LLM re-org proposals) is human-accepted
  diffs only — the LLM proposes, the human accepts, the deterministic
  tier owns the write. No auto-mutations, ever.
- **Re-litigate "Cut from v1" items** (tags, subtasks, Eisenhower
  matrix, recurring tasks pre-I-21, custom tier schemes, dark-mode
  toggles pre-I-25, etc.). These are doctrine; if a v2 feature seems
  to require one, file BLOCKER, do not implement.
- **Any action whose undo cost cannot be stated in one sentence →
  route to stage-3** (file QUESTION/BLOCKER), regardless of how
  pre-authorized the *category* seemed. Reversibility gates every
  action, not just speculations.

## 6. Speculation budget

- Max active speculations: **25**.
- At threshold → ESCALATION, terminate run gracefully.
- Inline tag: `// SPECULATION: see QUESTIONS.md#Q<NN>` (or
  `<!-- SPECULATION: -->` for TSX/HTML, `# SPECULATION:` for
  `.sql`/`Cargo.toml`).
- `grep -r "SPECULATION:" .` produces the complete list for operator
  audit on return.

## 7. Run termination conditions

- Hard time limit: **40 wall-clock hours** (multi-session).
- Speculation budget exceeded (25 active).
- BLOCKERS depth > 8.
- Unreviewed-LOC ceiling: **3000** → stop, let operator review.
- **`FOUNDATIONAL_BLOCKER_FANOUT: 3`** — at/above, prefer early clean
  stop over speculative dependents. Log `negative_work_avoided` to
  run-metrics.jsonl.

## 8. Energy-state policy

`OPERATOR_STATE: rested` (assumed at run start).

If the run extends past operator's typical attention window (judged by
PROGRESS cadence degradation): tighten to tired-mode — saves every 15
min, speculation budget 15, broader two-pass, no high-stakes refactors.

## 9. Critical-module classification (two-pass + non-LLM oracle)

These six modules trigger automatic two-pass verification (main
implements; fresh `verifier` subagent with cold context verifies) AND
require a non-LLM oracle (property test + golden cases) before
promotion to DONE:

1. `src-tauri/src/db/mod.rs::write_events` — the atomicity primitive.
   Oracle: property test (rollback on apply failure) + golden
   `swap.json`.
2. `src-tauri/src/db/items.rs::apply_event_to_projection` — the
   projection. Oracle: property test (projection determinism =
   rebuild_projection reproduces items) + golden `projection.json`.
3. `src-tauri/src/commands/items.rs::swap_move_inner` — atomic
   two-event swap. Oracle: property test (atomicity + cap math) +
   golden `swap.json`.
4. `src-tauri/src/domain/rank.rs::rank_between` — fractional
   indexing. Oracle: property test (strictly-between, monotone) +
   existing `rank-fixtures.json` golden.
5. `src-tauri/src/commands/events.rs::get_items_at_inner` —
   time-travel replay. Oracle: property test (monotone;
   `get_items_at(now) == list_active_items`) + golden `projection.json`.
6. `src-tauri/src/commands/events.rs::rebuild_projection_inner` —
   full replay. Oracle: property test (idempotent; reproduces items
   from events) + golden `projection.json`.

Non-critical modules ship single-pass with characterization tests.

## 10. Dynamic charter expansion policy

`CHARTER_EXPANSION: enabled` for this run.

The agent may self-authorize for decisions that are:
- Clearly reversible (file creation, lint rule addition, test
  scaffolding, refactor within a module, naming choices within scope)
- Within run boundaries (§1 whitelist)
- Not in hard prohibitions (§5)

**Audit trail required:**
- Log in DECISIONS.md tagged `CHARTER_EXPANSION:` with full rationale.
- Tag commit `CHARTER_EXPANSION:`.
- Append to `PROGRESS.md` for batch operator review.

**NOT allowed under expansion:** anything in §5 prohibitions, anything
that modifies contracts/golden cases, anything irreversible, anything
touching the six critical modules beyond their declared scope.

## 11. MCP server whitelist

`[filesystem, git]` — minimal. No auto-discovery. GitHub not needed
(this is a local repo; `gh` CLI available if needed for PRs but no
PRs planned in this run).

## 12. Golden-case ownership

Golden cases (`contracts/golden/*.json` and the existing
`scripts/rank-fixtures.json`) are **operator-owned** — read-only to the
agent, treated like this charter itself.

- The agent authors golden cases as **proposals** (Phase 2c). The
  operator reviews and freezes them. After freezing, the agent may not
  edit them without `SPEC:` tag + logged operator action.
- CI/hook check: fail the build if (a) a critical-module contract has
  zero golden cases, or (b) a diff touches a `GOLDEN` block without
  `SPEC:` tag + operator action.
- **Joint-wrong detector:** if implementation passes contract tests
  but fails a golden case, log a `JOINT_WRONG:` finding — the most
  dangerous class, because everything else looked green.

---

*This charter is the alignment between operator intent and agent
autonomy. The substrate buys capability; the charter buys alignment;
the spine buys survivability; the Externality Principle buys
correctness.*
