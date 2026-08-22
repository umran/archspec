//! The obligation-report format the visualization consumes.
//!
//! The format and its builders live in the library
//! (`archspec::analyzer::report`): `scaffold` enumerates every
//! declared obligation as `unknown`, and `obligations` fills the same
//! enumeration in from real verification results.

pub use archspec::analyzer::report::*;
