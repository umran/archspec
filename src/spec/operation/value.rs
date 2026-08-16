use crate::spec::{FieldPath, Id};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueRef {
    pub source: ValueSource,
    pub path: FieldPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSource {
    Input(Id),

    /// A field in the payload of a Publication or Request effect.
    Effect(Id),

    /// A field in a durable invocation result.
    InvocationResult(Id),

    /// A field on the persistent object governed by a state machine.
    StateMachineSubject(Id),
}
