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
    InvocationResult,
    Response,
    Transaction,
    InvocationFlow,
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
            Self::InvocationResult => "invocation result",
            Self::Response => "response",
            Self::Transaction => "transaction",
            Self::InvocationFlow => "invocation flow",
        };

        f.write_str(name)
    }
}
