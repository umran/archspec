//! The obligation-report format the visualization consumes.
//!
//! The format and its builders live in the library
//! (`conseqa::analyzer::report`): `scaffold` enumerates every
//! declared obligation as `unknown`, and `obligations` fills the same
//! enumeration in from real verification results.

pub use conseqa::analyzer::report::*;
