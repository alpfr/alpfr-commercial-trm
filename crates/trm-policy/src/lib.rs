//! Policy core for the Commercial TRM.
//!
//! * [`Version`]: lenient parsing and ordering of real-world version strings.
//! * [`Range`]: the range grammar stored in `decision.version_range`.
//! * [`evaluate`]: picks the decision that governs a version in a scope and
//!   turns it into a verdict for enforcement points.
//!
//! The crate does no I/O. Callers load a tenant's current decisions for one
//! technology and pass them in. See `docs/VERSION_RANGES.md` for the rules.

mod decision;
mod range;
mod version;

pub use decision::{evaluate, Decision, Outcome, Query, Status, Target, Verdict};
pub use range::{Range, RangeError};
pub use version::{Version, VersionError};
