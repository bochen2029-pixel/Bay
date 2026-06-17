# CLAUDE.md — Bay

> v1.6 — 2026-05-07. Refreshed "Current state" section to reflect
> v0.1.1 (the v1.1 cleanup pass shipped: vitest+RTL harness, rank
> Rust↔TS parity, mockito HTTP mocks for the LLM client, dedicated
> LAN-capture toast, close-to-tray promoted to settings toggle, and
> Archive view for soft-deleted items). Doctrine principles and "Cut
> from v1" list unchanged. Co-pass with SPEC.md v1.4 → v1.5 reconciling
> §10.3 / §10.10 / §5.1 with the shipped surfaces. Prior versions at
> archive/CLAUDE_v1.0.md … archive/CLAUDE_v1.5.md.

> Working title. Rename at will. Placeholder chosen to match the ATC strip-bay metaphor the design derives from. If renamed, update this file, `package.json`, `tauri.conf.json`, and any user-facing strings.

## Resumable context — read this first if you are a fresh Claude Code session

Accumulated cross-session context for this project lives at:

    C:\Users\user\.claude\projects\C--Bay\memory\

Start by reading `MEMORY.md` in that directory — it is a one-line-per-entry index pointing at per-topic files:

- `user_bo.md` — user profile + collaboration preferences
- `project_bay.md` — project overview and six load-bearing principles
- `project_bay_state.md` — current build state (what's shipped, known tech debt)
- `feedback_bay_discipline.md` — 13 patterns to preserve, anti-patterns to reject
- `feedback_bay_sessions.md` — session protocol, doctrine-pass rules, autonomous-mode rhythm
- `reference_bay_architecture.md` — codebase map, tool commands, DB path, build deps
- `reference_bay_doctrine.md` — role of CLAUDE.md / SPEC.md / PROMPTS.md, version history

The Claude Code auto-memory harness normally loads `MEMORY.md` automatically at session start. If it hasn't (different harness, different entry path, compacted context), read those files manually before touching code. The doctrine and discipline there encode decisions that are not obvious from the current source alone.

After the memory scan, continue with the three-file doctrine in this repo: CLAUDE.md (this file) → SPEC.md → PROMPTS.md. Each file's top-of-file blockquote gives its version and revision history.

## Purpose

This directory builds **Bay**, a single-user desktop task manager whose core design principle is **capacity as discipline**. Unlike conventional to-do apps, which degenerate because they let the inbox grow without bound, Bay enforces hard caps on the highest-priority tiers and forces the user to swap items rather than add them indefinitely. The ATC strip-bay metaphor is load-bearing: controllers don't pretend they have more capacity than they have, and neither should the user.

The companion goal is **perfect self-auditability**. Every user action is recorded as an append-only event. The visible board is a projection of the event log at time T. Undo, time-travel, pattern analysis, and LLM advisory features all derive from this single source of truth.

## What lives here

- `CLAUDE.md` — this file. Operating doctrine, locked decisions, scope boundaries.
- `SPEC.md` — detailed component spec, wireframes, SQL schema, event payload JSON schemas, build plan.
- `src/` — Tauri + React + TypeScript application source.
- `src-tauri/` — Rust backend (event store, SQLite, LAN capture server, keychain integration).
- `migrations/` — SQL schema migrations.
- `README.md` — user-facing install/use docs.

## If you (Claude Code) are invoked in this directory

Most likely tasks, in descending order of probability:

1. **Implement a feature from `SPEC.md`.** Read `CLAUDE.md` and `SPEC.md` top to bottom before writing code. Do not improvise features not in SPEC; flag deviations in comments and in the commit message.
2. **Debug or extend existing code.** Check event-log integrity first — most bugs in event-sourced systems are projection drift. Replay the log against the projection to verify consistency before hunting elsewhere.
3. **Extend `SPEC.md`.** Revisions must explicitly note what changed and why. Never silently drop scope from CLAUDE.md's "Cut from v1" list; promotion to v1 requires an explicit note here.

## Design philosophy — load-bearing, do not dilute

These are not preferences. They are the claims the product rests on. Violating any of them produces a different, worse product.

### 1. Capacity bounds A and B

- **A cap = 5.** Attempting to add a 6th A forces the user to demote an existing A first.
- **B cap = 12.** Same swap-or-reject rule.
- **C unbounded.** C is the later/someday/backlog bucket.
- **Inbox unbounded.** Inbox is the triage staging area; items arrive here from capture and must be hand-tiered into A/B/C before they are actionable.

Removing the caps = reinventing Todoist. The caps are the product.

### 2. LLM firewalled out of the decision path

The LLM **never mutates state**. The deterministic tier (typed event handlers, SQLite) owns all writes. The LLM observes the event log and produces advisory output only: pattern summaries, staleness callouts, inconsistency flags. Re-org proposals (v2+) must be presented as an atomic accept/reject diff — never incremental silent edits.

This is compiler/runtime separation applied at app scale. The user's prioritization judgment is the product. An LLM that auto-tiers items removes the cognitive work the app was built to force. **Do not add auto-tiering. Do not add "smart sort." Do not.**

### 3. Event log is the product

`events` is append-only. Nothing in the system updates or deletes past events. The `items` table is a materialized projection, rebuildable from the log at any time. Every feature — undo, time-travel, analytics, LLM advisory — is a query against the log.

If you ever find yourself writing `UPDATE events SET …` or `DELETE FROM events`, stop. You have misunderstood the architecture.

### 4. Blocked state is real

Rigid "A-exhausted-before-B" is wrong. Real rule: **work A unless all active A items are blocked or done, then work B**. Blocked items carry an optional reason string and do not count against "A is active." When the blocker resolves, the user returns the item to active.

### 5. Capture is load-bearing

The app must capture items from wherever the thought arrives. Desktop-only capture is insufficient. Minimum capture surfaces for v1:

- Global OS hotkey → quick-capture modal → Inbox.
- LAN-accessible HTML form served by the Tauri backend on port `47821` → same endpoint → Inbox. User hits this from their phone on the same wifi.

Both go to Inbox, never directly to A/B/C. Tiering is deliberate human work.

### 6. Asymmetric friction on cross-tier moves

Intra-tier reorder = drag, no friction, single event. Cross-tier move = drag + confirmation modal + optional reason string. Reasons log to the event stream. Over weeks the log exposes whether A is being used correctly or is leaking into the inbox role.

## Architecture — locked

- **Shell**: Tauri v2. Rationale: small binary, low memory footprint, Rust backend gives a real database handle and a real HTTP listener without shipping Node.
- **Frontend**: React 18 + TypeScript + Vite.
- **DnD**: `@dnd-kit/core`. Not `react-dnd` — better performance, no HTML5-backend quirks.
- **Storage**: SQLite via `rusqlite` in the Rust backend. Event-sourced schema.
- **LAN capture**: Rust `axum` server bound to `0.0.0.0:47821`, serving one HTML form page and accepting `POST /capture`. Disabled by default; toggle in settings. Binds only to local-network interfaces.
- **LLM integration** (optional, off by default):
  - Local: Ollama or LM Studio via OpenAI-compatible endpoint.
  - Remote: user-supplied API key (Anthropic, OpenAI). Key stored in OS keychain via `keyring` crate, never in config files or SQLite.
- **No cloud sync. No account system. No telemetry.** Single-user, local-first, forever.

## Data model — locked

```sql
-- Append-only. Nothing updates or deletes rows here.
CREATE TABLE events (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  ts        INTEGER NOT NULL,                    -- unix ms
  type      TEXT NOT NULL,                       -- enum below
  item_id   TEXT,                                -- nullable for non-item events
  payload   TEXT NOT NULL                        -- JSON
);
CREATE INDEX idx_events_item ON events(item_id);
CREATE INDEX idx_events_ts   ON events(ts);

-- Projection. Rebuildable from events at any time.
CREATE TABLE items (
  id              TEXT PRIMARY KEY,
  content         TEXT NOT NULL,
  tier            TEXT NOT NULL CHECK (tier IN ('inbox','A','B','C')),
  rank            TEXT NOT NULL,                 -- lexicographic fractional indexing
  state           TEXT NOT NULL CHECK (state IN ('active','blocked','done')),
  blocked_reason  TEXT,
  start_at        INTEGER,                       -- unix ms, nullable
  due_at          INTEGER,                       -- unix ms, nullable
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  deleted         INTEGER NOT NULL DEFAULT 0     -- soft-delete flag
);
CREATE INDEX idx_items_tier_rank ON items(tier, rank) WHERE deleted = 0;
```

### Event types — exhaustive for v1

| Type | Payload |
|---|---|
| `ITEM_CREATED` | `{content, tier, rank, start_at?, due_at?}` |
| `ITEM_EDITED` | `{content_before, content_after}` |
| `ITEM_MOVED` | `{tier_before, rank_before, tier_after, rank_after, reason?}` |
| `ITEM_STATE_CHANGED` | `{state_before, state_after, blocked_reason?}` |
| `ITEM_DATE_SET` | `{field: 'start'\|'due', value_before, value_after}` |
| `ITEM_DELETED` | `{soft: true}` |
| `ITEM_RESTORED` | `{}` |
| `LLM_SUGGESTION_GENERATED` | `{kind, scope, content}` |
| `LLM_SUGGESTION_ACCEPTED` | `{suggestion_event_id, resulting_event_ids: [...]}` |
| `LLM_SUGGESTION_REJECTED` | `{suggestion_event_id, reason?}` |

Rank uses lexicographic fractional indexing: insert between two items by computing a string strictly between their existing ranks (see SPEC §4.2). Rebalance is deferred in v1 per SPEC §10.4; the helper tolerates rank strings well beyond realistic solo usage. Atomic swaps (cap-enforcement demotions) must emit both events in a single transaction.

## Interaction rules — locked

- **Add to Inbox**: always allowed.
- **Add to A when |A_active| < 5**: allowed.
- **Add to A when |A_active| = 5**: swap modal — "A is full. Which item leaves A?" User selects an A item and its destination (B or C). Both moves log atomically in one transaction.
- **Same swap rule for B** at cap 12.
- **Blocked and done items do not count against caps.** A cap applies to `state = 'active'` items only.
- **Drag within tier**: reorder, no modal, single `ITEM_MOVED` event.
- **Drag across tiers**: modal with optional reason string. Logs `ITEM_MOVED` with reason.
- **Mark done**: state → done. Strip collapses to one-line strikethrough row, stays visible until next app launch; session-scoped reveal via per-bay "Show N earlier done items" link. Logs `ITEM_STATE_CHANGED`.
- **Mark blocked**: modal requiring reason. State → blocked. Visual marker. Logs `ITEM_STATE_CHANGED` with reason.
- **"A exhausted" signal**: when every active A item is blocked or done, the UI surfaces a message inviting B work. Advisory only. User can still work A.
- **Staleness**: items untouched past their tier's threshold flagged visually. Per-tier thresholds (Inbox 3d, A 14d, B 21d, C off) configurable in Settings — see SPEC §5.3. No nag modals. Visibility is the nudge.

## Views — v1

- **Board** (default): four vertical bays — Inbox, A, B, C — with strips. A and B show capacity indicators (`3/5`, `9/12`). Full A or B refuses drops and prompts swap.
- **Calendar**: monthly grid. Renders items with `start_at` or `due_at` only. No inference. No recurring tasks.
- **Time-travel**: date/time picker. Replays events up to that timestamp. Read-only board view of history.
- **Inspector**: click any item → side panel showing full event history for that item.

## LLM scope v1 — do not exceed

**Pattern surfacing only, on-demand.** User presses "Analyze" button. LLM receives a compressed representation of the recent event log (last 30 days or last N events — determined in SPEC) plus the current board state, and produces observations. Acceptable output:

- "You created 23 A items this month and completed 6. Your A-tier is inflated relative to throughput."
- "This A item has been untouched for 14 days. Promote, demote, or delete — those are the honest options."
- "You move items from A to C within 48 hours of creation 40% of the time. A is being used as an inbox."

Unacceptable in v1:

- Auto-ranking new items.
- Silently reorganizing the board.
- Suggesting specific tier assignments at capture time.
- Any action that bypasses the explicit accept/reject event path.

Every LLM-generated suggestion emits `LLM_SUGGESTION_GENERATED` to the event log. Any user response emits `LLM_SUGGESTION_ACCEPTED` or `LLM_SUGGESTION_REJECTED`. This data feeds its own later analysis.

## Capture surfaces — v1

- **Desktop global hotkey** (default `Ctrl+Alt+N`, configurable): shows lightweight input field, user types or pastes, `Enter` commits to Inbox.
- **LAN capture page**: Rust backend serves a single HTML page at `http://<desktop-LAN-IP>:47821/` with one `<textarea>` and a submit button. `POST /capture` with `{content}` creates an Inbox item. CORS locked to same-origin; LAN-trust by default with optional shared-secret hardening (see SPEC §7.4); off by default, user enables in settings. Display the LAN URL and a QR code in the settings panel.

Voice transcription is out of scope for v1. The phone's native dictation into the LAN capture page is sufficient.

## Cut from v1 — do not add

These were considered and rejected. Adding them is scope creep.

- Tags, labels, categories beyond A/B/C/Inbox.
- Subtasks or checklists within items.
- Multi-user, accounts, cloud sync, any network dependency beyond the optional LLM endpoint.
- Eisenhower matrix, scoring overlays, urgency × importance axes, any second prioritization scheme.
- Recurring / repeating tasks.
- LLM auto-tiering or any LLM write path.
- Email / SMS / push notifications.
- Custom tier schemes (A/B/C/D, user-defined names, etc.).
- Dark-mode toggles, theme customization, icon packs. System theme followed, nothing more.

Some are reasonable v2 candidates (LLM re-org proposals; capture-time tier suggestions as accept/reject; recurring tasks). None belong in v1.

## Parent directory context

None assumed. If this directory ends up nested under another project root with its own `CLAUDE.md`, that parent's doctrine does not apply here. This file is authoritative for this subtree.

## Current state

**v0.1.1 shipped.** All 14 build-plan increments (`I-01..I-14`) landed at
`v0.1.0` (2026-04-24). A disciplined v1.1 cleanup pass landed at `v0.1.1`
(2026-05-05): vitest + @testing-library/react render harness with jsdom
dialog polyfill (closes the rendering-bug class motivated by the
post-I-14 overflow-clip bug `1912e5b`); `rank_between` Rust↔TS
fixture-driven parity test (caught a real empty-`b` crash in the TS port);
`mockito` HTTP mocks for `OpenAiCompatClient`; dedicated
`lan_capture_received` toast; close-to-tray promoted from hardcoded
behavior to a Settings → General toggle (default true); and a top-level
`<ArchiveView>` for soft-deleted items, fulfilling the SPEC §10.3
"alternative" deferred from v1.

Doctrine versions at this commit: CLAUDE.md v1.6, SPEC.md v1.5,
PROMPTS.md v1.3. Public on GitHub at
https://github.com/bochen2029-pixel/Bay (MIT).

Next action is yours: residual cleanup (the only empirically motivated
debt left is a React DevTools Profiler sibling-render automated check;
rank rebalance is intentionally deferred per SPEC §10.4) or v2 scope
(LLM re-org accept-path, recurring tasks, LLM streaming). Both should
follow the increment-prompt rhythm in PROMPTS.md §2 with archive-and-diff
doctrine discipline.
