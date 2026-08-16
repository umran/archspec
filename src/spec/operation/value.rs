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

    /// A field in a durable invocation result.
    InvocationResult(Id),

    /// A field on the persistent object governed by a state machine.
    StateMachineSubject(Id),
}
