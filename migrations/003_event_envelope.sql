-- 003_event_envelope.sql — event envelope v2 (v0.3, ADR-007 / VISION §3.0 T1).
--
-- Adds six provenance/transaction/integrity columns to the append-only
-- `events` table, a `meta` table for durable per-database identity, and
-- the txn index that undo grouping uses.
--
--   txn_id     TEXT    — one uuid per `write_events` call. THE transaction
--                        boundary. Undo groups compensating events by
--                        txn_id (closes QUESTIONS Q01; the (ts,type)
--                        heuristic becomes the legacy-rows fallback).
--   actor      TEXT    — 'human' | 'system'. The LLM is NOT an actor: it
--                        has no write path (ProjectionEvent firewall).
--                        'system' is reserved for deterministic execution
--                        of human-configured timers (VISION law 6 — e.g.
--                        the Today day-roll).
--   origin     TEXT    — finer provenance where trivially known:
--                        'lan', 'llm_accept:<event id>', 'undo:<txn id>'.
--                        Granularity grows over time; NULL = unrecorded.
--   device_id  TEXT    — writer identity (from meta.device_id), preparing
--                        sync-as-replication (VISION §3.8). Meaningless
--                        but harmless single-device.
--   schema_ver INTEGER — per-event payload schema version. Bump per type
--                        when a payload shape changes; upcasters read old
--                        versions. A forever-log needs this eventually;
--                        it is cheapest now.
--   prev_hash  TEXT    — SHA-256 over the previous event row: the log
--                        becomes a hash CHAIN, tamper-EVIDENT end to end
--                        ("perfect self-auditability" made cryptographic).
--                        The first event chains from the 64-zero genesis.
--
-- ALTER TABLE ADD COLUMN deliberately, NOT a table rebuild: `events` is
-- the source of truth, and a copy-rebuild would put DROP TABLE events in
-- a migration (AUTONOMY_CHARTER §5 "never DROP tables"; data risk for
-- zero benefit). ALTER is DDL — it does not fire the migration-002
-- append-only row triggers, which remain in force untouched. Legacy rows
-- keep NULL in every new column (valid forever; readers treat NULL as
-- pre-envelope). scripts/verify-schema.py verifies ALTERed tables with a
-- column-set check instead of CREATE byte-matching.

ALTER TABLE events ADD COLUMN txn_id TEXT;
ALTER TABLE events ADD COLUMN actor TEXT CHECK (actor IS NULL OR actor IN ('human','system'));
ALTER TABLE events ADD COLUMN origin TEXT;
ALTER TABLE events ADD COLUMN device_id TEXT;
ALTER TABLE events ADD COLUMN schema_ver INTEGER CHECK (schema_ver IS NULL OR schema_ver >= 1);
ALTER TABLE events ADD COLUMN prev_hash TEXT;

CREATE INDEX idx_events_txn ON events(txn_id);

-- Durable per-database key/value store. First key: device_id, seeded by
-- the migration runner (Rust generates the uuid; SQL cannot). Lives in
-- the DB rather than settings.json so the identity travels with the
-- data it stamps.
CREATE TABLE meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
