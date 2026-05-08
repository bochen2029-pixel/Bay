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

Six load-bearing design principles (see [CLAUDE.md](CLAUDE.md) for the full
doctrine):

1. **Capacity bounds.** A=5, B=12 (active items only; blocked and done don't
   count). C and Inbox unbounded. Removing the caps is scope creep.
2. **LLM firewalled out of state.** Optional LLM analysis is advisory only —
   it observes patterns in the event log and surfaces observations. It
   never mutates items.
3. **Event log is the product.** `events` is append-only. `items` is a
   materialized projection, fully rebuildable from the log. Undo,
   time-travel, and analysis are all queries against the log.
4. **Blocked state is real.** Work A unless every A item is blocked or done,
   then work B. Blocked items don't count toward caps.
5. **Capture is load-bearing.** Global hotkey (default Ctrl+Alt+N) and an
   optional LAN server for phone capture. Both go to Inbox.
6. **Asymmetric cross-tier friction.** Intra-tier drag is free; cross-tier
   drag requires a reason modal (and a swap if the target is at cap).

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

**Dates.** Each item can carry optional start and due dates. Items render
their dates as badges (`▸ start`, `● due`); overdue active items show red.

**Staleness.** Per-tier thresholds (Inbox 3d, A 14d, B 21d, C disabled)
flag untouched active items with a ⚠. The threshold is the nudge — there's
no alert or modal.

**Calendar.** Monthly grid showing any item with a start or due in that
month. Click a day for the list; click an item to jump back to the board.

**Time-travel.** Scrubber replays the event log to a point in time. Board
renders read-only. Useful for "what was this bay like Tuesday?"

**Analyze (optional).** If you've configured an LLM endpoint (OpenAI,
Ollama, LM Studio, etc.), the Analyze button compresses the last N days of
activity into aggregates, asks the LLM for pattern observations, and shows
them. The LLM never touches your items — only the event log, via Mark
reviewed / Dismiss, records what it said and what you did with it.

**LAN capture (optional).** Enable in Settings → Capture. A tiny HTTP
server binds on your LAN port (default 47821). Scan the QR on your phone to
open a capture page; submissions arrive in Inbox. Optional shared-secret
hardening for coffee-shop WiFi.

## Keyboard

| Shortcut | Effect |
|---|---|
| `Ctrl+Alt+N` (configurable) | Open quick-capture modal |
| `Enter` in modal | Commit to Inbox |
| `Ctrl+Enter` in modal | Commit + open inspector |
| `Esc` in any modal | Cancel |
| `Shift+Enter` in textareas | Newline |
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
- Subtasks or checklists within items
- Cloud sync, accounts, multi-user, or any network dependency beyond the
  optional LLM endpoint
- Eisenhower matrix, urgency × importance scoring, any second prioritization
  axis
- Recurring / repeating tasks
- LLM auto-tiering or any LLM write path — the firewall is absolute
- Email / SMS / push notifications
- Custom tier schemes (A/B/C/D, user-defined names)
- Dark-mode toggles, theme customization, icon packs (system theme
  followed, nothing more)

Some are reasonable v2 candidates (LLM re-org proposals as atomic accept/
reject diffs, recurring tasks, archive view). None belong in v1.

## Versioning

- v0.1.0 — initial release. All 14 increments from the build plan.

## License

MIT — see [LICENSE](LICENSE). Fork, adapt, ship; an issue or note back if
something is useful is welcome but not required.
