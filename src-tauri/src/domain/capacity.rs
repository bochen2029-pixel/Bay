//! Tier capacity constants. Per CLAUDE.md §Design philosophy #1, caps
//! apply only to items with `state == active`; blocked and done items
//! never count toward capacity. C and Inbox are unbounded.

/// Maximum active items permitted in tier A.
pub const A_CAP: usize = 5;

/// Maximum active items permitted in tier B.
pub const B_CAP: usize = 12;
