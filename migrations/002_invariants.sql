-- 002_invariants.sql — DB-enforced invariants for Bay v0.2.0.
--
-- Migration 001 created the schema; this migration adds the mechanical
-- enforcement that CLAUDE.md previously asserted only as prose:
--
--   1. `events` is append-only. A BEFORE UPDATE OR DELETE trigger
--      raises ABORT. This makes `UPDATE events SET ...` / `DELETE FROM
--      events` mechanically impossible from any code path. The only
--      write path is INSERT via `db::write_events` -> `events::append_event`.
--      CLAUDE.md: "If you ever find yourself writing UPDATE events SET
--      or DELETE FROM events, stop. You have misunderstood the architecture."
--      This trigger turns that doctrine into a runtime truth.
--
--   2. `items` CHECK constraints pin the invariants the projection
--      handlers currently enforce in Rust:
--        - deleted IN (0, 1)              (soft-delete flag is boolean)
--        - length(content) BETWEEN 1 AND 4096  (SPEC 4.3; matches Rust
--          MAX_CONTENT_LEN counted as Unicode scalar values)
--        - length(rank) >= 1              (rank is never empty)
--        - state != 'blocked' OR blocked_reason IS NOT NULL
--          (SPEC 3.1 guard: blocked requires a reason)
--
-- These are additive (no column changes, no data migration). Existing
-- rows satisfy all constraints by construction (the Rust handlers
-- already enforce them). PRAGMA user_version bumps 1 -> 2.
--
-- The trigger uses `raise(ABORT, ...)` so the violating statement
-- fails fast with a doctrine-named message that surfaces in sqlite3
-- error output and the rusqlite error chain.

-- ── events append-only trigger ───────────────────────────────────
-- Any UPDATE or DELETE on events aborts. INSERT is the only legal
-- mutation. This is the load-bearing append-only invariant, enforced
-- at the storage layer so no code path — present or future — can
-- violate it without an explicit migration that drops the trigger.

CREATE TRIGGER events_no_update BEFORE UPDATE ON events
BEGIN
  SELECT raise(ABORT, 'events is append-only (Bay doctrine): UPDATE refused');
END;

CREATE TRIGGER events_no_delete BEFORE DELETE ON events
BEGIN
  SELECT raise(ABORT, 'events is append-only (Bay doctrine): DELETE refused');
END;

-- ── items CHECK constraints ──────────────────────────────────────
-- Add the constraints the projection handlers enforce in Rust, so a
-- bug in a handler (or a future hand-rolled write path) can't land a
-- row that violates the invariants. SQLite ALTER TABLE ADD CONSTRAINT
-- doesn't exist; we use a table-rebuild via the standard CREATE TABLE
-- ... + INSERT SELECT + DROP + RENAME pattern. The rebuild preserves
-- all existing rows (which already satisfy the constraints).
--
-- Note: no explicit BEGIN/COMMIT here — the migration runner in
-- db/mod.rs wraps each migration in its own transaction, so this whole
-- rebuild is already atomic. Adding BEGIN/COMMIT would fail with
-- "cannot start a transaction within a transaction."

CREATE TABLE items_new (
  id              TEXT PRIMARY KEY,
  content         TEXT NOT NULL CHECK (length(content) BETWEEN 1 AND 4096),
  tier            TEXT NOT NULL CHECK (tier IN ('inbox','A','B','C')),
  rank            TEXT NOT NULL CHECK (length(rank) >= 1),
  state           TEXT NOT NULL CHECK (state IN ('active','blocked','done')),
  blocked_reason  TEXT,
  start_at        INTEGER,
  due_at          INTEGER,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  deleted         INTEGER NOT NULL DEFAULT 0 CHECK (deleted IN (0, 1)),
  -- SPEC 3.1 guard: blocked requires a non-null reason.
  CHECK (state != 'blocked' OR blocked_reason IS NOT NULL)
);

INSERT INTO items_new (id, content, tier, rank, state, blocked_reason, start_at, due_at, created_at, updated_at, deleted)
SELECT id, content, tier, rank, state, blocked_reason, start_at, due_at, created_at, updated_at, deleted
FROM items;

DROP INDEX idx_items_tier_rank;
DROP TABLE items;
ALTER TABLE items_new RENAME TO items;
CREATE INDEX idx_items_tier_rank ON items(tier, rank) WHERE deleted = 0;
