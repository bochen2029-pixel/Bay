//! Domain types shared across db, commands, and IPC.
//!
//! Mirrors SPEC §4.1 (common types) and §4.3 (event payloads). Matching
//! Zod schemas live in src/domain.ts; keep the two in lockstep.

pub mod capacity;
pub mod event;
pub mod item;
pub mod rank;
pub mod recurrence;
pub mod session;

pub use capacity::{A_CAP, B_CAP};
pub use event::{Actor, Event, EventType, ProjectionEvent};
pub use item::{Item, ItemState, Tier};
pub use rank::rank_between;
pub use recurrence::Recurrence;
pub use session::{Session, SessionOutcome, INTERRUPT_REASONS};
