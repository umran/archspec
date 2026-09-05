use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    Service,
    Schema,

    DataModel,
    DataObject,

    Topic,

    StateMachine,
    State,
    Transition,

    Operation,
    Input,
    Effect,
    EffectIntent,
    TransactionOutput,
    Transaction,
    TransactionRead,

    /// A result binding declared by an effect-executing program step.
    EffectResult,
}

impl fmt::Display for ReferenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Service => "service",
            Self::Schema => "schema",

            Self::DataModel => "data model",
            Self::DataObject => "data object",

            Self::Topic => "topic",

            Self::StateMachine => "state machine",
            Self::State => "state",
            Self::Transition => "transition",

            Self::Operation => "operation",
            Self::Input => "input",
            Self::Effect => "effect",
            Self::EffectIntent => "effect intent",
            Self::TransactionOutput => "transaction output",
            Self::Transaction => "transaction",
            Self::TransactionRead => "transaction read",
            Self::EffectResult => "effect result binding",
        };

        f.write_str(name)
    }
}
