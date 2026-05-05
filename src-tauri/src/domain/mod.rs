//! Domain types shared across db, commands, and IPC.
//!
//! Mirrors SPEC §4.1 (common types) and §4.3 (event payloads). Matching
//! Zod schemas live in src/domain.ts; keep the two in lockstep.

pub mod capacity;
pub mod event;
pub mod item;
pub mod rank;

pub use capacity::{A_CAP, B_CAP};
pub use event::{Event, EventType};
pub use item::{Item, ItemState, Tier};
pub use rank::rank_between;
