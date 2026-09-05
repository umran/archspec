use serde::{Deserialize, Serialize};

use crate::spec::Id;

/// A typed logical value a transaction deliberately exports into the
/// enclosing operation's control.
///
/// A transaction output represents data: a reservation id, a routing
/// decision, a normalized version of the input. It is a framework
/// transaction artifact established atomically with its transaction's
/// commit, and it is the only way information observed or computed
/// inside a transaction reaches later operation control — a
/// transaction read stays transaction-local.
///
/// It is not an operation result, not a success or failure, not an
/// effect, and not inherently durable. Its availability to a retry
/// follows the artifact replay rules: reconstructed by a naturally
/// replayable transaction with a replay-deterministic derivation, or
/// recovered exactly from a keyed commit. An `EffectIntent` is the
/// other principal transaction artifact and represents pending work;
/// the two are not interchangeable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionOutput {
    pub schema: Id,
}
