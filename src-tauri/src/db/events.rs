//! The one place events get written — and, since migration 003, the
//! one place the envelope is stamped and the hash chain is computed.
//! Called only from `db::write_events_ctx` inside a caller-provided
//! transaction — never standalone.
//!
//! Envelope v2 (migration 003, ADR-007/ADR-008): every appended row
//! carries `txn_id` (one per write transaction — the boundary undo
//! groups by), `actor` ('human' | 'system'), optional `origin`
//! provenance, `device_id` (from `meta`), `schema_ver`, and
//! `prev_hash` — the SHA-256 of the previous event row, making the
//! append-only log tamper-EVIDENT end to end. Legacy rows (pre-003)
//! carry NULL in all six columns and are tolerated at the head of the
//! chain only.

use rusqlite::{params, OptionalExtension, Transaction};
use sha2::{Digest, Sha256};

use crate::domain::{Actor, EventType};

/// The `prev_hash` of the first event ever written to a log (fresh
/// install): 64 hex zeros, deliberately distinguishable from any real
/// digest.
pub const GENESIS_HASH: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

/// Payload schema version stamped on every event this build writes.
/// Bump per event type when a payload shape changes; upcasters read
/// old versions (VISION §3.0 — a forever-log needs this before its
/// second decade, cheapest now).
pub const ENVELOPE_SCHEMA_VER: i64 = 1;

/// The envelope columns stamped on one appended row. Built by
/// `db::write_events_ctx`; every draft in one call shares `txn_id`,
/// `actor`, `origin`, and `device_id`, while `prev_hash` threads from
/// row to row.
pub struct EnvelopeStamp<'a> {
    pub txn_id: &'a str,
    pub actor: Actor,
    pub origin: Option<&'a str>,
    pub device_id: Option<&'a str>,
    pub schema_ver: i64,
    pub prev_hash: &'a str,
}

/// Append a row to the `events` table. Returns the new `id` (SQLite
/// AUTOINCREMENT). Must run inside an open transaction that the caller
/// also commits via `write_events_ctx`.
///
/// `payload_json` is pre-serialized by the caller: the hash chain must
/// cover the exact bytes stored, so serialization happens once and the
/// same string is inserted and hashed.
pub fn append_event(
    tx: &Transaction<'_>,
    ts: i64,
    event_type: EventType,
    item_id: Option<&str>,
    payload_json: &str,
    stamp: &EnvelopeStamp<'_>,
) -> Result<i64, String> {
    tx.execute(
        "INSERT INTO events (ts, type, item_id, payload, txn_id, actor, origin, device_id, schema_ver, prev_hash) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            ts,
            event_type.as_sql(),
            item_id,
            payload_json,
            stamp.txn_id,
            stamp.actor.as_sql(),
            stamp.origin,
            stamp.device_id,
            stamp.schema_ver,
            stamp.prev_hash
        ],
    )
    .map_err(|e| format!("insert event: {e}"))?;
    Ok(tx.last_insert_rowid())
}

// ── hash chain ──────────────────────────────────────────────────────

/// Length-prefixed, None-tagged field encoding: `0x00` for NULL,
/// `0x01 || len(u64 LE) || bytes` for a value. Unambiguous under
/// concatenation — no separator collisions, no ambiguity between
/// NULL and empty string.
fn hash_field(h: &mut Sha256, v: Option<&[u8]>) {
    match v {
        None => h.update([0u8]),
        Some(b) => {
            h.update([1u8]);
            h.update((b.len() as u64).to_le_bytes());
            h.update(b);
        }
    }
}

/// Canonical SHA-256 over one event row, all eleven columns in schema
/// order. Works for legacy rows too (envelope fields None) so the
/// chain can extend across the pre-003 boundary.
#[allow(clippy::too_many_arguments)]
pub fn event_row_hash(
    id: i64,
    ts: i64,
    type_sql: &str,
    item_id: Option<&str>,
    payload_json: &str,
    txn_id: Option<&str>,
    actor: Option<&str>,
    origin: Option<&str>,
    device_id: Option<&str>,
    schema_ver: Option<i64>,
    prev_hash: Option<&str>,
) -> String {
    let mut h = Sha256::new();
    let id_s = id.to_string();
    let ts_s = ts.to_string();
    let ver_s = schema_ver.map(|v| v.to_string());
    hash_field(&mut h, Some(id_s.as_bytes()));
    hash_field(&mut h, Some(ts_s.as_bytes()));
    hash_field(&mut h, Some(type_sql.as_bytes()));
    hash_field(&mut h, item_id.map(str::as_bytes));
    hash_field(&mut h, Some(payload_json.as_bytes()));
    hash_field(&mut h, txn_id.map(str::as_bytes));
    hash_field(&mut h, actor.map(str::as_bytes));
    hash_field(&mut h, origin.map(str::as_bytes));
    hash_field(&mut h, device_id.map(str::as_bytes));
    hash_field(&mut h, ver_s.as_deref().map(str::as_bytes));
    hash_field(&mut h, prev_hash.map(str::as_bytes));
    let digest = h.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// All eleven raw columns of one event row, in schema order.
type RawRow = (
    i64,            // id
    i64,            // ts
    String,         // type
    Option<String>, // item_id
    String,         // payload
    Option<String>, // txn_id
    Option<String>, // actor
    Option<String>, // origin
    Option<String>, // device_id
    Option<i64>,    // schema_ver
    Option<String>, // prev_hash
);

const RAW_COLS: &str =
    "id, ts, type, item_id, payload, txn_id, actor, origin, device_id, schema_ver, prev_hash";

fn parse_raw_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok((
        r.get(0)?,
        r.get(1)?,
        r.get(2)?,
        r.get(3)?,
        r.get(4)?,
        r.get(5)?,
        r.get(6)?,
        r.get(7)?,
        r.get(8)?,
        r.get(9)?,
        r.get(10)?,
    ))
}

fn hash_raw(r: &RawRow) -> String {
    event_row_hash(
        r.0,
        r.1,
        &r.2,
        r.3.as_deref(),
        &r.4,
        r.5.as_deref(),
        r.6.as_deref(),
        r.7.as_deref(),
        r.8.as_deref(),
        r.9,
        r.10.as_deref(),
    )
}

/// Hash of the current last event row, or None on an empty log (the
/// caller then chains from `GENESIS_HASH`). Read inside the SAME write
/// transaction that will append, so the chain tail cannot race.
pub fn last_event_hash(tx: &Transaction<'_>) -> Result<Option<String>, String> {
    let row: Option<RawRow> = tx
        .query_row(
            &format!("SELECT {RAW_COLS} FROM events ORDER BY id DESC LIMIT 1"),
            [],
            |r| parse_raw_row(r),
        )
        .optional()
        .map_err(|e| format!("read chain tail: {e}"))?;
    Ok(row.map(|r| hash_raw(&r)))
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct ChainReport {
    /// Total event rows walked.
    pub total: i64,
    /// Rows carrying an envelope (prev_hash) that verified against the
    /// recomputed chain.
    pub enveloped: i64,
}

/// Walk the whole log in id order and verify the hash chain:
/// - every enveloped row's `prev_hash` must equal the recomputed hash
///   of the immediately preceding row (or `GENESIS_HASH` for the first
///   row);
/// - un-enveloped (legacy, pre-003) rows are tolerated only BEFORE the
///   first enveloped row — a NULL after the chain has started is a gap.
///
/// O(n) over the log; at solo scale this is milliseconds and runs on a
/// background thread at boot (trust made visible, never a boot block).
pub fn verify_event_chain(conn: &rusqlite::Connection) -> Result<ChainReport, String> {
    let mut stmt = conn
        .prepare(&format!("SELECT {RAW_COLS} FROM events ORDER BY id"))
        .map_err(|e| format!("prepare chain walk: {e}"))?;
    let rows = stmt
        .query_map([], |r| parse_raw_row(r))
        .map_err(|e| format!("query chain walk: {e}"))?;

    let mut prev_computed: Option<String> = None;
    let mut seen_enveloped = false;
    let mut total: i64 = 0;
    let mut enveloped: i64 = 0;
    for row in rows {
        let row = row.map_err(|e| format!("chain row: {e}"))?;
        total += 1;
        match row.10.as_deref() {
            Some(stored_prev) => {
                let expected = prev_computed.as_deref().unwrap_or(GENESIS_HASH);
                if stored_prev != expected {
                    return Err(format!(
                        "CHAIN_BROKEN at event {}: stored prev_hash {} != recomputed {}",
                        row.0, stored_prev, expected
                    ));
                }
                seen_enveloped = true;
                enveloped += 1;
            }
            None => {
                if seen_enveloped {
                    return Err(format!(
                        "CHAIN_GAP at event {}: un-enveloped row after the chain started",
                        row.0
                    ));
                }
            }
        }
        prev_computed = Some(hash_raw(&row));
    }
    Ok(ChainReport { total, enveloped })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Baseline row: every field distinct, every Option populated, so a
    /// field silently dropping out of the digest cannot be masked by a
    /// neighbour that happens to hold the same bytes.
    fn baseline() -> String {
        event_row_hash(
            7,
            1_700_000_000_000,
            "ITEM_MOVED",
            Some("item-7"),
            r#"{"tier_after":"A"}"#,
            Some("txn-7"),
            Some("human"),
            Some("llm_accept:3"),
            Some("dev-7"),
            Some(2),
            Some("aa".repeat(32).as_str()),
        )
    }

    #[test]
    fn every_column_participates_in_the_row_hash() {
        // The chain's whole claim — CLAUDE.md v1.9, ADR-007/008 — is
        // that the append-only log is tamper-EVIDENT. That rests
        // entirely on every column reaching the digest, and NOTHING
        // asserted it: v0.3 pass 9 dropped `payload` (the field that
        // carries all the meaning) out of `event_row_hash` and all 252
        // tests stayed green. Self-consistency tests cannot catch this
        // — write-then-verify agrees with itself no matter how few
        // columns are hashed.
        let base = baseline();
        let perturbed: Vec<(&str, String)> = vec![
            ("id", event_row_hash(8, 1_700_000_000_000, "ITEM_MOVED", Some("item-7"), r#"{"tier_after":"A"}"#, Some("txn-7"), Some("human"), Some("llm_accept:3"), Some("dev-7"), Some(2), Some("aa".repeat(32).as_str()))),
            ("ts", event_row_hash(7, 1_700_000_000_001, "ITEM_MOVED", Some("item-7"), r#"{"tier_after":"A"}"#, Some("txn-7"), Some("human"), Some("llm_accept:3"), Some("dev-7"), Some(2), Some("aa".repeat(32).as_str()))),
            ("type", event_row_hash(7, 1_700_000_000_000, "ITEM_DELETED", Some("item-7"), r#"{"tier_after":"A"}"#, Some("txn-7"), Some("human"), Some("llm_accept:3"), Some("dev-7"), Some(2), Some("aa".repeat(32).as_str()))),
            ("item_id", event_row_hash(7, 1_700_000_000_000, "ITEM_MOVED", Some("item-8"), r#"{"tier_after":"A"}"#, Some("txn-7"), Some("human"), Some("llm_accept:3"), Some("dev-7"), Some(2), Some("aa".repeat(32).as_str()))),
            ("payload", event_row_hash(7, 1_700_000_000_000, "ITEM_MOVED", Some("item-7"), r#"{"tier_after":"C"}"#, Some("txn-7"), Some("human"), Some("llm_accept:3"), Some("dev-7"), Some(2), Some("aa".repeat(32).as_str()))),
            ("txn_id", event_row_hash(7, 1_700_000_000_000, "ITEM_MOVED", Some("item-7"), r#"{"tier_after":"A"}"#, Some("txn-8"), Some("human"), Some("llm_accept:3"), Some("dev-7"), Some(2), Some("aa".repeat(32).as_str()))),
            ("actor", event_row_hash(7, 1_700_000_000_000, "ITEM_MOVED", Some("item-7"), r#"{"tier_after":"A"}"#, Some("txn-7"), Some("system"), Some("llm_accept:3"), Some("dev-7"), Some(2), Some("aa".repeat(32).as_str()))),
            ("origin", event_row_hash(7, 1_700_000_000_000, "ITEM_MOVED", Some("item-7"), r#"{"tier_after":"A"}"#, Some("txn-7"), Some("human"), Some("llm_accept:4"), Some("dev-7"), Some(2), Some("aa".repeat(32).as_str()))),
            ("device_id", event_row_hash(7, 1_700_000_000_000, "ITEM_MOVED", Some("item-7"), r#"{"tier_after":"A"}"#, Some("txn-7"), Some("human"), Some("llm_accept:3"), Some("dev-8"), Some(2), Some("aa".repeat(32).as_str()))),
            ("schema_ver", event_row_hash(7, 1_700_000_000_000, "ITEM_MOVED", Some("item-7"), r#"{"tier_after":"A"}"#, Some("txn-7"), Some("human"), Some("llm_accept:3"), Some("dev-7"), Some(3), Some("aa".repeat(32).as_str()))),
            ("prev_hash", event_row_hash(7, 1_700_000_000_000, "ITEM_MOVED", Some("item-7"), r#"{"tier_after":"A"}"#, Some("txn-7"), Some("human"), Some("llm_accept:3"), Some("dev-7"), Some(2), Some("bb".repeat(32).as_str()))),
        ];
        assert_eq!(perturbed.len(), 11, "all eleven columns must be covered");
        for (field, h) in &perturbed {
            assert_ne!(
                &base, h,
                "changing `{field}` left the row hash unchanged — that column is not \
                 in the digest, and forging it would be undetectable"
            );
        }
        // And every perturbation is distinct from every other, so no two
        // columns are being folded together.
        for (i, (fa, ha)) in perturbed.iter().enumerate() {
            for (fb, hb) in perturbed.iter().skip(i + 1) {
                assert_ne!(ha, hb, "`{fa}` and `{fb}` collide in the digest");
            }
        }
    }

    #[test]
    fn null_is_distinguishable_from_empty_string() {
        // The encoding's stated property. Without the None tag, a NULL
        // `origin` and an empty-string `origin` would hash identically,
        // so provenance could be erased rather than altered.
        let with_none = event_row_hash(
            1, 1, "ITEM_CREATED", None, "{}", None, None, None, None, None, None,
        );
        let with_empty = event_row_hash(
            1, 1, "ITEM_CREATED", Some(""), "{}", Some(""), Some(""), Some(""), Some(""), None,
            Some(""),
        );
        assert_ne!(with_none, with_empty, "NULL and empty string must not collide");
    }

    #[test]
    fn field_boundaries_are_unambiguous_under_concatenation() {
        // The length prefix's reason for existing: without it, adjacent
        // fields could be re-partitioned to produce the same digest —
        // ("ab","c") and ("a","bc"). Currently no field can carry the
        // bytes needed to exploit that through the app's own write path,
        // so this is defence-in-depth rather than a live hole; it is
        // asserted anyway because the encoding's doc comment claims it.
        let left = event_row_hash(
            1, 1, "AB", Some("C"), "{}", None, None, None, None, None, None,
        );
        let right = event_row_hash(
            1, 1, "A", Some("BC"), "{}", None, None, None, None, None, None,
        );
        assert_ne!(left, right, "field boundaries must survive concatenation");
    }
}
