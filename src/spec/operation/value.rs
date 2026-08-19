use serde::{Deserialize, Serialize};

use crate::spec::{FieldPath, Id};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueRef {
    pub source: ValueSource,
    pub path: FieldPath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ValueSource {
    Input(Id),

    /// A field in the payload of a Publication or Request effect.
    Effect(Id),

    /// A field in a logical invocation result available to the
    /// current invocation.
    InvocationResult(Id),

    /// A field on the persistent object governed by a state machine.
    StateMachineSubject(Id),

    /// A field observed by a named Read earlier in the same
    /// transaction execution.
    ///
    /// Transaction-read results are transaction-local. They do not
    /// become durable cross-transaction artifacts.
    TransactionRead(Id),
}

/// Provenance of an opaque value computation.
///
/// A derivation describes how values are produced. It is deliberately
/// not an expression language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Derivation {
    /// The model provides no fact about how the values are produced.
    Unspecified,

    /// The produced values are a deterministic function solely of the
    /// declared source values.
    ///
    /// This does not assert that those sources are stable across
    /// retries; replay stability of the provenance roots is
    /// established separately.
    Deterministic { from: Vec<ValueRef> },
}
