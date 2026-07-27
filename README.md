# Bay

A single-user desktop task manager that enforces capacity as discipline. Four
bays (Inbox, A, B, C); A and B have hard caps (5 and 12 active items); adding
to a full A or B forces a swap modal rather than silently accepting. Every
mutation is logged as an append-only event; the visible board is a projection
that can be rebuilt from the log at any time.

Built with Tauri 2 + React 18 + SQLite. Local-first, no cloud sync, no
accounts, no telemetry.

## Why

Most to-do apps degenerate because they let the inbox grow without bound.
Bay's central claim is that capacity is the product: the caps force you to
swap items rather than add them indefinitely. The ATC strip-bay metaphor is
load-bearing — controllers don't pretend they have more capacity than they
have, and neither should you.

Ten load-bearing design principles (see [CLAUDE.md](CLAUDE.md) for the full
doctrine; 1–6 are original, 7–10 arrived with v0.3):

1. **Capacity bounds.** A=5, B=12 (active items only; blocked and done don't
   count). C and Inbox unbounded. Removing the caps is scope creep.
2. **LLM firewalled out of state.** Optional LLM analysis is advisory only —
   it observes patterns in the event log and may propose a re-org you accept
   or reject. It never mutates items; the type system won't let it.
3. **Event log is the product.** `events` is append-only, and since v0.3
   hash-chained, so tampering is *evident*, not merely forbidden. `items` and
   `sessions` are projections, fully rebuildable from the log. Undo,
   time-travel, analysis, and the Mirror are all queries against it.
4. **Blocked state is real.** Work A unless every A item is blocked or done,
   then work B. Blocked items don't count toward caps.
5. **Capture is load-bearing.** Global hotkey (default Ctrl+Alt+N) and an
   optional LAN server for phone capture. Both go to Inbox.
6. **Asymmetric cross-tier friction.** Intra-tier drag is free; cross-tier
   drag requires a reason modal (and a swap if the target is at cap).
7. **Caps bind flow, not just stock.** Today holds at most 3 active items,
   chosen once in the morning so the rest of the day is re-decision-free;
   at most one focus session runs at a time.
8. **Starting is the cheapest verb, and Bay never interrupts.** A one-line
   *first step* per item, one-click Start, and "tomorrow's first move" chosen
   at day-close. No notifications, badges, or streaks — ever.
9. **The mirror is deterministic and never shames.** Throughput, lead time,
   leak rates, and what you've been avoiding are computed from your own
   recorded behavior with no model configured. Finished work stays visible
   as evidence.
10. **The system acts alone only to execute a timer you set.** Exactly one
    machine write exists — the nightly expiry of Today membership. It's
    logged, it never touches tier or state, and undo ignores it.

## Install & run

Build from source. You'll need:

- Rust (stable, 1.77+) — `rustup` will install the right toolchain
- Node 20+ and pnpm
- Platform build deps:
  - **Windows**: Visual Studio 2022 Build Tools (MSVC linker)
  - **macOS**: Xcode Command Line Tools
  - **Linux**: `libwebkit2gtk-4.1-dev` + `libssl-dev` (Debian/Ubuntu names)

Clone and build:

```bash
git clone https://github.com/bochen2029-pixel/Bay.git
cd Bay
pnpm install
pnpm tauri dev      # run from source
pnpm tauri build    # produce a release binary
```

Data lives in your platform's app-data directory:

- Windows: `%APPDATA%\com.bay.desktop\`
- macOS: `~/Library/Application Support/com.bay.desktop/`
- Linux: `~/.config/com.bay.desktop/` (or `$XDG_CONFIG_HOME`)

The SQLite database (`bay.db`) and settings (`settings.json`) sit alongside
each other. Back them up if you care about the data — Bay does not sync.

## Basic usage

**Capture.** Press the global hotkey (default Ctrl+Alt+N) from anywhere.
Type, Enter. Lands at top of Inbox. Close-to-tray keeps the hotkey live
after you dismiss the window.

**Triage.** Drag items out of Inbox into A, B, or C. A and B cap at 5 and 12
active items; dropping into a full A or B opens a swap modal that makes you
pick which existing item leaves.

**Work.** A is what you're working. B is parked-ready. C is someday.
Everything else is Inbox. Items can be blocked (with a reason), done, or
deleted. Delete shows an undo toast for 10 seconds.

**Today (≤3).** The lane above the board holds the day's commitments. Hit
"Plan today…", pick at most three active A/B items, and that decision is
made — no re-choosing every time you glance at the board. Membership expires
at the day boundary automatically; nothing rolls over and nothing scolds you,
but the expiry is recorded, and the Mirror will show you the gap between what
you planned and what you finished.

**Focus sessions.** Hover any active item and hit ▶ (or Start from the Today
lane). A bar appears with the item, its first step, and elapsed time. End it
one of three ways: **Done** (finishes the item — and spawns its next
occurrence if it repeats), **Pause** (honest, work advanced), or
**Interrupt** with a reason from a fixed five-word list. Only one session runs
at a time. Undo can revert what a session did to the board, but never the
record that you spent the time.

**First step.** Each item can carry one line — the next *physical* action
("open contract.pdf", "dial Marco"). It's deliberately one line, not a
checklist: a checklist is a place to hide from work, a first step is a place
to start it.

**Day close.** One question: what's tomorrow's first move? Answer it tonight
and it's waiting for you at tomorrow's plan-today. That's the whole ritual.

**Repeating items.** Set an item to repeat daily, weekly, or monthly from its
⋯ menu. Marking it done finishes it and creates the next instance in the same
atomic step, with the due date advanced (Jan 31 + 1 month lands on Feb 28, or
the 29th in a leap year). One Ctrl+Z undoes the whole thing.

**Mirror.** Your own numbers, computed locally with no model involved:
throughput and lead time, how often A-items get demoted within 48 hours of
arriving (i.e. whether A has quietly become a second inbox), which committed
items have *zero* recorded focus sessions, what your blocks actually cost in
days, where your interruptions come from, Today planned-vs-finished, and a
running list of what you've finished with the journey each item took.

**Dates.** Each item can carry optional start and due dates. Items render
their dates as badges (`▸ start`, `● due`); overdue active items show red.

**Staleness.** Per-tier thresholds (Inbox 3d, A 14d, B 21d, C disabled)
flag untouched active items with a ⚠. The threshold is the nudge — there's
no alert or modal.

**Calendar.** Monthly grid showing any item with a start or due in that
month. Click a day for the list; click an item to jump back to the board.

**Time-travel.** Scrubber replays the event log to a point in time. Board
renders read-only. Useful for "what was this bay like Tuesday?"

**Command palette.** `Cmd/Ctrl+K` opens a fuzzy palette: jump to any item,
create in a tier, switch views, run Analyze, open Settings, or restore from
the archive — all from the keyboard.

**Undo.** `Ctrl/Cmd+Z` undoes your last action by appending a compensating
event (the event log is the product, so undo is just a query against it).
Works across creates, edits, moves, state changes, deletes, swaps — and
whole batches as one step.

**Batch operations.** Tick the checkbox on any strip (Shift-click for a
range) to multi-select, then mark done, mark active, or delete the whole
selection at once. Each batch is one atomic transaction and one undo step.

**Audit log.** The Audit view is full-text search over the event log —
filter by type, item, or date. The log is a first-class surface, not a
hidden implementation detail.

**Analyze (optional).** If you've configured an LLM endpoint (OpenAI,
Ollama, LM Studio, etc.), the Analyze button compresses the last N days of
activity into aggregates and asks the LLM for pattern observations. The LLM
may also propose a small re-org — move or complete a few items — shown as a
diff you accept or reject per item. Nothing is applied until you accept, and
even then the deterministic core does the write under the same cap rules.
The LLM never touches your items directly; the event log records what it
said and what you did with it.

**LAN capture (optional).** Enable in Settings → Capture. A tiny HTTP
server binds on your LAN port (default 47821). Scan the QR on your phone to
open a capture page; submissions arrive in Inbox. Optional shared-secret
hardening for coffee-shop WiFi.

## Keyboard

| Shortcut | Effect |
|---|---|
| `Ctrl+Alt+N` (configurable) | Open quick-capture modal |
| `Cmd/Ctrl+K` | Open the command palette |
| `Ctrl/Cmd+Z` | Undo the last action (incl. batches) |
| `Enter` in modal | Commit to Inbox |
| `Ctrl+Enter` in modal | Commit + open inspector |
| `Esc` in any modal / clears a selection | Cancel |
| `Shift+Enter` in textareas | Newline |
| Strip checkbox (`Shift`-click) | Multi-select for batch ops |
| Strip `▶` (on hover) | Start a focus session |
| Drag grip `≡` | Reorder / move |

## Architecture

Event-sourced, local-first, single-process.

```
┌─────────────┐     invoke      ┌──────────────┐
│  React UI   │  ◂──────────▸   │  Rust core   │
│ (Zustand +  │     events      │  (tauri 2)   │
│  @dnd-kit)  │                 │              │
└─────────────┘                 │   write_event│
                                │       ↓      │
                                │   append to  │
                                │   events[]   │
                                │       ↓      │
                                │   apply to   │
                                │   items (tx) │
                                └──────┬───────┘
                                       │
                                 ┌─────▼─────┐
                                 │ bay.db    │
                                 │ (SQLite)  │
                                 └───────────┘
```

Key invariants:

- `db::write_event` / `db::write_events` is the only place events are
  written. Direct append-without-apply (or apply-without-append) is a bug.
- `apply_event_to_projection` is a single exhaustive match on `EventType`.
  Adding a new variant forces the compiler to drag you back here.
- Every Tauri command is a thin wrapper over a pure `*_inner` function that
  takes `&SqlitePool` and can be tested without Tauri scaffolding.

Project layout:

```
src/                    frontend (React + TS)
  App.tsx               orchestrator, DnD context, top bar
  store.ts              Zustand store (items + derived itemsByTier)
  domain.ts             Zod schemas mirroring Rust types
  rank.ts               lex fractional-indexing (TS port of Rust)
  swap.ts               needsSwap helper
  staleness.ts          per-tier staleness predicate
  components/           Strip, modals, panels, CalendarView, etc.
src-tauri/              Rust backend
  src/db/               pool, migrations, write_event wrapper, handlers
  src/domain/           Item, Tier, ItemState, EventType, rank_between
  src/commands/         Tauri #[command] functions
  src/llm/              OpenAI-compatible client + analyze
  src/capture/          LAN capture axum server
  src/settings.rs       settings JSON load/save
  src/keychain.rs       API key via OS keychain
migrations/             SQL schema files (linear, forward-only)
scripts/                dev utilities (schema verify, store-logic smoke)
archive/                historical doctrine versions
```

## Development

Verify locally:

```bash
# Rust
(cd src-tauri && cargo build)
(cd src-tauri && cargo test)      # 75 tests

# Frontend
pnpm build                        # tsc --noEmit + vite build
node scripts/test-store-logic.mjs # pure-logic smoke, 55 assertions

# Schema integrity
python scripts/verify-schema.py   # byte-diff against migrations
```

All tests must pass with zero warnings before any commit.

The build plan lives in [SPEC.md](SPEC.md) §9 as 14 numbered increments.
Per-increment prompts for Claude Code live in [PROMPTS.md](PROMPTS.md) §2.
Prior doctrine versions are archived under [archive/](archive/).

## Non-goals

Explicitly out of scope in v1, by design:

- Tags / labels / categories beyond A/B/C/Inbox
- Subtasks or checklists within items (a *first step* is one line, on purpose)
- Cloud sync, accounts, multi-user, or any network dependency beyond the
  optional LLM endpoint
- Eisenhower matrix, urgency × importance scoring, any second prioritization
  axis
- LLM auto-tiering or any LLM write path — the firewall is absolute
- Email / SMS / push notifications — Bay never pings you
- Gamification: points, streaks, levels, confetti. A streak turns one bad
  Tuesday into cascading shame; the receipts list is the honest version
- Auto-planning: the machine never fills your Today or orders your day
- Manual time estimates — sessions measure, so nothing needs guessing
- Custom tier schemes (A/B/C/D, user-defined names)
- Dark-mode toggles, theme customization, icon packs (system theme
  followed, nothing more)

Recurring tasks were on this list through v0.2 as a sanctioned v2 candidate;
they shipped in v0.3. The rest stand.

## Versioning

- v0.1.0 — initial release. All 14 increments from the build plan.
- v0.1.1 — cleanup pass: frontend test harness, rank parity tests, archive
  view, LAN-capture toast, close-to-tray toggle.
- v0.2.0 — correctness layer (property tests, DB-enforced invariants,
  operator golden cases, type-level LLM firewall) plus command palette,
  undo, audit search, batch operations, and LLM re-org proposals.
- v0.3 (unreleased) — **Execution.** Golden cases now execute in CI; the
  event log gained a provenance envelope and a hash chain (tamper-evident,
  not just append-only); undo groups by transaction; recurring tasks;
  and the execution core — Today, focus sessions, first steps, day
  rituals, and the Mirror.

## License

MIT — see [LICENSE](LICENSE). Fork, adapt, ship; an issue or note back if
something is useful is welcome but not required.
