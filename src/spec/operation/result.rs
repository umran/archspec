use serde::{Deserialize, Serialize};

use crate::spec::Id;

/// A first-class `Result<Ok, Err>` contract: a tagged sum holding
/// exactly one of an `Ok` payload shaped by `ok` or an `Err` payload
/// shaped by `err`, where both name schemas.
///
/// Mutual exclusivity is structural. Archspec models the algebraic
/// outcome, not any language's API around it.
///
/// `Err` is a *logical* returned outcome — a synchronous interaction
/// completed and reported a modeled failure, such as a declined card.
/// It is not an interrupted execution: a crash, a timeout, or a lost
/// connection is an idempotency and recoverability question, not an
/// `Err` payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultType {
    pub ok: Id,
    pub err: Id,
}

impl ResultType {
    /// The schema of one variant's payload.
    pub fn schema(&self, variant: ResultVariant) -> &Id {
        match variant {
            ResultVariant::Ok => &self.ok,
            ResultVariant::Err => &self.err,
        }
    }
}

/// Which arm of a `Result` an outcome, a match arm, or a value source
/// refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultVariant {
    Ok,
    Err,
}

impl std::fmt::Display for ResultVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Ok => "ok",
            Self::Err => "err",
        })
    }
}
