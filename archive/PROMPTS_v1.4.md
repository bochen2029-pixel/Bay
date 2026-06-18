# PROMPTS.md — Bay

> v1.4 — 2026-06-17. v0.2.0 revamp in progress. §1 principle count
> unchanged (six). The §2 increment corpus gains I-15..I-27 as the
> v0.2.0 phases ship (Phase 4: I-15 palette, I-16 C-tier
> virtualization, I-17 undo/redo, I-18 audit-log search, I-19 batch
> ops; Phase 5: I-20 LLM re-org diffs, I-21 recurring, I-22 streaming;
> Phase 6: I-23 sync, I-24 multi-profile, I-25 theming, I-26 plugin
> surface, I-27 mobile companion). Each increment prompt is added with
> the same scope/out-of-scope/demo/verify structure as I-01..I-14 when
> its phase lands. §4 commit template gains a `DECISION:`/`SPEC:`/
> `CHARTER_EXPANSION:` prefix note matching the autonomy spine. Co-pass
> with CLAUDE.md v1.6 → v1.7 and SPEC.md v1.5 → v1.6. Prior versions
> in archive/.

> Operational companion to `CLAUDE.md` (doctrine) and `SPEC.md` (implementation detail). Copy-paste-ready prompts for Claude Code sessions. Stable across SPEC revisions because increment prompts reference `SPEC.md §9 I-NN` rather than restating increment content.

## 0. How to use this file

- **One increment per session.** Start a fresh Claude Code session for each increment. Do not chain I-07 onto the tail of I-06's session; context bloat and scope creep are structural, not moral.
- **Copy the whole block.** Each prompt is self-contained. Don't edit it before pasting; the specific wording resists predictable failure modes for that increment.
- **Review before committing.** Each prompt ends with an explicit demo + verification list. Run it yourself. Do not accept "I believe this should work."
- **Commit with the template in §4.** Every commit references the increment and the event log state it produced.

---

## 1. Session opening (universal)

Prepend to every session. Factored out here for reference; already embedded in each increment prompt.

```
You are Claude Code operating on the Bay project. Before writing any
code, read these files top to bottom, in order:

  1. CLAUDE.md        — design doctrine, locked scope, what NOT to add
  2. SPEC.md          — implementation specification
  3. PROMPTS.md §1    — this protocol

Acknowledge, in one sentence each:

  a. The six load-bearing design principles in CLAUDE.md §Design philosophy.
  b. The capacity caps and which items they apply to.
  c. The LLM firewall rule.
  d. The event-sourcing invariant (events is append-only; items is a
     projection).

After acknowledging, wait for the increment prompt before writing code.
```

---

## 2. Increment prompts

Each prompt assumes the session opening in §1 has been executed. If starting cold, paste §1 first, wait for acknowledgment, then paste the increment.

### I-01 — Scaffold and empty board

```
Execute SPEC.md §9 I-01 (Scaffold and empty board).

Scope for this session:
- pnpm create tauri-app → Tauri 2, React + TypeScript, Vite
- @tauri-apps/api wired; invoke("bootstrap") returns stub data only
- Render four empty bays: Inbox, A, B, C, in that vertical order
- Bay headers show title and counter (0 for all)
- Top bar with view switcher (Board / Calendar / Time-travel) —
  buttons are rendered but only Board activates; Calendar and
  Time-travel log "not yet implemented" and do nothing
- System-theme CSS via prefers-color-scheme

Out of scope:
- SQLite, migrations, any database code
- Domain types beyond what's needed to compile the stub
- Drag-and-drop libraries
- Any UI component library (no shadcn, no Radix, no Mantine, no Chakra,
  no MUI). Styling is vanilla CSS or CSS Modules only, per SPEC §6.2.
- Placeholder items, fake data, lorem ipsum
- Settings, modals, strips

Demo when done:
- App launches.
- Window shows four empty bays labeled Inbox (0), A (0/5), B (0/12),
  C (0). Top bar visible.
- Clicking Calendar or Time-travel buttons produces a console log or
  toast saying "not yet implemented" and the Board stays rendered.

Verify before claiming done:
- cargo build passes with no warnings
- pnpm build passes
- pnpm dev launches the app and renders the above
- No dependencies in package.json beyond what SPEC §6.2 authorizes

Stop after I-01. Do not start on I-02.
```

### I-02 — Schema, migrations, domain types

```
Execute SPEC.md §9 I-02 (Schema, migrations, domain types).

Scope:
- migrations/001_initial.sql with events and items tables per SPEC §4
- db/mod.rs with rusqlite + r2d2 pool and a migration runner that
  checks PRAGMA user_version and applies numbered migrations
- domain/item.rs, domain/event.rs, domain/rank.rs (rank_between helper),
  domain/capacity.rs (cap constants)
- Matching Zod schemas + TypeScript types in src/domain.ts
- bootstrap command now opens the DB, runs migrations, returns
  { items: [], settings: <defaults> }

Out of scope:
- Any command other than bootstrap
- Writing to the events table (no ITEM_CREATED path yet)
- Any projection logic (I-03)
- Tests beyond compile-time type checks

Demo:
- App launches, DB file created at platform-appropriate app-data path
- `sqlite3 <path> .schema` shows events and items tables with exactly
  the columns, types, and indexes specified in SPEC §4.2
- `sqlite3 <path> "PRAGMA user_version"` returns 1

Verify:
- rank_between has unit tests covering:
    (None, None), (None, Some("m")), (Some("a"), None),
    (Some("a"), Some("c")), (Some("a"), Some("b"))
- Each returns a string strictly between its bounds in lexicographic
  order (or a bounded start/end string when a bound is None).
- TypeScript types compile.

Stop after I-02.
```

### I-03 — Event store + create_item end-to-end

```
Execute SPEC.md §9 I-03 (Event store + create_item end-to-end).

Scope:
- db/events.rs::append_event — single append path, inside a caller-
  provided transaction
- db/items.rs::apply_event_to_projection — handles ITEM_CREATED only
- commands/items.rs::create_item — validates content, computes rank
  (end-of-tier by default), opens tx, appends ITEM_CREATED event,
  applies to projection, commits
- Temporary dev-only button on each Bay header: "+" that calls
  createItem for that tier with placeholder content like "new item"

Out of scope:
- Any other event type handler in apply_event_to_projection
- Capacity enforcement (I-05 adds it)
- Real UI for item creation (I-05)
- Drag (I-06)

Demo:
- Click "+" on Inbox → a row appears in events and items in SQLite
- Click "+" on A → same, with tier = 'A'
- Restart the app → items load (no render yet; verify via bootstrap
  return value in devtools)

Verify:
- events has ts, type='ITEM_CREATED', item_id, payload JSON
- items row matches payload exactly for tier, rank, content, state='active'
- Running: DELETE FROM items; then calling an (imagined future)
  rebuild-projection-from-events command would reconstruct the same
  rows. (You can test this manually with a one-off function now, or
  wait for I-10 time-travel. Do not build a rebuild command in I-03.)

Stop after I-03.
```

### I-04 — Read path: render items from DB

```
Execute SPEC.md §9 I-04 (Read path: render items from DB).

Scope:
- bootstrap returns real items from the items table
- Zustand store populates items and itemsByTier on bootstrap
- itemsByTier selector sorts by rank (lexicographic string compare)
- Strip component: minimal render — drag handle placeholder (no DnD
  yet), content text, overflow-menu placeholder icon (non-functional)
- Bay renders strips in rank order

Out of scope:
- Drag-and-drop
- Overflow menu functionality
- Real "+ Add" button (still uses the I-03 dev button)
- Any mutation beyond create

Demo:
- Click dev "+" on A → new item appears in bay immediately, in last
  position by rank
- Click "+" on Inbox three times → three items appear in Inbox in
  creation order
- Restart → items still there, same order

Verify:
- Items render in rank order, not creation order (they coincide here
  because dev-add uses end-of-tier, but the selector must actually
  sort by rank)
- Counter in BayHeader reflects active item count
- A and B show "N/5" and "N/12"

Stop after I-04.
```

### I-05 — Create UI with capacity check + hotkey capture

```
Execute SPEC.md §9 I-05 (Create UI + hotkey capture).

Scope:
- Remove dev "+" buttons. Replace with proper "+ Add" on each
  BayHeader that opens an inline input (not a modal) for that bay
- For A and B: if count_active(tier) >= cap, "+ Add" is disabled with
  a tooltip "Full — drag an item in (swap will be offered)." Swap UI
  is NOT built here; it's I-07.
- Backend: create_item enforces cap server-side; returns CAP_EXCEEDED
  error for A/B when at cap. Frontend respects this as authoritative.
- Global hotkey via tauri-plugin-global-shortcut. Default Ctrl+Alt+N
  per SPEC §10.1. Registration happens on app startup from settings.
- QuickCaptureModal: single autofocused textarea, Enter commits to
  Inbox, Esc cancels, Ctrl+Enter commits and opens inspector on the
  new item (inspector renders nothing yet; just set selectedItemId)

Out of scope:
- SwapModal (I-07)
- Drag (I-06)
- Hotkey reconfiguration UI (I-11 adds settings UI)

Demo:
- Click "+ Add" on Inbox → inline input → type → Enter → strip appears
- Click "+ Add" on A four times → fifth time disabled
- Close app window. Press Ctrl+Alt+N anywhere on desktop → QuickCapture
  modal appears (app returns from tray; see I-11 for close-to-tray
  wiring — for now, keep app open during test).
- Type, Enter → item appears in Inbox

Verify:
- Backend refuses creating a 6th A even if frontend is tricked
- Hotkey registration logs success on startup
- Ctrl+Enter path sets selectedItemId in store (verify in devtools)

Stop after I-05.
```

### I-06 — Intra-tier drag reorder

```
Execute SPEC.md §9 I-06 (Intra-tier drag reorder).

Scope:
- Add @dnd-kit/core + @dnd-kit/sortable + @dnd-kit/utilities to
  package.json
- Bay becomes SortableContext. Strip uses useSortable.
- onDragEnd, if source tier == target tier:
  - Compute new rank via rank_between(prev.rank, next.rank)
  - Call move_item command with { id, to_tier: same, to_rank: computed }
  - Backend emits ITEM_MOVED and updates projection
- If source tier != target tier, do nothing for this increment (revert
  the drag). Cross-tier is I-07.
- move_item validates no-op moves (rejects tier_before == tier_after AND
  rank_before == rank_after)

Out of scope:
- Cross-tier drag (I-07)
- MoveReasonModal (I-07)
- Swap flow (I-07)
- Drag preview/overlay beyond @dnd-kit defaults

Demo:
- Drag an A item above another A item → order changes, persists across
  reload
- Drag an A item over a B item → drop is rejected, A item returns to
  original position
- sqlite3 query: SELECT * FROM events WHERE type = 'ITEM_MOVED' shows
  a row per intra-tier drag, with tier_before == tier_after

Verify:
- Strip identity stable across reorders (use id as key; no remount
  during drag or useSortable breaks)
- Rank strings remain short (inspect items.rank in DB — should not be
  blowing up length)
- No cross-tier rows exist in ITEM_MOVED events yet

Stop after I-06.
```

### I-07 — Cross-tier drag with reason + swap flow

```
Execute SPEC.md §9 I-07 (Cross-tier drag + swap).

This is the trickiest increment. Read SPEC §3.3 and §3.4 twice before
writing code.

Scope:
- onDragEnd, source tier != target tier:
  - If target in {A, B} and count_active(target) >= cap(target) and
    item.state == 'active':
      - Open SwapModal (see SPEC §2.2)
      - On confirm: call swap_move command (§5.1) — backend emits both
        ITEM_MOVED events in a single SQLite transaction
      - On cancel: drag is fully reverted; no events emitted
  - Else:
      - Open MoveReasonModal (see SPEC §2.5); reason is optional
      - On confirm: call move_item with reason
      - On cancel: drag reverted
- Backend: implement swap_move per SPEC §5.1 and §3.3 invariants. The
  transaction must:
  - append ITEM_MOVED for the leaving item
  - apply leaving's projection update
  - append ITEM_MOVED for the entering item
  - apply entering's projection update
  - commit; rollback on any error
- Blocked and done items dragged into A/B do NOT trigger swap (they
  don't count toward cap). They just open MoveReasonModal.

Out of scope:
- Adding new items directly to full A/B via "+ Add" with swap prompt
  (currently "+ Add" is disabled when full — sufficient for v1; revisit
  only if Bo asks)
- LLM-suggested swaps

Demo:
- Drag A item to C → MoveReasonModal appears → type reason → confirm
  → item in C, event has reason
- Fill A to 5. Drag a B item into A → SwapModal appears. Pick an A
  item, choose B as destination, confirm → two events, both committed,
  both visible in event log atomically (no half-state on crash)
- Cancel the swap modal → no events, drag reverts

Verify:
- sqlite3 "BEGIN; SELECT * FROM events ORDER BY id DESC LIMIT 2;"
  shows both events have adjacent ids and close timestamps (same tx)
- Kill the app mid-swap (with a breakpoint between the two appends,
  simulated via a deliberate panic in a test branch): restart, verify
  neither event persisted (transaction rolled back). REMOVE the test
  panic before committing.
- Blocked and done items moving to a full A do not trigger SwapModal

Stop after I-07. If this increment exceeds ~600 LOC, split into I-07a
(cross-tier with reason) and I-07b (swap flow) and commit I-07a alone.
```

### I-08 — State transitions (block / done) + overflow menu

```
Execute SPEC.md §9 I-08 (State transitions).

Scope:
- Strip overflow menu (· · ·): Edit, Set Start Date, Set Due Date,
  Mark Done, Mark Blocked, Delete. Set-date items are stubbed (open a
  toast "coming in I-09"); others are live.
- BlockModal per SPEC §2.4. Requires non-empty reason to confirm.
- Mark Done: calls set_item_state(id, 'done'). Strip re-renders with:
  - strikethrough content
  - opacity 0.5
  - collapsed to single line (no metadata visible)
- Mark Blocked: opens BlockModal, calls set_item_state(id, 'blocked',
  reason). Strip shows ⏸ badge + reason preview + age ("blocked 2d").
- Undo done: Mark Done action on a done item toggles it back to
  active. Same for unblock from blocked.
- Delete: calls delete_item (soft). Projection excludes; event
  preserved.
- Done visibility (session-scoped, not persisted): done items remain
  in their bay until next app launch, then auto-hidden. Each bay
  renders a "Show N earlier done items" link when hidden done items
  exist; click reveals them for the current session only. This is
  Zustand UI state — no Settings field, no persistence.
- Undo-delete toast: on delete, show a toast "Deleted. Undo" for 10
  seconds. Clicking Undo calls a new restore_item command, which
  emits ITEM_RESTORED and clears items.deleted=1 in the projection.
  After 10s with no action, the toast dismisses and the item stays
  deleted.

Out of scope:
- Dates (I-09)
- Restore from soft-delete (§10.3 defaults to "no UI")
- Edit (leave as stub that logs "not yet" — or implement if trivial; 
  ITEM_EDITED is specified in SPEC §4.3)

Actually implement edit:
- Edit opens inline rename on the strip; commits on blur/Enter; emits
  ITEM_EDITED.

Demo:
- Mark A item blocked with reason "waiting on X" → ⏸ badge + reason
  shows; item still counts in A's render but NOT in A's active count
  (A's counter "3/5" decrements if item was active)
- Mark an A item done → strikes through, collapses; counter decrements
- Restart the app → done items auto-hidden; bay shows "Show N earlier
  done items" link; click reveals them for this session only
- Delete an item → toast "Deleted. Undo" appears for 10s; click Undo
  within that window → ITEM_RESTORED event appended, item reappears;
  OR let the toast dismiss → sqlite3 shows items.deleted=1,
  event_log has ITEM_DELETED (and no matching ITEM_RESTORED)
- Edit content → ITEM_EDITED event emitted; projection updated

Verify:
- Capacity math: A counter shows count_active(A), not total A items
- Setting an item to 'blocked' without reason returns REASON_REQUIRED
  error from backend

Stop after I-08.
```

### I-09 — Dates + staleness + calendar view

```
Execute SPEC.md §9 I-09 (Dates + staleness + calendar).

Scope:
- Set Start Date / Set Due Date overflow items open a native <input
  type="date"> (no date-picker library). Confirm calls set_item_date.
- Strip renders date badges inline: "▸ Apr 20" for start, "● Apr 30"
  for due. When past due and state==active, due shows in red.
- Staleness: per-tier thresholds. Compute days since max(events.ts
  WHERE item_id = X). Look up the threshold for the item's tier:
  settings.staleness_inbox_days (default 3), staleness_a_days
  (default 14), staleness_b_days (default 21), staleness_c_days
  (default null). If the threshold is non-null and days exceeds it,
  strip shows ⚠ badge. A null threshold disables flagging for that
  tier. All four settings are editable in I-11.
- CalendarView per SPEC §2.7:
  - Month navigation (prev/next month, "Today" button)
  - Day cells render pills for items with start or due on that day
  - Click day → day sheet (drawer or modal) listing all items with
    that start or due
  - Uses date-fns for date math; no other date library
  - Rendered items link to their strip: clicking a pill sets
    selectedItemId and switches back to Board view
- View switcher in TopBar now wires Calendar fully (Time-travel is
  still "not yet implemented" — I-10)

Out of scope:
- Recurring / repeating tasks (cut list)
- Calendar-based filtering or search
- Timezone handling (use local TZ, store as UnixMs)
- Date drag-to-reschedule

Demo:
- Set a due date on an A item → strip shows due badge; calendar view
  shows the item on that day
- Back-date an item's last event via direct SQL (INSERT INTO events
  ... ts = <N d ago>) to exercise each tier threshold, or temporarily
  drop the tier threshold to 1 day → ⚠ appears
- Past-due item shows red

Verify:
- Staleness computation uses event ts, not items.updated_at (they can
  diverge; event log is source of truth per CLAUDE.md)
- Per-tier thresholds exercised:
  - Inbox item with last event 4 days ago → ⚠ (exceeds 3d default)
  - A item with last event 4 days ago → no ⚠ (under 14d default)
  - A item with last event 15 days ago → ⚠
  - B item with last event 15 days ago → no ⚠ (under 21d default)
  - B item with last event 22 days ago → ⚠
  - C item with any age → no ⚠ (default null disables flagging);
    setting staleness_c_days to 30 makes a 31d C item flag
- Calendar handles month boundaries (e.g., due date on Apr 30 shows
  when viewing April, not when viewing May)
- Day sheet lists items in tier order (Inbox, A, B, C) then by rank

Stop after I-09.
```

### I-10 — Inspector panel + time-travel

```
Execute SPEC.md §9 I-10 (Inspector + time-travel).

Scope:
- InspectorPanel (side drawer, right side):
  - Opens when selectedItemId is set; closes via × or pressing Escape
  - Header: item content (truncated), tier · rank · state, timestamps
  - EVENTS section: chronological list, human-formatted, matches
    SPEC §2.6 layout
  - Re-fetches on Tauri item_updated event for this id
- get_events command per SPEC §5.1
- TimeTravelView:
  - TimestampPicker at top: start timestamp + "now" end. Scrubber maps
    linearly through the range in N minutes granularity (use N=5 min
    default).
  - get_items_at(ts) command: replays events in order up to ts,
    returns Item[]
  - BoardView reused with a readOnly prop: all interactions disabled
    (clicks do nothing except a "View is read-only" toast; drag fully
    disabled via @dnd-kit's disabled prop on useSortable)
  - Hotkey capture continues to work during time-travel (captures to
    live Inbox, not to past)
  - Exit button returns to live Board

Out of scope:
- Restore-from-time-travel (§10.3 default: no UI)
- Event log export (flagged for v1.1; skip)
- Projection rebuild command (defer; can be added to Settings in I-11
  if trivial, otherwise post-v1)

Additional scope: implement rebuild_projection as a primary
command (not dev-only). Settings wiring deferred to I-11; for
I-10, expose via devtools invocation only.

Demo:
- Click an item → InspectorPanel opens with full event list
- Move the item to another tier → panel updates live
- Enter time-travel, pick a timestamp before the move → item renders
  in original tier; all controls inert
- Exit time-travel → live board restored, latest state intact
- Run rebuild_projection from devtools → items table rebuilt; board
  renders identically

Verify:
- get_items_at(now) matches live items exactly (same ids, tiers, ranks,
  states, dates)
- get_items_at(epoch) returns empty
- Rebuild is idempotent: calling twice produces the same items table

Stop after I-10.
```

### I-11 — Settings page + keychain

```
Execute SPEC.md §9 I-11 (Settings + keychain).

Scope:
- SettingsView with sections (see SPEC §5.3 for fields):
  - General: hotkey (configurable; press-to-capture widget);
    per-tier staleness thresholds — staleness_inbox_days,
    staleness_a_days, staleness_b_days, staleness_c_days (each a
    number input with a null/disabled toggle; null disables flagging
    for that tier); close-to-tray (toggle, default true per SPEC
    §10.10); "Export event log" button (opens native save dialog via
    tauri-plugin-dialog, writes JSONL via the new export_events
    command, surfaces {events_written, path} on success)
  - Capture: lan_capture_enabled (toggle — triggers server
    start/stop stub, actual server is I-12), lan_capture_shared_secret
    (optional text, advanced disclosure)
  - LLM: base_url, model, api_key (write-only; shows "●●●● stored" if
    has_api_key is true, input to set/clear), timeout_ms, analyze
    window days
  - Advanced: "Rebuild projection from event log" button, wired to
    the rebuild_projection command (from I-10) behind a confirmation
    dialog; surfaces {items_affected} on success
- export_events command (new): opens the save dialog via
  tauri-plugin-dialog, iterates events in order, writes one JSON
  object per line to the chosen path; returns {events_written, path}
- Settings persisted to JSON file in app-data dir (non-secret fields)
- api_key → keychain via `keyring` crate; get_settings returns
  has_api_key boolean, never the key itself
- Hotkey change: unregister old, register new; show error if new one
  fails (already registered by another app, etc.)
- Close-to-tray: window.onCloseRequested → hide window instead of
  quit; tray icon with menu (Open, Quit)

Out of scope:
- LAN server implementation (I-12 — just wire the toggle to a stub
  that returns { enabled, url: null })
- Actual LLM calls (I-13)

Demo:
- Change hotkey → re-register → new hotkey works, old one doesn't
- Set an API key → stored in keychain (verify with `keyring` CLI or
  platform equivalent)
- Close window → app hides to tray, hotkey still works, reopening
  from tray restores board state
- Cmd/Ctrl+Q quits with a confirmation dialog
- Change A staleness threshold to 7 days → ⚠ appears on 7-day-old A
  items; set C threshold to non-null and verify C items start
  flagging; set an inbox item's threshold back to null and verify
  ⚠ disappears
- Click "Export event log" → choose a path → confirm the JSONL file
  is written with a line count equal to SELECT COUNT(*) FROM events;
  re-import round-trip can be verified by reading the first line and
  comparing to the first DB row
- Click "Rebuild projection from event log" → confirm dialog → items
  table is reconstructed; board renders identically; events table
  unchanged

Verify:
- settings.json on disk does NOT contain api_key field
- Keychain entry exists under service name "bay" and account name
  matching the user (or a fixed constant like "default")
- Hotkey unregister is clean (old hotkey doesn't leave a dead binding)
- export_events JSONL round-trip: line count matches COUNT(*) from
  events; first and last lines correspond to the first and last
  rows by id
- rebuild_projection is idempotent: running twice produces the same
  items table

Stop after I-11.
```

### I-12 — LAN capture server

```
Execute SPEC.md §9 I-12 (LAN capture server).

Scope:
- capture/server.rs: axum router with routes per SPEC §7.2
- capture/html.rs: compile-time embedded HTML per SPEC §7.3 via
  include_str!("capture.html")
- capture.html: single-file mobile-optimized page, no external deps,
  dark-mode aware. Behavior per SPEC §7.3.
- toggle_lan_capture command: starts/stops server. Returns {enabled,
  url} where url is http://<LAN-IP>:47821/?s=<secret> if secret set.
- Settings (Capture section): when enabled, show LAN URL, QR code
  PNG (via `qrcode` crate), and "Copy URL" button.
- POST /capture creates Inbox item via the same code path as
  create_item (reuse, do not duplicate). Emits lan_capture_received
  event to frontend → toast notification on desktop.
- Shared secret: if settings.lan_capture_shared_secret is non-null,
  POST /capture requires ?s= or X-Bay-Secret header match, else 401.
- Graceful shutdown on toggle-off: tokio::signal-driven; bind released
  before command returns.
- Port conflict: returns PORT_IN_USE error; user can change port in
  Settings (add lan_capture_port field, default 47821).

Out of scope:
- TLS (LAN-trust model explicit)
- Authentication beyond shared secret
- Multi-device session tracking

Demo:
- Enable LAN capture → Settings shows URL + QR
- Scan QR with phone on same wifi → capture page loads
- Submit text from phone → item appears in Inbox on desktop within
  ~1s with toast
- Disable capture → phone page stops working; port released
- Set a shared secret → phone page URL includes ?s= param; POST
  without secret returns 401

Verify:
- Server binds to 0.0.0.0:47821 (or configured port) only when
  enabled; check with `lsof -i :47821` or equivalent
- Stopping releases port cleanly (can toggle on-off-on without
  "address in use")
- capture.html renders correctly on iOS Safari and Android Chrome
  (Bo: test manually)
- POST with malformed body returns 400, not 500

Stop after I-12.
```

### I-13 — LLM client + test connection

```
Execute SPEC.md §9 I-13 (LLM client + test connection).

Scope:
- llm/mod.rs: LlmClient trait per SPEC §8.1
- llm/openai_compat.rs: reqwest-based implementation targeting
  /v1/chat/completions. Supports:
  - base_url (e.g., http://localhost:11434/v1 for Ollama, or
    https://api.openai.com/v1)
  - model
  - optional Authorization: Bearer <api_key>
  - timeout_ms
- test_llm_connection command per SPEC §5.1:
  - Sends a 2-token completion request (e.g., user message "hi",
    max_tokens: 5)
  - Returns { ok, latency_ms, model_echoed }
  - Maps errors to LlmError variants per SPEC §8.5
- Settings LLM section: "Test connection" button → loading spinner →
  result or error inline
- set_llm_config command writes non-secret fields to settings.json,
  api_key to keychain (may already exist from I-11; ensure this
  command supersedes any earlier ad-hoc path)

Out of scope:
- analyze command (I-14)
- Streaming (v2)
- Prompt templates (I-14)

Demo:
- Install Ollama locally with llama3.2 or similar
- Point base_url at http://localhost:11434/v1, model llama3.2
- Click Test → ✓ with latency and model name
- Stop Ollama → Test → "Can't reach the LLM endpoint" error
- Point at OpenAI with invalid key → "LLM rejected the API key"
- Point at OpenAI with valid key (Bo will test this manually) → ✓

Verify:
- Timeout actually triggers at timeout_ms (not just reqwest default)
- Network errors surface with useful messages, not generic "error"
- No api_key leakage in logs (use debug_assert! or sanitize)

Stop after I-13.
```

### I-14 — Analyze prompt, compression, UI, accept/reject

```
Execute SPEC.md §9 I-14 (Analyze end-to-end).

Read SPEC §8 in full before writing code. The LLM firewall rule in
CLAUDE.md §Design philosophy applies: advisory only, no mutations.

Scope:
- llm/compression.rs: computes aggregates per SPEC §8.2 template via
  SQL (no raw events to LLM in v1)
- llm/prompt.rs: const strings for system and user templates per
  SPEC §8.2
- llm/parse.rs: strict JSON parser for observation array; one retry
  with an explicit "return only JSON" prompt prefix; on second failure
  returns LLM_PARSE_ERROR
- analyze command:
  - window_days defaults from settings.analyze_window_days (30)
  - compress + prompt + call + parse
  - emits LLM_SUGGESTION_GENERATED with full observations payload
    before returning
  - emits analyze_progress events at 'compressing', 'calling_llm',
    'parsing' stages
- AnalyzePanel per SPEC §2.10:
  - Opened via TopBar Analyze button
  - Shows loading state with progress stages
  - Renders observations with severity icons (info = ℹ, warn = ⚠)
  - affected_item_ids: clicking an id scrolls the board to that strip
    and briefly highlights it
  - Mark Reviewed → accept_suggestion command → LLM_SUGGESTION_ACCEPTED
    event (resulting_event_ids: [])
  - Dismiss → reject_suggestion command → LLM_SUGGESTION_REJECTED event

Out of scope — DO NOT ADD:
- Any code path where the LLM output causes mutations to items
- LLM-suggested moves, tier changes, or reorganizations
- Streaming responses
- Sending raw event.content to the LLM (only aggregates + current
  board snapshot per SPEC §8.3)
- Auto-analyze on timer or event trigger (Analyze is strictly
  user-initiated)

Demo (requires I-13 Ollama setup):
- Board has ~20 items, some with recent activity, some stale
- Click Analyze → loading state with progress stages → observations
  appear within ~5s on Ollama
- Click an affected_item_id → board scrolls and highlights
- Mark Reviewed → panel closes, event logged
- Click Analyze again → new LLM_SUGGESTION_GENERATED event

Verify:
- sqlite3 "SELECT payload FROM events WHERE type='LLM_SUGGESTION_GENERATED' ORDER BY id DESC LIMIT 1;"
  shows aggregates and observations as JSON
- No mutation events (ITEM_MOVED, ITEM_STATE_CHANGED, etc.) emitted as
  a side effect of analyze. Analyze is strictly read-only on items.
- Bad JSON from LLM → retry fires → on second failure, UI shows a
  readable error and the suggestion is logged with empty observations
  and an error field

Stop after I-14. v1 is complete. Tag the commit v0.1.0.
```

---

## 3. Operational prompts

### 3.1 Reviewing deviations from SPEC

When you suspect the codebase has drifted from SPEC.md.

```
Do not modify any code. Read the current state of the codebase against
SPEC.md. Produce a table of deviations with columns:

  | Location (file:line) | SPEC section | Actual behavior | Spec behavior | Severity |

Severity levels:
  - BLOCKING: violates a load-bearing CLAUDE.md principle
  - STRUCTURAL: departs from SPEC but does not violate CLAUDE.md
  - COSMETIC: differs but is functionally equivalent

Do not recommend fixes in this pass. List only. After I review, I'll
direct which to address.

If you find deviations that CLAUDE.md or SPEC.md failed to anticipate
(genuine ambiguity, not specification gaps), list them in a separate
section titled "AMBIGUITIES". These become candidates for SPEC revision,
not code change.
```

### 3.2 Debug — something is broken

```
Symptom: <describe exactly what's happening, including steps to
reproduce, expected vs actual behavior>

Before touching any code, isolate the layer:

  1. Is the event being written correctly?
     → sqlite3 "SELECT * FROM events ORDER BY id DESC LIMIT 5"
     → Report: event exists? payload correct?

  2. Is the projection applying correctly?
     → Compare events payload to items row for that item_id
     → Report: mismatch? If yes, where?

  3. Is the frontend receiving the update?
     → Check Tauri event listeners; does store receive item_updated?
     → Report: event observed in devtools?

  4. Is the component re-rendering?
     → Check Zustand selector; is it returning a new reference?
     → Report: render observed?

Identify the layer where the chain breaks. Name the mechanical cause
before proposing a fix.

Do not construct a narrative explaining why the current behavior is
"actually correct" or "probably a race condition." Name the exact line
where the expected behavior diverges. If you cannot, say "I cannot
localize this with the information available" and propose the next
diagnostic step.
```

### 3.3 Projection audit

Run this whenever the UI and the database appear to disagree.

```
Do not modify code. Run the projection audit:

  1. Snapshot the items table: SELECT * FROM items WHERE deleted = 0
  2. Run rebuild_projection command (from I-10)
  3. Snapshot items again
  4. Diff the two snapshots row by row

Report any differences:
  - Missing rows (projection dropped an item the events support)
  - Extra rows (projection has an item the events do not create)
  - Field mismatches (same id, different tier/rank/state/content/dates)

If the diff is empty, projection is consistent with events. The bug is
elsewhere (likely frontend state).

If the diff is non-empty, the bug is in the projection logic in
db/items.rs::apply_event_to_projection. Identify which event type's
handler produced the drift.

Do not fix in this pass. Report only.
```

### 3.4 Scope correction

When Claude Code is implementing beyond the current increment.

```
Stop. You are implementing <specific feature> which is not in the
scope of <current increment>. It belongs to <target increment> per
SPEC.md §9.

Revert <specific feature>. Complete only what is in <current
increment>'s scope and stop. Commit, and we will address <target
increment> in a dedicated session.

If you believe <specific feature> is required for <current increment>
to work, explain the dependency before reverting. If the dependency is
real, SPEC.md needs revision, not ad-hoc implementation.
```

### 3.5 Revising a recent increment

When an increment shipped but needs correction.

```
Increment I-<NN> was committed but deviates from SPEC in this way:
<describe>

Do not proceed to I-<NN+1>. Revise I-<NN> in a new commit. Scope of
this revision:

  - <specific change 1>
  - <specific change 2>

Out of scope for this revision:
  - Anything beyond the listed changes
  - Refactoring adjacent code "while we're here"
  - Adding tests not directly validating the fix

Demo the fix with the same verify list from I-<NN>'s original prompt.
Commit with message prefix "fix(I-<NN>): ..."
```

### 3.6 Returning after time away

When you open Claude Code on this project after days or weeks.

```
I'm returning to Bay after <N days/weeks> away. Before any work:

  1. Read CLAUDE.md in full.
  2. Read SPEC.md in full.
  3. Run `git log --oneline -20` and summarize the last 20 commits in
     1 sentence each.
  4. Identify the most recent completed increment from commit
     messages. Confirm its demo still works — launch the app, execute
     the I-NN demo, report pass/fail.
  5. Tell me what the next increment is and what its demo should be.

Do not write code until I confirm orientation and direct the next
increment.
```

---

## 4. Commit message template

```
<type>(I-<NN>): <short summary>

Scope: <one-line description of what changed>

Events introduced: <list of event types first emitted in this commit,
                   or "none" for non-mutation changes>
Schema changes:   <"none" or migration file name>
Demo:             <the demo from this increment's spec prompt>

Verified:
  - cargo build + cargo test pass
  - pnpm build + pnpm test pass
  - Demo executes as described above
  - <any increment-specific verify items>

Refs: SPEC.md §9 I-<NN>, CLAUDE.md §<relevant section>
```

`<type>`:
- `feat` — new increment work
- `fix` — revision to a prior increment
- `chore` — dependencies, build config, non-behavioral
- `docs` — CLAUDE.md, SPEC.md, PROMPTS.md, README.md edits
- `refactor` — internal restructuring with no behavior change (rare in v1; be skeptical)

Prefix the body with `BREAKING:` if the commit invalidates existing event log payloads or DB schema in a non-migratable way. None should be v1; if one appears, stop and reconsider.

---

## 5. End-of-session checklist

Before closing Claude Code and committing, confirm:

- [ ] The increment's demo executes exactly as specified.
- [ ] `cargo build` passes with no new warnings. Treat warnings as errors in v1.
- [ ] `cargo test` passes.
- [ ] `pnpm build` passes.
- [ ] `pnpm tsc --noEmit` passes (no type errors).
- [ ] No `TODO`, `FIXME`, or `XXX` comments in this commit's diff, unless tagged with an increment number for future work.
- [ ] No `console.log`, `dbg!`, or `println!` for debugging left behind.
- [ ] No dependencies added beyond those authorized in SPEC §6.1 or §6.2. If a new dependency is needed, that's a SPEC amendment, not a drive-by add.
- [ ] Commit message follows §4 template.
- [ ] Event log integrity spot-checked via `sqlite3` for events written by this increment.

---

*End of PROMPTS.md. Current version: v1.4. Prior versions in archive/. Per-prompt wording is tuned to resist specific failure modes; change with care.*
