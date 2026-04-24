# SPEC.md — Bay

> v1.2 — 2026-04-24. Minor reconciliation of §5.3, §7.1, and
> closing line. v1.0 and v1.1 at archive/SPEC_v1.0.md and
> archive/SPEC_v1.1.md.

> Implementation specification. Scope-locked by `CLAUDE.md`. All design decisions in `CLAUDE.md` are authoritative; this document translates them into buildable detail without introducing new scope.

## 0. Reading order

1. `CLAUDE.md` — doctrine. Read first, every session.
2. This document — implementation detail.
3. `PROMPTS.md` (to be written) — per-increment Claude Code prompts.

Sections in this document are independent but cross-reference by number. Section 9 (build plan) is the execution order.

---

## 1. Component tree

### 1.1 State management — choice and rationale

**Zustand.** Single store, TypeScript-native, minimal boilerplate, no providers required, works cleanly with Tauri `invoke` (no middleware needed for async). Rejected alternatives:

- **Redux/RTK**: excessive ceremony for single-user local state; action/reducer pattern duplicates the event-sourcing the Rust backend already owns.
- **Jotai**: atom granularity is valuable for complex reactive graphs; this app has a flat state tree and doesn't benefit.
- **React Context**: re-render fanout on any state change is a drag-and-drop killer; this app will have 50+ items rendered simultaneously.
- **TanStack Query**: optimistic updates for CRUD over Tauri commands is overkill; the command round-trip is <5ms locally and events-out from backend give us real-time invalidation for free.

Store shape:

```ts
interface BayStore {
  // Data (server state)
  items: Record<string, Item>;                        // id → Item
  itemsByTier: Record<Tier, string[]>;                // pre-sorted ids per tier (derived, cached)
  settings: Settings;

  // UI state
  activeView: 'board' | 'calendar' | 'timetravel' | 'settings';
  timeTravelTs: number | null;                        // unix ms when in timetravel mode
  selectedItemId: string | null;                      // for inspector
  modals: {
    quickCapture: boolean;
    swap: { open: boolean; tier: 'A' | 'B'; pendingContent?: string; pendingDropId?: string } | null;
    block: { open: boolean; itemId: string } | null;
    moveReason: { open: boolean; itemId: string; fromTier: Tier; toTier: Tier } | null;
    analyze: boolean;
    settings: boolean;
  };
  analyzeState: {
    running: boolean;
    suggestionId: string | null;
    observations: Observation[];
    error: string | null;
  };

  // Actions (all thin wrappers over invoke)
  loadInitial: () => Promise<void>;
  createItem: (tier: Tier, content: string) => Promise<void>;
  moveItem: (id: string, toTier: Tier, toRank: string, reason?: string) => Promise<void>;
  swapAndCreate: (leavingId: string, leavingDest: 'B' | 'C', newContent: string, newTier: 'A' | 'B') => Promise<void>;
  setState: (id: string, state: ItemState, blockedReason?: string) => Promise<void>;
  editItem: (id: string, content: string) => Promise<void>;
  setDate: (id: string, field: 'start' | 'due', value: number | null) => Promise<void>;
  deleteItem: (id: string) => Promise<void>;

  enterTimeTravel: (ts: number) => Promise<void>;
  exitTimeTravel: () => void;

  analyze: () => Promise<void>;
  acceptAnalyze: () => Promise<void>;
  rejectAnalyze: (reason?: string) => Promise<void>;

  // Internal — called by Tauri event listeners
  _onItemCreated: (item: Item) => void;
  _onItemUpdated: (item: Item) => void;
  _onItemDeleted: (id: string) => void;
  _onLanCapture: (item: Item) => void;
}
```

`itemsByTier` is recomputed on any mutation affecting rank or tier. Rank sorting uses lexicographic string comparison (see §4.2).

### 1.2 Component tree

```
<App>
 ├─ <TauriEventBridge/>                    // no render; subscribes to backend events
 ├─ <TopBar>
 │    ├─ <ViewSwitcher/>                   // Board | Calendar | Time-travel
 │    ├─ <TimeTravelIndicator/>            // only when in timetravel
 │    ├─ <AnalyzeButton/>                  // opens AnalyzePanel
 │    └─ <SettingsButton/>                 // opens Settings
 │
 ├─ <MainContent>
 │    ├─ <BoardView> (default)
 │    │    ├─ <Bay tier="inbox">
 │    │    │    ├─ <BayHeader/>            // title, counter, +Add button
 │    │    │    ├─ <DndContext>
 │    │    │    │    └─ <Strip/>*
 │    │    │    └─ <BayEmptyState/>        // shown when bay empty
 │    │    ├─ <Bay tier="A" cap={5}/>
 │    │    ├─ <Bay tier="B" cap={12}/>
 │    │    └─ <Bay tier="C"/>
 │    │
 │    ├─ <CalendarView>
 │    │    ├─ <MonthNavigator/>
 │    │    ├─ <MonthGrid>
 │    │    │    └─ <DayCell>
 │    │    │         └─ <CalendarItemPill/>*
 │    │    └─ <CalendarLegend/>            // start-date vs due-date color key
 │    │
 │    └─ <TimeTravelView>
 │         ├─ <TimestampPicker/>
 │         ├─ <BoardView readOnly/>        // reuses BoardView
 │         └─ <ExitTimeTravelButton/>
 │
 ├─ <InspectorPanel/>                      // side drawer; opens when selectedItemId
 │
 ├─ <Modals>
 │    ├─ <QuickCaptureModal/>              // global hotkey trigger
 │    ├─ <SwapModal/>                      // A/B cap overflow
 │    ├─ <BlockModal/>                     // require reason
 │    ├─ <MoveReasonModal/>                // optional reason on cross-tier drag
 │    ├─ <AnalyzePanel/>                   // LLM observations + accept/reject
 │    └─ <SettingsModal/>                  // or <SettingsView/> inline
 │
 └─ <ToastHost/>                           // transient notifications
```

### 1.3 Component ownership rules

- **Data ownership**: all item mutations flow through Zustand store actions, which call Tauri commands. Never mutate store directly from a component; components dispatch actions only.
- **Drag state**: `@dnd-kit` owns transient drag state in its own context; store receives the committed result via `onDragEnd`.
- **Modals**: open/close state lives in store under `modals.*`. Rendering root is `<Modals>`. Components open modals by dispatching store actions (`openSwap(...)`), not by passing props.
- **Derived data**: `itemsByTier` is computed via Zustand selector with referential equality via `shallow`. Strip components subscribe to their own item by id — a move of item X does not re-render item Y.

---

## 2. Wireframes

ASCII wireframes for layout intent. Final styling per system-theme neutrality (CLAUDE.md cut list).

### 2.1 Board view

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ Bay                                                                           │
│ [Board] [Calendar] [Time-travel]                     [Analyze] [⚙ Settings] │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│ ▼ INBOX                                                      2 items   [ + ] │
│ ┌─────────────────────────────────────────────────────────────────────────┐ │
│ │ ≡  Idea from phone capture 2h ago                                  · · · │ │
│ │ ≡  Note from hotkey last night                                     · · · │ │
│ └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                               │
│ ▼ A                                                           3 / 5    [ + ] │
│ ┌─────────────────────────────────────────────────────────────────────────┐ │
│ │ ≡  Q3 bond covenant review                              [block] [done]   │ │
│ │ ≡  Prep for Friday 1-on-1 with Marcolin                 [block] [done]   │ │
│ │ ⏸  Waiting on vendor quote — SAN expansion           (blocked: 4 days)   │ │
│ └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                               │
│ ▼ B                                                           9 / 12   [ + ] │
│ ┌─────────────────────────────────────────────────────────────────────────┐ │
│ │ ≡  Draft new backup runbook                             [block] [done]   │ │
│ │ ≡  Review AORTA confidence calibration notes            [block] [done]   │ │
│ │ ⚠  14 days stale — Update network diagram               [block] [done]   │ │
│ │ ... (6 more)                                                              │ │
│ └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                               │
│ ▼ C                                                         21 items   [ + ] │
│ ┌─────────────────────────────────────────────────────────────────────────┐ │
│ │ ≡  Learn Rust async internals properly                                   │ │
│ │ ... (virtualized; showing 20 of 21)                                      │ │
│ └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                               │
└──────────────────────────────────────────────────────────────────────────────┘
```

Strip annotations:
- `≡` drag handle
- `●` active (default, no badge shown in final design — just absence of other badges)
- `⏸` blocked, with reason preview and age
- `⚠` stale (exceeds staleness threshold)
- `· · ·` overflow menu (edit, set dates, delete, inspect)
- Done items: strikethrough, reduced opacity, collapsed to single line, remain in place until archive or session end.

### 2.2 Swap modal

```
┌──────────────────────────────────────────────────────────────────┐
│ A is full (5 / 5)                                             ✕  │
├──────────────────────────────────────────────────────────────────┤
│ To add "Review bond payment schedule" to A, one A item must      │
│ leave A. Pick one to move.                                       │
│                                                                   │
│ ○ Q3 bond covenant review        → ● B    ○ C                    │
│ ○ Prep 1-on-1 with Marcolin      → ● B    ○ C                    │
│ ○ Vendor quote (blocked)         → ● B    ○ C                    │
│                                                                   │
│ Reason (optional): [________________________________________]    │
│                                                                   │
│                              [ Cancel ]   [ Swap ]               │
└──────────────────────────────────────────────────────────────────┘
```

### 2.3 Quick capture modal (global hotkey)

```
┌──────────────────────────────────────────────────────┐
│ Quick capture → Inbox                                │
├──────────────────────────────────────────────────────┤
│ ┌──────────────────────────────────────────────────┐ │
│ │                                                  │ │
│ │                                                  │ │
│ └──────────────────────────────────────────────────┘ │
│                                                       │
│ Esc cancel · Enter commit · Ctrl+Enter commit & open │
└──────────────────────────────────────────────────────┘
```

### 2.4 Block modal

```
┌──────────────────────────────────────────────────┐
│ Mark blocked                                  ✕  │
├──────────────────────────────────────────────────┤
│ What's blocking this item?                        │
│ ┌──────────────────────────────────────────────┐ │
│ │                                              │ │
│ └──────────────────────────────────────────────┘ │
│                                                   │
│                         [ Cancel ]   [ Block ]   │
└──────────────────────────────────────────────────┘
```

### 2.5 Move reason modal (cross-tier drag)

```
┌──────────────────────────────────────────────────┐
│ Moving A → C                                  ✕  │
├──────────────────────────────────────────────────┤
│ Item: "Q3 bond covenant review"                   │
│                                                   │
│ Reason (optional): [_______________________]      │
│                                                   │
│                     [ Cancel ]   [ Confirm ]     │
└──────────────────────────────────────────────────┘
```

### 2.6 Inspector panel (side drawer)

```
┌─────────────────────────────────────────────────────┐
│ Item history                                    ✕  │
├─────────────────────────────────────────────────────┤
│ "Q3 bond covenant review"                           │
│                                                      │
│ Tier: A · Rank: m3k · State: active                 │
│ Created: 2026-04-18 14:22                           │
│ Start: 2026-04-20 · Due: 2026-04-30                 │
│                                                      │
│ ── EVENTS ──                                        │
│                                                      │
│ 2026-04-18 14:22  CREATED in inbox                  │
│ 2026-04-18 14:25  MOVED inbox → A                   │
│                   reason: "blocking monthly close"  │
│ 2026-04-19 09:10  DATE_SET due=2026-04-30           │
│ 2026-04-21 11:04  STATE blocked                     │
│                   reason: "waiting on trustee reply"│
│ 2026-04-22 15:30  STATE active                      │
│                                                      │
└─────────────────────────────────────────────────────┘
```

### 2.7 Calendar view

```
┌──────────────────────────────────────────────────────────────────┐
│ [Board] [Calendar] [Time-travel]                                  │
│                                                                   │
│                  ◂  April 2026  ▸                                 │
│ ┌──────┬──────┬──────┬──────┬──────┬──────┬──────┐              │
│ │ Mon  │ Tue  │ Wed  │ Thu  │ Fri  │ Sat  │ Sun  │              │
│ ├──────┼──────┼──────┼──────┼──────┼──────┼──────┤              │
│ │      │      │  1   │  2   │  3   │  4   │  5   │              │
│ │      │      │ ▸ Q3 │      │      │      │      │              │
│ ├──────┼──────┼──────┼──────┼──────┼──────┼──────┤              │
│ │  6   │  7   │  8   │  9   │ 10   │ 11   │ 12   │              │
│ ...                                                               │
│ Legend:  ▸ start-date    ● due-date                              │
└──────────────────────────────────────────────────────────────────┘
```

Day-cell click opens a day sheet listing all items with that start or due date. Only items with explicit `start_at` or `due_at` render.

### 2.8 Time-travel view

```
┌──────────────────────────────────────────────────────────────────┐
│ Time-travel   ⟵ 2026-04-22 09:00  [________|___]  now ⟶   [Exit]│
│ READ ONLY                                                         │
├──────────────────────────────────────────────────────────────────┤
│ ... (BoardView reused, disabled interactions)                    │
└──────────────────────────────────────────────────────────────────┘
```

Scrubber covers the earliest-event-timestamp → now range. Discrete snap points at every event boundary optional (v2).

### 2.9 LAN capture page (served from Rust backend)

Single HTML page. Mobile-optimized. Rendered at `GET /`.

```
┌──────────────────────────────┐
│ Bay — Capture to Inbox        │
├──────────────────────────────┤
│ ┌──────────────────────────┐ │
│ │                          │ │
│ │                          │ │
│ │                          │ │
│ └──────────────────────────┘ │
│                              │
│        [ Capture ]           │
│                              │
│ Recent (this device):        │
│ 14:22  Idea from walk        │
│ 12:05  Reminder for dentist  │
└──────────────────────────────┘
```

`Recent` list is session-local (client-side, not stored server-side). Persists until page refresh.

### 2.10 Analyze panel (side drawer)

```
┌─────────────────────────────────────────────────────┐
│ Analyze                                           ✕ │
├─────────────────────────────────────────────────────┤
│ Analyzing last 30 days of events...                 │
│                                                      │
│ Model: ollama/llama3.2 · Latency: 2.4s              │
│                                                      │
│ ── OBSERVATIONS ──                                  │
│                                                      │
│ ⚠  Your A tier is inflated.                         │
│    Created 23 A items this month; completed 6.      │
│                                                      │
│ ⚠  "Prep for 1-on-1 with Marcolin" has been in      │
│    A for 19 days untouched.                         │
│                                                      │
│ ℹ  40% of items moved A→C within 48h of creation.   │
│    A may be functioning as a de-facto inbox.        │
│                                                      │
│ [ Mark reviewed ]   [ Dismiss ]                     │
└─────────────────────────────────────────────────────┘
```

`Mark reviewed` emits `LLM_SUGGESTION_ACCEPTED` (with empty `resulting_event_ids` in v1 — advisory-only; see §10). `Dismiss` emits `LLM_SUGGESTION_REJECTED`.

---

## 3. State machines

### 3.1 Item lifecycle (states)

```mermaid
stateDiagram-v2
    [*] --> active: ITEM_CREATED

    active --> blocked: ITEM_STATE_CHANGED\n(reason required)
    blocked --> active: ITEM_STATE_CHANGED

    active --> done: ITEM_STATE_CHANGED
    blocked --> done: ITEM_STATE_CHANGED
    done --> active: ITEM_STATE_CHANGED (undo)

    active --> deleted: ITEM_DELETED (soft)
    blocked --> deleted: ITEM_DELETED (soft)
    done --> deleted: ITEM_DELETED (soft)

    deleted --> [*]
```

Guards:

- `active → blocked`: requires non-empty `blocked_reason`.
- `* → blocked`: prohibited from `done`. (Must un-done first.)
- `done` items still count as occupying their tier but **do not count toward capacity** (caps apply to `active` only).

### 3.2 Tier transitions (independent of state)

Tier is an orthogonal dimension. Any state may exist in any tier. Tier moves emit `ITEM_MOVED`.

```mermaid
stateDiagram-v2
    inbox --> A
    inbox --> B
    inbox --> C
    A --> B
    A --> C
    A --> inbox
    B --> A
    B --> C
    B --> inbox
    C --> A
    C --> B
    C --> inbox
```

Guards on inbound to A / B when state is `active`:

```
if target_tier == A and count_active(A) >= 5: REQUIRE_SWAP
if target_tier == B and count_active(B) >= 12: REQUIRE_SWAP
```

`blocked` and `done` items moving into A/B do not trigger swap (they don't count).

### 3.3 Capacity swap flow

```mermaid
sequenceDiagram
    actor User
    participant UI
    participant Store
    participant Backend

    User->>UI: Drag item X → A (or Add new to A)
    UI->>Store: requestMoveToA(X)
    Store->>Store: check count_active(A)
    alt count < 5
        Store->>Backend: move_item(X, A, rank)
        Backend-->>Store: Item
    else count == 5
        Store->>UI: open SwapModal(pending: X, targetTier: A)
        User->>UI: select leavingItem Y, destination (B or C), optional reason
        UI->>Store: confirmSwap(Y, dest, X, A)
        Store->>Backend: swap_move(leaving=Y, leavingDest=dest, entering=X, enteringTier=A, reason)
        Backend->>Backend: begin tx
        Backend->>Backend: append ITEM_MOVED(Y, A→dest)
        Backend->>Backend: append ITEM_MOVED(X, fromTier→A)
        Backend->>Backend: commit tx
        Backend-->>Store: [Item_Y_updated, Item_X_updated]
    end
```

Invariant: swap is atomic. Either both moves commit or neither does. Backend enforces this via SQLite transaction; frontend treats as single operation.

### 3.4 Cross-tier move with reason

```
user drags item from tier_from → tier_to (tier_from != tier_to)
    ↓
if tier_to in {A, B} and count_active(tier_to) >= cap(tier_to) and item.state == active:
    → swap flow (§3.3)
else:
    → open MoveReasonModal (optional reason)
    → on confirm: move_item(id, tier_to, rank, reason)
    → on cancel: drag is reverted (UI returns item to tier_from via store)
```

Intra-tier drag skips the modal entirely. One `ITEM_MOVED` event, no reason.

---

## 4. Event payload schemas

All payloads are JSON, stored as TEXT in SQLite, validated on write via Rust `serde` and on the frontend via zod. Unknown fields are preserved (forward-compat) but never generated by v1 code.

### 4.1 Common types

```ts
type Tier  = 'inbox' | 'A' | 'B' | 'C';
type State = 'active' | 'blocked' | 'done';
type Rank  = string;                      // lexicographic, see §4.2
type UnixMs = number;
```

### 4.2 Rank — fractional indexing

Lexicographic string ranks over the alphabet `[a–z0–9]` (base 36, case-insensitive, sorted by standard string comparison). Library: port of `fractional-indexing` (Observable / npm by Dan Brown) to Rust.

Operations:
- `rank_between(a: Option<&str>, b: Option<&str>) -> String`
- Returns a string strictly between `a` and `b` in string order. `None` means "boundary" (start or end).
- Example: between `"a"` and `"c"` returns `"b"`. Between `"a"` and `"b"` returns `"an"`. Monotonic growth; no collisions in realistic usage.

Rebalance trigger: if any inserted rank exceeds 64 characters, background task renumbers the tier's items to short canonical ranks (`"a"`, `"b"`, ...) via a single batch of `ITEM_MOVED` events with reason `"rank-rebalance"`. Expected frequency: effectively never in single-user usage. Implement the helper; defer invoking rebalance to v1.5 unless testing shows genuine need.

### 4.3 Event payload specifications

Every event row: `{ id, ts, type, item_id, payload }`. `item_id` is NULL for non-item events (none in v1). `payload` schemas below.

**ITEM_CREATED**
```json
{
  "content":  "string (1..4096 chars)",
  "tier":     "Tier",
  "rank":     "Rank",
  "start_at": "UnixMs | null",
  "due_at":   "UnixMs | null"
}
```

**ITEM_EDITED**
```json
{
  "content_before": "string",
  "content_after":  "string"
}
```

**ITEM_MOVED**
```json
{
  "tier_before": "Tier",
  "rank_before": "Rank",
  "tier_after":  "Tier",
  "rank_after":  "Rank",
  "reason":      "string | null"
}
```
Invariant: `tier_before != tier_after` OR `rank_before != rank_after`. (No-op moves rejected at command layer.)

**ITEM_STATE_CHANGED**
```json
{
  "state_before":   "State",
  "state_after":    "State",
  "blocked_reason": "string | null"
}
```
`blocked_reason` required when `state_after == "blocked"`, otherwise null.

**ITEM_DATE_SET**
```json
{
  "field":        "\"start\" | \"due\"",
  "value_before": "UnixMs | null",
  "value_after":  "UnixMs | null"
}
```

**ITEM_DELETED**
```json
{
  "soft": true
}
```
Sets `items.deleted = 1`. Projection excludes. Event log preserved for audit.

**ITEM_RESTORED**
```json
{}
```
Clears `items.deleted` (sets to 0). Emitted only via the undo-delete toast path (see §9 I-08). Backend rejects if the item is not currently soft-deleted (`NOT_DELETED` error).

**LLM_SUGGESTION_GENERATED**
```json
{
  "kind":   "\"analyze\"",
  "scope":  { "since_ts": "UnixMs", "until_ts": "UnixMs", "event_count": "number" },
  "model":  "string",
  "observations": [
    {
      "severity":          "\"info\" | \"warn\"",
      "text":              "string",
      "affected_item_ids": ["string"]
    }
  ]
}
```

**LLM_SUGGESTION_ACCEPTED**
```json
{
  "suggestion_event_id": "number",
  "resulting_event_ids": ["number"]
}
```
In v1 `resulting_event_ids` is always empty (advisory-only; see §10).

**LLM_SUGGESTION_REJECTED**
```json
{
  "suggestion_event_id": "number",
  "reason":              "string | null"
}
```

---

## 5. IPC contract

All commands return `Result<T, BayError>`. `BayError` serializes to `{ code: string, message: string, detail?: any }`. Frontend discriminates on `code`.

### 5.1 Commands (frontend → backend)

| Command | Params | Returns | Errors |
|---|---|---|---|
| `bootstrap` | — | `{ items: Item[], settings: Settings, lanCapture: { enabled: bool, url: string \| null } }` | `DB_MIGRATE_FAILED` |
| `create_item` | `{ content, tier, start_at?, due_at? }` | `Item` | `CAP_EXCEEDED`, `CONTENT_EMPTY`, `CONTENT_TOO_LONG` |
| `edit_item` | `{ id, content }` | `Item` | `ITEM_NOT_FOUND`, `CONTENT_EMPTY`, `CONTENT_TOO_LONG` |
| `move_item` | `{ id, to_tier, to_rank?, reason? }` | `Item` | `ITEM_NOT_FOUND`, `CAP_EXCEEDED`, `INVALID_RANK` |
| `swap_move` | `{ leaving_id, leaving_dest, entering_id?, entering_content?, entering_tier, entering_rank?, reason? }` | `{ leaving: Item, entering: Item }` | `ITEM_NOT_FOUND`, `CAP_EXCEEDED`, `BAD_ARGS` |
| `set_item_state` | `{ id, state, blocked_reason? }` | `Item` | `ITEM_NOT_FOUND`, `INVALID_TRANSITION`, `REASON_REQUIRED` |
| `set_item_date` | `{ id, field, value }` | `Item` | `ITEM_NOT_FOUND` |
| `delete_item` | `{ id }` | `void` | `ITEM_NOT_FOUND` |
| `restore_item` | `{ id }` | `Item` | `ITEM_NOT_FOUND`, `NOT_DELETED` |
| `get_events` | `{ item_id?, since_ts?, until_ts?, limit? }` | `Event[]` | — |
| `get_items_at` | `{ ts }` | `Item[]` | `TS_BEFORE_EPOCH` |
| `rebuild_projection` | — | `{ items_affected: number }` | `DB_ERROR` |
| `export_events` | `{ path }` | `{ events_written: number, path: string }` | `IO_ERROR` |
| `get_settings` | — | `Settings` | — |
| `update_settings` | `Partial<Settings>` | `Settings` | `INVALID_SETTING` |
| `toggle_lan_capture` | `{ enabled }` | `{ enabled, url \| null }` | `PORT_IN_USE` |
| `set_llm_config` | `{ base_url, model, api_key?, timeout_ms }` | `void` | `KEYCHAIN_ERROR` |
| `test_llm_connection` | — | `{ ok, latency_ms, model_echoed }` | `LLM_UNREACHABLE`, `LLM_AUTH_FAILED`, `LLM_TIMEOUT` |
| `analyze` | `{ window_days? }` | `{ suggestion_event_id, observations: Observation[] }` | `LLM_UNREACHABLE`, `LLM_PARSE_ERROR`, `LLM_TIMEOUT` |
| `accept_suggestion` | `{ suggestion_event_id }` | `void` | `EVENT_NOT_FOUND` |
| `reject_suggestion` | `{ suggestion_event_id, reason? }` | `void` | `EVENT_NOT_FOUND` |

**Capacity enforcement**: `create_item` and `move_item` both check caps server-side. Frontend also checks for responsive UI, but backend is the authority. `swap_move` skips the cap check on `entering_tier` since it's paired with an outgoing move in the same transaction.

**Rank resolution**: when `to_rank` is omitted, backend places item at end of `to_tier`. Client may precompute rank via `rank_between` and pass explicitly for drag-drop precision.

### 5.2 Events (backend → frontend)

Emitted via Tauri's event system. Frontend listens in `TauriEventBridge`.

| Event | Payload | Meaning |
|---|---|---|
| `item_created` | `Item` | New item (including via LAN capture) |
| `item_updated` | `Item` | Any projection change to an existing item |
| `item_deleted` | `{ id: string }` | Soft delete applied |
| `lan_capture_received` | `Item` | LAN capture POST produced an item. (Subset of item_created; separate for toast notification.) |
| `analyze_progress` | `{ suggestion_event_id, stage: 'compressing' \| 'calling_llm' \| 'parsing' }` | UI progress indicator |

Store handlers are the `_on*` internal actions in §1.1.

### 5.3 Settings shape

```ts
interface Settings {
  hotkey: string;                          // default: "Ctrl+Shift+Space"; see §10
  staleness_inbox_days: number | null;     // default 3
  staleness_a_days: number | null;         // default 14
  staleness_b_days: number | null;         // default 21
  staleness_c_days: number | null;         // default null — null disables staleness flagging for that tier
  lan_capture_enabled: boolean;            // default false
  lan_capture_port: number;                // default 47821
  lan_capture_shared_secret: string | null;// default null; if set, required as ?s= query param (see §10)
  llm: {
    base_url: string;                      // default "http://localhost:11434/v1" (Ollama)
    model: string;                         // default "llama3.2"
    has_api_key: boolean;                  // true if key exists in keychain
    timeout_ms: number;                    // default 30000
  };
  analyze_window_days: number;             // default 30
}
```

`api_key` is never returned to the frontend. Only `has_api_key: boolean`. Set via `set_llm_config` which writes to OS keychain.

---

## 6. Rust module layout

```
src-tauri/src/
├── main.rs                    — Tauri builder, command registration, bootstrap
├── lib.rs                     — re-exports (for unit testing harness)
│
├── db/
│   ├── mod.rs                 — Pool setup, migration runner, transaction helpers
│   ├── events.rs              — append_event, get_events, iterate_events
│   ├── items.rs               — items table read queries, update_from_event
│   └── projection.rs          — replay events → rebuild items table; verify_projection
│
├── domain/
│   ├── mod.rs
│   ├── item.rs                — Item struct, Tier enum, State enum
│   ├── event.rs               — Event struct, EventType enum, typed Payload variants
│   ├── rank.rs                — fractional-indexing helpers
│   └── capacity.rs            — cap constants, cap check functions
│
├── commands/
│   ├── mod.rs                 — Error type, Result alias, shared helpers
│   ├── bootstrap.rs           — bootstrap command
│   ├── items.rs               — create/edit/move/delete/state/date commands
│   ├── swap.rs                — swap_move command (its own file; transactional)
│   ├── events.rs              — get_events, get_items_at commands
│   ├── settings.rs            — get/update settings
│   ├── capture.rs             — toggle_lan_capture
│   └── llm.rs                 — set_llm_config, test_llm_connection, analyze, accept/reject
│
├── capture/
│   ├── mod.rs                 — lifecycle: start, stop, is_running
│   ├── server.rs              — axum router, routes
│   ├── html.rs                — compile-time embedded capture page (include_str!)
│   └── ip.rs                  — LAN IP detection (first non-loopback IPv4)
│
├── llm/
│   ├── mod.rs                 — LlmClient trait
│   ├── openai_compat.rs       — OpenAI-compatible endpoint implementation
│   ├── prompt.rs              — system + user prompt templates (const strs)
│   ├── compression.rs         — event log → compact prompt input
│   └── parse.rs               — strict parser for LLM response → Observation[]
│
├── keychain.rs                — thin wrapper around `keyring` crate
├── settings_file.rs           — JSON settings file in app-data dir (non-secret fields)
├── hotkey.rs                  — global shortcut registration via tauri-plugin-global-shortcut
├── error.rs                   — BayError enum + serde + From impls
└── tracing.rs                 — log setup (to file in app-data dir)
```

### 6.1 Key dependencies (Cargo.toml)

```toml
[dependencies]
tauri = { version = "2", features = ["macos-private-api"] }
tauri-plugin-global-shortcut = "2"
tauri-plugin-dialog = "2"
tauri-plugin-fs = "2"

rusqlite = { version = "0.32", features = ["bundled"] }
r2d2 = "0.8"
r2d2_sqlite = "0.25"

serde = { version = "1", features = ["derive"] }
serde_json = "1"

axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors"] }

reqwest = { version = "0.12", features = ["json", "rustls-tls"] }

keyring = "3"
qrcode = "0.14"
local-ip-address = "0.6"

thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-appender = "0.2"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

uuid = { version = "1", features = ["v7"] }
```

UUIDv7 for item ids: time-ordered, so items sort naturally by creation time when ranks tie.

### 6.2 Frontend dependencies (package.json)

```json
{
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-global-shortcut": "^2",
    "@dnd-kit/core": "^6",
    "@dnd-kit/sortable": "^8",
    "@dnd-kit/utilities": "^3",
    "react": "^18",
    "react-dom": "^18",
    "zustand": "^4",
    "zod": "^3",
    "date-fns": "^3"
  }
}
```

No UI component library. Styling with CSS modules or vanilla CSS per system-theme-only directive. If a calendar grid becomes painful, consider `@internationalized/date` + hand-rolled grid; do not pull in a date-picker library.

---

## 7. LAN capture server

### 7.1 Lifecycle

- **Default**: disabled.
- **Enable flow**: user toggles in Settings → `toggle_lan_capture(enabled: true)` → Rust starts axum on `0.0.0.0:47821` → returns `{ enabled: true, url: "http://<LAN-IP>:47821/" }`.
- **Disable flow**: reverse. Graceful shutdown (tokio `shutdown_signal`). Port released.
- **On port conflict**: command returns `PORT_IN_USE`. User can change port via `settings.lan_capture_port` (default 47821; see §5.3).
- **On app quit**: server stops. No autostart in v1.

### 7.2 Routes

| Method | Path | Response |
|---|---|---|
| `GET` | `/` | HTML capture page (see §7.3) |
| `POST` | `/capture` | `{ ok: true, id: string }` on success; 400 on empty content; 401 on shared-secret mismatch |
| `GET` | `/health` | `{ ok: true, app: "Bay", version: "x.y.z" }` |

### 7.3 Capture page

Single HTML file, compile-time embedded via `include_str!`. Self-contained: no external JS/CSS. Viewport meta for mobile. Dark-mode-aware via `prefers-color-scheme`. No framework.

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Bay · Capture</title>
  <style>
    /* minimal, system-font, single-column, mobile-first */
  </style>
</head>
<body>
  <main>
    <h1>Capture → Inbox</h1>
    <textarea id="c" rows="6" autofocus placeholder="What came to mind?"></textarea>
    <button id="b">Capture</button>
    <div id="status" aria-live="polite"></div>
    <section>
      <h2>Recent (this session)</h2>
      <ul id="recent"></ul>
    </section>
  </main>
  <script>
    // Read ?s=... shared secret from URL on load, include in POST.
    // fetch('/capture', { method: 'POST', ... }) on button click.
    // On success, prepend to Recent list with HH:MM timestamp.
    // On failure, show error in #status.
    // No localStorage: Recent is session-only.
  </script>
</body>
</html>
```

### 7.4 Shared secret (optional hardening)

If `settings.lan_capture_shared_secret` is set:
- Capture page URL includes `?s=<secret>` query param in the rendered QR code.
- `POST /capture` requires matching `?s=` (or `X-Bay-Secret` header). Mismatch → 401.
- Not shown in Settings UI by default; expose behind a "Advanced" disclosure.

This addresses the coffee-shop scenario Bo flagged. Default remains no-auth LAN-trust.

### 7.5 CORS and binding

- Bind explicitly to `0.0.0.0:47821` when enabled. Never bind on startup unconditionally.
- CORS allow-origin: `*` for GET `/` and `/health`; POST `/capture` requires same-origin or omits CORS headers entirely (non-preflight simple request — acceptable for v1). Revisit if we ever add cross-origin clients.
- No TLS. LAN-trust model is explicit in docs.

### 7.6 Settings-page rendering

Settings page shows, when enabled:
- Current LAN URL (with secret if set).
- QR code PNG rendered in-app via `qrcode` crate, displayed in a dialog.
- Copy-to-clipboard button.
- Number of captures received this session.

---

## 8. LLM integration

### 8.1 Client trait

```rust
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn test_connection(&self) -> Result<TestResult, LlmError>;
    async fn analyze(&self, prompt: &AnalyzePrompt) -> Result<Vec<Observation>, LlmError>;
}
```

Single implementation in v1: `OpenAiCompatClient`. Works against OpenAI, Anthropic (via proxy), Ollama, LM Studio — all expose OpenAI-compatible `/v1/chat/completions`.

### 8.2 Prompt construction

System prompt (const string in `prompt.rs`):

```
You are an analyst observing a single user's task management event log.
Your job: identify patterns the user would benefit from seeing.

Rules:
- Observe only. Do not suggest specific reassignments.
- Output strictly valid JSON matching the schema provided.
- Prefer sharp, specific observations over generic advice.
- If the data shows no interesting patterns, return an empty array.
- Do not repeat what the user can trivially see by looking at the board.

Output schema:
{
  "observations": [
    { "severity": "info" | "warn", "text": "string (<= 200 chars)",
      "affected_item_ids": [ "string" ] }
  ]
}
```

User prompt template:

```
Window: last {N} days ({from_iso} to {to_iso})
Total events in window: {event_count}

=== AGGREGATE ===
- Items created: {by_tier_counts}
- Items moved between tiers: {transition_counts}
- Items completed: {done_count}
- Items currently blocked: {blocked_count}
- Average time in A before done: {avg_ms_in_A_done}
- Median time in A before demoted: {median_ms_in_A_demoted}

=== CURRENT BOARD ===
Inbox: {inbox_count} items
A ({a_count}/5 active): {a_list}
B ({b_count}/12 active): {b_list}
C ({c_count} items): {c_summary}

=== STALE ITEMS (>{staleness_days}d untouched) ===
{stale_list}

Return observations JSON.
```

`{a_list}` format: `[{id, content, days_in_tier, state}]`, JSON-encoded. Similarly for B. C summarized as counts only to keep prompt short.

### 8.3 Event log compression

`compression.rs` computes the aggregates server-side in SQL, never ships raw events to the LLM in v1. This:
- Keeps prompt size bounded (<4k tokens for any reasonable user).
- Avoids leaking item content the LLM doesn't need.
- Keeps inference fast on local Ollama.

v2 option: ship raw event sample when user requests deeper analysis. Not v1.

Target input budget: ≤2500 tokens system+user. Target output: ≤500 tokens.

### 8.4 Output parsing

Strict JSON parse. If parse fails:
- Retry once with prompt prefix `"Your previous response was not valid JSON. Return only the JSON object, no prose. Schema: ..."`.
- On second failure, return `LLM_PARSE_ERROR` to caller; log suggestion with empty observations and parse error details.

`Observation.affected_item_ids` are validated to exist; unknown ids dropped with a warn log.

### 8.5 Error surfaces

| LlmError | User-facing message |
|---|---|
| `Unreachable` | "Can't reach the LLM endpoint. Check that Ollama is running or your API endpoint is correct." |
| `AuthFailed` | "LLM rejected the API key. Re-enter your key in Settings." |
| `Timeout` | "LLM took longer than {N}s. Increase timeout in Settings or try a smaller model." |
| `ParseError` | "LLM returned an unparseable response. This may be a model quality issue — try a different model." |
| `RateLimited` | "LLM endpoint is rate limiting. Try again in a moment." |

All surfaced in `AnalyzePanel` inline, not as blocking modals.

### 8.6 No streaming in v1

`chat/completions` called with `stream: false`. Single response. Progress indicator uses backend `analyze_progress` events (`compressing` / `calling_llm` / `parsing`). Streaming is a v2 nicety.

---

## 9. Build plan

Fourteen increments. Each produces a buildable, runnable, demonstrable state. Commit after each. Target: ≤500 LOC net change per increment; flag if exceeded.

Conventions for every increment:
- `cargo build && cargo test && pnpm build && pnpm test` must pass.
- One-sentence demo: what the user can observably do after this commit that they couldn't before.
- No speculative scaffolding. Only what's needed for the increment's demo.

### I-01 — Scaffold and empty board

- `pnpm create tauri-app@latest` → Tauri 2, React+TS, Vite.
- Wire `@tauri-apps/api` and verify `invoke("bootstrap")` returns stub data.
- Render four empty bays (Inbox, A, B, C) with headers and counters showing 0.
- System-theme CSS (light/dark via `prefers-color-scheme`).
- No DB yet; `bootstrap` returns `{ items: [], settings: <defaults> }`.

Demo: app opens, four empty bays visible, top bar with view switcher (non-functional).

### I-02 — Schema, migrations, domain types

- `migrations/001_initial.sql`: `events` and `items` tables per §4.
- `db/mod.rs` migration runner (checks `PRAGMA user_version`).
- `domain/item.rs`, `domain/event.rs`, `domain/rank.rs`, `domain/capacity.rs`.
- Zod schemas in `src/domain.ts` mirroring Rust types.
- `bootstrap` opens DB, runs migrations, returns empty item list.

Demo: DB file created on first launch at platform-appropriate path; `sqlite3 bay.db .schema` shows expected tables.

### I-03 — Event store + create_item end-to-end

- `db/events.rs::append_event` transaction helper.
- `db/items.rs::apply_event_to_projection` — handles `ITEM_CREATED`.
- `commands/items.rs::create_item` command.
- Frontend: temporary dev-only "Add item" button on Inbox bay that calls `createItem("inbox", "test")`.

Demo: click Add → item appears in DB (events + items rows); after restart, re-appears (loaded via bootstrap — but render not wired until I-04).

### I-04 — Read path: render items from DB

- `bootstrap` now returns real items.
- Store: `itemsByTier` selector computes sorted lists per tier from rank.
- `Strip` component renders content, drag handle placeholder, overflow menu placeholder.
- `Bay` renders strips in rank order.
- `BayHeader` shows correct counter and `N/cap` for A and B.

Demo: dev-add-button inserts item; bay shows it without reload.

### I-05 — Create UI with capacity check; hotkey capture

- `BayHeader` `+ Add` button opens inline input (not modal) for that bay.
- For A / B: if cap reached, `+ Add` is disabled with tooltip "Use drag or swap". No swap UI yet.
- Global hotkey registration via `tauri-plugin-global-shortcut` with default `Ctrl+Shift+Space`.
- `QuickCaptureModal`: single textarea, Enter commits to Inbox, Esc cancels.
- Replace dev-only Add button.

Demo: hotkey anywhere on OS → capture modal → type → Enter → item in Inbox. Direct `+Add` on each bay works, subject to cap.

### I-06 — Intra-tier drag reorder

- `@dnd-kit/core` + `@dnd-kit/sortable` wiring.
- `Bay` becomes `SortableContext`. `Strip` becomes `useSortable`.
- `onDragEnd` within same bay: compute new rank via `rank_between`, call `move_item` command.
- `ITEM_MOVED` events emitted. Backend applies to projection.

Demo: drag to reorder within Inbox/A/B/C; order persists across reload; events visible in `sqlite3` browser.

### I-07 — Cross-tier drag with reason + swap flow

- `onDragEnd` across tiers: if target is A/B and `count_active(target) >= cap`, open `SwapModal`; else open `MoveReasonModal`.
- Implement `swap_move` command — transactional two-event emit.
- `MoveReasonModal`: optional reason, confirm/cancel (cancel reverts drag).
- `SwapModal`: list current A (or B) active items, select one to demote, choose B or C destination, optional reason.
- Frontend enforces that if drag comes from Inbox → A-at-cap, the swap is: incoming item replaces an outgoing one.

Demo: drag A item to C with reason logged; drag Inbox item to full A, pick demotion, verify both events in DB.

### I-08 — State transitions (block / done) + overflow menu

- Strip overflow menu: Edit, Set Dates (stubbed), Mark Done, Mark Blocked, Delete.
- `BlockModal` requires reason.
- `done` items render with strikethrough + collapsed single-line + muted color.
- Done items visible until next app launch, then auto-hidden. Per-bay "Show N earlier done items" link reveals them for the current session only. Session visibility state is Zustand UI state, not persisted.
- Undo-delete toast: on delete, show "Deleted. Undo" toast for 10 seconds. Undo calls `restore_item`, which emits `ITEM_RESTORED` and clears `items.deleted=1`.
- Done items don't count toward A/B caps.

Demo: mark item blocked with reason; mark another done; done collapses inline; cap logic unaffected by done count; delete an item and click Undo in the toast within 10s → item returns via `ITEM_RESTORED`; restart app → done items auto-hidden; per-bay "Show N earlier done items" link reveals them for the session.

### I-09 — Dates + staleness + calendar view

- Strip overflow: Set Start Date / Set Due Date → native date input → `set_item_date` command.
- Staleness: per-tier thresholds (Inbox 3d, A 14d, B 21d, C null by default). Compute `days_since_last_event(item_id)`; if the threshold for the item's tier is non-null and exceeded, Strip shows ⚠ badge. A null threshold disables flagging for that tier. All four thresholds configurable in Settings (wired in I-11).
- `CalendarView`: month grid (`date-fns`), `DayCell` highlights days with items. Click → day sheet listing items with that start or due.
- View switcher wired.

Demo: set a due date; day shows pill on calendar; leave an A item untouched 14+ days (or set back-dated event `ts` in DB for testing) → ⚠ appears; a same-age Inbox item flags earlier (>3d); a C item of any age does not flag.

### I-10 — Inspector panel + time-travel

- Click strip → `selectedItemId` set → `InspectorPanel` opens with item header + event history (via `get_events({item_id})`).
- Time-travel: `TimestampPicker` scrubber at top of view. Selecting a timestamp invokes `get_items_at(ts)` and renders the board read-only.
- `get_items_at` implemented by replaying events into an in-memory projection up to `ts`.
- `rebuild_projection` command: truncates the `items` table and replays all events to reconstruct the projection from scratch. Returns `{items_affected}`. Exposed as a manual dev command in I-10; Settings wiring in I-11.
- Exit button returns to live board.

Demo: move item around, open inspector, see full event trail; enter time-travel at a past timestamp, see old board state; exit; invoke `rebuild_projection` from devtools → items table rebuilt identically.

### I-11 — Settings page + keychain

- `SettingsView` with sections: General (hotkey, per-tier staleness thresholds for Inbox/A/B/C with null = disabled, close-to-tray, "Export event log" button), Capture (LAN toggle — non-functional stub), LLM (base_url, model, api_key input, timeout), Advanced ("Rebuild projection from event log" button wired to `rebuild_projection` behind a confirmation dialog).
- `export_events` command: opens a native save dialog via `tauri-plugin-dialog`; writes every event row as JSONL to the chosen path. Returns `{events_written, path}`.
- Settings persisted to JSON file in app-data dir (non-secret fields); api_key to OS keychain via `keyring`.
- `has_api_key: bool` exposed; real key never crosses IPC after write.
- Hotkey change re-registers global shortcut.

Demo: change A staleness threshold to 7 days → ⚠ now appears on 7-day-old A items; enter LLM API key, verify via `keyring` CLI that it's in keychain; click Export event log → choose a path → JSONL file written with a row count matching `SELECT COUNT(*) FROM events`.

### I-12 — LAN capture server

- `capture/server.rs` with axum routes.
- `capture/html.rs` with embedded HTML.
- `toggle_lan_capture` command starts/stops server.
- `LAN IP + QR code` rendered in Settings when enabled. Uses `local-ip-address` + `qrcode` crates.
- POST `/capture` creates Inbox item; emits `lan_capture_received` event; frontend shows toast.
- Optional shared-secret check gated behind settings flag.

Demo: enable LAN capture; scan QR from phone; submit text from phone browser; item appears in Inbox on desktop; toast notification.

### I-13 — LLM client + test connection

- `llm/openai_compat.rs` implementation.
- `test_llm_connection` command: sends 2-token chat request, returns latency and echoed model name.
- Settings page: "Test connection" button → shows result or error.
- `set_llm_config` writes non-secret fields to settings file, api_key to keychain.

Demo: point at local Ollama (`http://localhost:11434/v1` + `llama3.2`) → test → green checkmark with latency. Wrong URL → red error with message.

### I-14 — Analyze prompt, compression, UI, accept/reject

- `llm/compression.rs`: SQL-driven aggregate computation.
- `llm/prompt.rs`: template filling.
- `llm/parse.rs`: strict observation parser with one retry.
- `analyze` command: emits `LLM_SUGGESTION_GENERATED` event with result, returns observations.
- `AnalyzePanel`: renders observations with severity icons; Mark Reviewed / Dismiss buttons.
- `accept_suggestion` / `reject_suggestion` commands emit respective events.
- `analyze_progress` events drive a small in-panel progress indicator.

Demo: board with ~20 items and some recent moves + completions → click Analyze → 2-5 observations appear within ~5s on Ollama → Mark Reviewed logs acceptance event.

### Post-v1 checklist (not part of v1)

- Rank rebalance implementation.
- LLM re-org proposals with accept/reject resulting in real events.
- Recurring tasks.
- Archive view for soft-deleted items.
- LLM response streaming.
- Per-increment unit tests tightened to behavior tests (currently each increment has shallow tests; harden in a v1.1 pass).

---

## 10. Decisions resolved (2026-04-24)

All decisions from v1.0 §10 are resolved. Below is the resolved state for audit trail.

### 10.1 Global hotkey default

**RESOLVED (default):** Ctrl+Alt+N as the shipping default; user-configurable in Settings.

**Ambiguity**: `Ctrl+Shift+Space` (prior proposal) conflicts with Windows IME language switch and some screen-reader commands.

**Recommended default**: `Ctrl+Alt+N` ("new"). No common conflicts on Windows/macOS/Linux. User-configurable in Settings.

**Alternative if you push back**: keep `Ctrl+Shift+Space` and accept conflict; ship with a prominent "Change hotkey" prompt on first launch.

### 10.2 Done items — archive semantics

**RESOLVED (revised):** No `show_done_items` toggle. Done items remain in their bay until next app launch, then auto-hidden; each bay exposes a "Show N earlier done items" link that reveals them for the current session only (Zustand UI state, not persisted).

**Ambiguity**: CLAUDE.md says done items "stay visible until manual archive or session end." "Archive" is undefined.

**Recommended default**: Settings toggle `show_done_items` (default true). When true, done items remain in their bay position with strikethrough + collapsed height + muted color, until explicitly deleted. When false, done items are hidden from bay render (still in DB, still in inspector history, still in time-travel). No "archive" as a distinct state — just a display toggle.

**Alternative**: add a fourth state `archived` reachable from `done` via explicit action, with a dedicated `Archive` view. More complex, more semantic, probably overkill for v1.

### 10.3 Soft-delete visibility

**RESOLVED (revised):** Undo-delete toast ("Deleted. Undo") appears for 10 seconds after delete; click emits `ITEM_RESTORED` via `restore_item` and clears `items.deleted=1`. No other restoration UI in v1.

**Ambiguity**: soft-deleted items are excluded from projection but preserved in event log. Can the user see or restore them?

**Recommended default**: no v1 UI for restoration. Deleted items visible only via time-travel (restore by… nothing, time-travel is read-only). Event history viewable only if user knows the item id (manual DB query).

**Alternative**: add a "Trash" view with restore button. Scope creep; defer to v1.5 if anyone asks.

### 10.4 Rank rebalance trigger

**RESOLVED (default):** Implement `rank_between`; no rebalance trigger in v1.

**Ambiguity**: rebalance logic is specified; trigger is not.

**Recommended default**: implement `rank_between` with the rebalance-safe library, but never trigger rebalance in v1. Worst case after a year of heavy use: rank strings of 40+ chars, still fast to compare. Defer rebalance to when empirically needed.

**Alternative**: rebalance eagerly on any rank ≥64 chars. Simple, probably unnecessary.

### 10.5 `LLM_SUGGESTION_ACCEPTED.resulting_events`

**RESOLVED (default):** "Mark reviewed" emits `LLM_SUGGESTION_ACCEPTED` with `resulting_event_ids: []`. Schema preserved unchanged for v2.

**Ambiguity**: schema includes `resulting_event_ids`, but v1 LLM scope is advisory-only with no mutations. So what does "accept" mean in v1?

**Recommended default**: "Mark reviewed" action emits `LLM_SUGGESTION_ACCEPTED` with empty `resulting_event_ids: []`. Semantically: "user saw this and acknowledged it." This preserves the schema for v2 re-org proposals without changing it.

**Alternative**: drop `resulting_event_ids` from v1 schema, re-add in v2. Backward-incompatible schema change, not worth saving two fields.

### 10.6 Time-travel write protection

**RESOLVED (default):** Time-travel view is fully read-only; all mutation controls disabled. Hotkey capture still commits to live Inbox independent of view.

**Ambiguity**: in time-travel view, are user mutations prevented, ignored, or allowed-with-warning?

**Recommended default**: view is fully read-only. All mutation buttons disabled. Drag returns item to original position. Hotkey capture still works (always goes to live Inbox, independent of view).

**Alternative**: allow mutations but treat them as applied to "now" with the time-travel view unchanged. Confusing; reject.

### 10.7 Staleness default

**RESOLVED (revised):** Per-tier thresholds — Inbox 3d, A 14d, B 21d, C null (disabled by default). All four configurable in Settings; a null threshold disables flagging for that tier. Alternative (per-tier) was taken over the default (uniform 14d).

**Ambiguity**: staleness threshold not specified in CLAUDE.md.

**Recommended default**: 14 days, user-configurable in Settings. Applies to all active items regardless of tier. Trigger: `now - max(event.ts WHERE event.item_id = X) > threshold`.

**Alternative**: per-tier thresholds (e.g., A = 7d, B = 21d, C = infinite). More precise, more complex; defer.

### 10.8 LAN capture port conflict

**RESOLVED (default):** Start failure returns `PORT_IN_USE`; user-configurable `lan_capture_port` in Settings (default 47821). (Note: the Settings shape in §5.3 does not yet list `lan_capture_port` as a field — pending follow-up.)

**Ambiguity**: port `47821` is hardcoded. What if it's taken?

**Recommended default**: if start fails, return `PORT_IN_USE` error. Add `lan_capture_port` to Settings with default 47821; user can change. QR code regenerates on port change.

**Alternative**: try next available port automatically. Surprising if the port changes silently; reject.

### 10.9 LAN capture shared secret default

**RESOLVED (default):** Shared-secret infrastructure implemented but disabled; exposed behind an "Advanced" disclosure in Settings → Capture.

**Ambiguity**: CLAUDE.md ships no-auth by default, notes shared secret as v1 option.

**Recommended default**: ship with shared-secret infrastructure implemented but disabled. Expose behind a "Security" disclosure in Settings → Capture. Makes belt-and-suspenders hardening a one-click toggle without forcing complexity on home-network use.

**Alternative**: ship disabled and don't build it until someone needs it. But §7.4 spec is cheap to include.

### 10.10 Window close behavior

**RESOLVED (default):** Close → minimize to tray; quit only via tray menu or Cmd/Ctrl+Q with confirmation. Global hotkey remains active while in tray.

**Ambiguity**: does closing the window quit the app or minimize to tray? Affects global hotkey availability.

**Recommended default**: close → minimize to tray. App continues running; global hotkey stays active. Quit only via tray menu → Quit or `Cmd/Ctrl+Q` with confirmation.

**Alternative**: close quits. Then global hotkey only works when app is open, which defeats most of its purpose. Reject.

### 10.11 Multi-monitor quick-capture position

**RESOLVED (default):** Quick-capture modal centered on the monitor containing the mouse cursor at hotkey-press time.

**Ambiguity**: which monitor shows the modal?

**Recommended default**: monitor containing the mouse cursor at hotkey-press time. Centered.

**Alternative**: always primary monitor. Annoying for users with asymmetric setups. Reject.

### 10.12 C virtualization threshold

**RESOLVED (default):** No virtualization in v1. At 100+ items, C default-collapses to "show first 50, click to expand." Virtualization deferred.

**Ambiguity**: C is unbounded. At 500+ items, rendering degrades.

**Recommended default**: no virtualization in v1. If C exceeds 100 items, default-collapse to "show first 50, click to expand all." Virtualization (e.g., `@tanstack/virtual`) deferred unless performance testing shows real problems with 100-item render.

**Alternative**: virtualize from day one. Premature.

### 10.13 Inconsistency flag — "actionable" wording

**RESOLVED (default):** "Actionable" is aspirational doctrine, not a mechanical constraint; no enforcement. Wording retained as-is.

**Flag only**: CLAUDE.md says Inbox items "must be hand-tiered into A/B/C before they're actionable." This is doctrine, not a mechanical constraint — the app does not prevent working on an Inbox item (no such concept as "mark active from inbox"). Reading it as intent: Inbox is the triage holding pen, A/B/C are the working trays. No enforcement needed; the wording is aspirational. Keeping as-is.

### 10.14 Inconsistency flag — done/blocked and cap math

**RESOLVED (default):** A/B counter displays `count_active(tier)/cap`; blocked and done items don't count. Total item count (including blocked/done) shown as a secondary/tooltip value per body.

**Flag only**: CLAUDE.md is explicit that caps apply to `active` only, and done/blocked don't count. This means a tier can have arbitrary total items as long as only 5 are active in A. The spec reflects this (§3.2 guard). Document it clearly in the UI: the A counter should display `3 / 5` where `3 = count_active(A)` and `5 = cap`. Total A item count (including blocked/done) shown as secondary, perhaps in a tooltip.

---

*End of SPEC.md. Current version: v1.2. Prior versions in archive/. Revision protocol: append-only archive per pass, v-header at top, inline edits allowed within version bumps.*
