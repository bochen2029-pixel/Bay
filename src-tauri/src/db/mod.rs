//! SQLite connection pool and linear migration runner.
//!
//! Tracks schema version via SQLite's built-in `PRAGMA user_version`.
//! Numbered SQL files embedded at compile time; applied in order, in a
//! transaction per migration. No metadata table, no rollback, no
//! migration graph — a solo project wants boring and forward-only.

use std::path::Path;

use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;

pub type SqlitePool = Pool<SqliteConnectionManager>;
#[allow(dead_code)] // used from I-03 onward
pub type SqliteConn = PooledConnection<SqliteConnectionManager>;

/// Migration SQL embedded at compile time so the binary is self-contained.
/// First tuple element is the target `user_version` after the SQL applies.
const MIGRATIONS: &[(i32, &str)] = &[
    (1, include_str!("../../../migrations/001_initial.sql")),
];

pub fn open_pool(db_path: &Path) -> Result<SqlitePool, String> {
    let manager = SqliteConnectionManager::file(db_path).with_init(|c| {
        c.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
    });
    Pool::builder()
        .max_size(8)
        .build(manager)
        .map_err(|e| format!("sqlite pool init failed: {e}"))
}

pub fn run_migrations(pool: &SqlitePool) -> Result<(), String> {
    let mut conn = pool.get().map_err(|e| format!("pool get: {e}"))?;
    let current: i32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| format!("read user_version: {e}"))?;

    for (target, sql) in MIGRATIONS {
        if current >= *target {
            continue;
        }
        let tx = conn
            .transaction()
            .map_err(|e| format!("begin migration tx: {e}"))?;
        tx.execute_batch(sql)
            .map_err(|e| format!("apply migration {target}: {e}"))?;
        // PRAGMA does not accept bound parameters. `target` is a compile-time
        // const, not user input, so string interpolation is safe here.
        tx.execute_batch(&format!("PRAGMA user_version = {target};"))
            .map_err(|e| format!("bump user_version to {target}: {e}"))?;
        tx.commit()
            .map_err(|e| format!("commit migration {target}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_pool() -> SqlitePool {
        let manager = SqliteConnectionManager::memory();
        Pool::builder().max_size(1).build(manager).unwrap()
    }

    #[test]
    fn migrations_bring_fresh_db_to_target_version() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        let v: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);
    }

    #[test]
    fn migrations_are_idempotent() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        run_migrations(&pool).unwrap(); // second call is a no-op
        let conn = pool.get().unwrap();
        let v: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);
    }

    #[test]
    fn schema_has_expected_tables_and_indexes() {
        let pool = mem_pool();
        run_migrations(&pool).unwrap();
        let conn = pool.get().unwrap();
        let names: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type IN ('table','index') AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            names,
            vec![
                "events".to_string(),
                "idx_events_item".to_string(),
                "idx_events_ts".to_string(),
                "idx_items_tier_rank".to_string(),
                "items".to_string(),
            ]
        );
    }
}
