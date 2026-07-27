-- 005_execution_core.sql — VISION v0.3 execution core, part 1
-- (ADR-007 dispositions T3 + first_step; laws 4 and 6).
--
--   first_step  TEXT — the single next PHYSICAL action ("open the
--                contract", "dial Marco"), <= 140 chars. Deliberately
--                ONE LINE: an activation-energy handle, not a subtask
--                list (subtasks remain cut — a checklist is a place to
--                hide from work; a first step is a place to start it).
--   today_on    TEXT — the local date ('YYYY-MM-DD') this item is
--                committed to, or NULL. "Today" is an EXECUTION OVERLAY
--                over the tiers, not a fifth tier: items keep their
--                tier; Today marks "chosen for this day", cap 3,
--                decided at day-open, expiring at day-roll (the one
--                sanctioned system-actor write: TODAY_REMOVED
--                cause=expired, executing the human-configured day
--                boundary). ISO date strings compare lexicographically,
--                which is all the roll needs.
--
-- Projection-only change: the sources of truth are ITEM_FIRST_STEP_SET
-- and TODAY_ADDED / TODAY_REMOVED on the event log; DAY_OPENED /
-- DAY_CLOSED are audit events with NULL item_id (the log's first).

ALTER TABLE items ADD COLUMN first_step TEXT CHECK (first_step IS NULL OR length(first_step) BETWEEN 1 AND 140);
ALTER TABLE items ADD COLUMN today_on TEXT;

CREATE INDEX idx_items_today ON items(today_on) WHERE today_on IS NOT NULL;
