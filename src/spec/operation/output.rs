use crate::spec::{Id, IdempotencyGuarantee, IdempotencyKeyPropagation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    Response(ResponseOutput),
    Publication(PublicationOutput),
    Request(RequestOutput),
    External(ExternalOutput),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseOutput {
    /// Request input to which this response belongs.
    pub request: Id,

    pub schema: Id,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationOutput {
    pub topic: Id,
    pub schema: Id,

    pub idempotency_key_propagation: Vec<IdempotencyKeyPropagation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestOutput {
    pub target: RequestTarget,

    /// Schema of the outgoing request.
    pub schema: Id,

    /// Whether this invocation may be retried/repeated by the caller.
    pub retry: RetrySemantics,

    pub idempotency_key_propagation: Vec<IdempotencyKeyPropagation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestTarget {
    pub operation: Id,
    pub input: Id,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrySemantics {
    Unspecified,
    Never,
    MayRepeat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalOutput {
    pub name: String,
    pub idempotency: IdempotencyGuarantee,
}
