// Domain types for Bay's frontend, mirrored from src-tauri/src/domain/*
// via Zod schemas. Keep this file in lockstep with the Rust side — any
// divergence breaks IPC at bootstrap. See CLAUDE.md §Data model and
// SPEC §4.1, §4.3, §5.3.

import { z } from "zod";

// ── primitives ───────────────────────────────────────────────────────────────

export const Tier = z.enum(["inbox", "A", "B", "C"]);
export type Tier = z.infer<typeof Tier>;

export const ItemState = z.enum(["active", "blocked", "done"]);
export type ItemState = z.infer<typeof ItemState>;

// ── event log ────────────────────────────────────────────────────────────────

export const EventType = z.enum([
  "ITEM_CREATED",
  "ITEM_EDITED",
  "ITEM_MOVED",
  "ITEM_STATE_CHANGED",
  "ITEM_DATE_SET",
  "ITEM_DELETED",
  "ITEM_RESTORED",
  "ITEM_RECURRENCE_SET",
  "ITEM_RECURRED",
  "ITEM_FIRST_STEP_SET",
  "TODAY_ADDED",
  "TODAY_REMOVED",
  "DAY_OPENED",
  "DAY_CLOSED",
  "SESSION_STARTED",
  "SESSION_ENDED",
  "LLM_SUGGESTION_GENERATED",
  "LLM_SUGGESTION_ACCEPTED",
  "LLM_SUGGESTION_REJECTED",
]);
export type EventType = z.infer<typeof EventType>;

export const Event = z.object({
  id: z.number(),
  ts: z.number(),
  type: EventType,
  item_id: z.string().nullable(),
  payload: z.unknown(),
  // Envelope v2 (migration 003) — omitted by the backend on legacy
  // (pre-envelope) rows, so every field is optional here.
  txn_id: z.string().nullable().optional(),
  actor: z.enum(["human", "system"]).nullable().optional(),
  origin: z.string().nullable().optional(),
  device_id: z.string().nullable().optional(),
  schema_ver: z.number().nullable().optional(),
  prev_hash: z.string().nullable().optional(),
});
export type Event = z.infer<typeof Event>;

// ── items projection ─────────────────────────────────────────────────────────

export const Item = z.object({
  id: z.string(),
  content: z.string(),
  tier: Tier,
  rank: z.string(),
  state: ItemState,
  blocked_reason: z.string().nullable(),
  start_at: z.number().nullable(),
  due_at: z.number().nullable(),
  // I-21: canonical RRULE-subset string (FREQ=…[;INTERVAL=n]) or null.
  recurrence: z.string().nullable(),
  // v0.3 execution core: the single next physical action (≤140 chars).
  first_step: z.string().nullable(),
  // v0.3: local date (YYYY-MM-DD) on the Today overlay, or null.
  today_on: z.string().nullable(),
  created_at: z.number(),
  updated_at: z.number(),
  deleted: z.boolean(),
});
export type Item = z.infer<typeof Item>;

// ── sessions (v0.3) ──────────────────────────────────────────────────────────

export const SessionOutcome = z.enum(["done", "progress", "interrupted"]);
export type SessionOutcome = z.infer<typeof SessionOutcome>;

export const INTERRUPT_REASONS = [
  "meeting",
  "person",
  "self_switch",
  "blocked",
  "energy",
] as const;

export const Session = z.object({
  id: z.string(),
  item_id: z.string(),
  started_at: z.number(),
  ended_at: z.number().nullable(),
  outcome: SessionOutcome.nullable(),
  reason: z.string().nullable(),
  note: z.string().nullable(),
});
export type Session = z.infer<typeof Session>;

export const EndSessionResult = z.object({
  session: Session,
  item: Item,
  spawned: z.array(Item),
});
export type EndSessionResult = z.infer<typeof EndSessionResult>;

// ── settings ─────────────────────────────────────────────────────────────────

export const LlmSettings = z.object({
  base_url: z.string(),
  model: z.string(),
  has_api_key: z.boolean(),
  timeout_ms: z.number(),
});
export type LlmSettings = z.infer<typeof LlmSettings>;

export const Settings = z.object({
  hotkey: z.string(),
  staleness_inbox_days: z.number().nullable(),
  staleness_a_days: z.number().nullable(),
  staleness_b_days: z.number().nullable(),
  staleness_c_days: z.number().nullable(),
  lan_capture_enabled: z.boolean(),
  lan_capture_port: z.number(),
  lan_capture_shared_secret: z.string().nullable(),
  llm: LlmSettings,
  analyze_window_days: z.number(),
  /** When true, closing the window hides to tray; otherwise OS-level
   *  close. Backend defaults to true; clients written before this
   *  field existed parse fine because the backend always returns it. */
  close_to_tray: z.boolean(),
});
export type Settings = z.infer<typeof Settings>;

// ── bootstrap payload ────────────────────────────────────────────────────────

export const BootstrapResult = z.object({
  items: z.array(Item),
  settings: Settings,
});
export type BootstrapResult = z.infer<typeof BootstrapResult>;

// ── capacity constants ───────────────────────────────────────────────────────
// Mirrors src-tauri/src/domain/capacity.rs. Caps apply to active items only.

export const A_CAP = 5;
export const B_CAP = 12;
