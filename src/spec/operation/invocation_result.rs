use serde::{Deserialize, Serialize};

use crate::spec::Id;

/// A logical transaction artifact shaped by a declared schema.
///
/// An invocation result is semantically separate from transaction
/// idempotency: establishing one does not prevent the enclosing
/// transaction from executing or committing again. Its availability
/// after a retry comes either from deterministic reconstruction by a
/// naturally replayable transaction or from recovery of the artifacts
/// retained by an explicitly keyed transaction commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationResult {
    pub schema: Id,
}
