//! SERVICE LAYER: business rules as pure functions.
//! No SQL, no HTTP, no clock – everything they need is passed in, so each
//! rule is unit-tested with plain values (`cargo test`).

pub mod availability;
pub mod bookings;
pub mod maintenance;
pub mod pricing;
pub mod wizard;
