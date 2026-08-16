use serde::{Deserialize, Serialize};

use crate::spec::{Id, IdempotencyGuarantee, IdempotencyKeyPropagation};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    Publication(PublicationEffect),
    Request(RequestEffect),
    External(ExternalEffect),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationEffect {
    pub topic: Id,
    pub schema: Id,

    pub idempotency_key_propagation: Vec<IdempotencyKeyPropagation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestEffect {
    pub target: RequestTarget,
    pub schema: Id,
    pub retry: RetrySemantics,

    pub idempotency_key_propagation: Vec<IdempotencyKeyPropagation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestTarget {
    pub operation: Id,
    pub input: Id,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalEffect {
    pub name: String,

    /// This is declared because the modeled system ends here;
    /// the checker cannot inspect the external implementation.
    pub idempotency: IdempotencyGuarantee,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrySemantics {
    Unspecified,
    Never,
    MayRepeat,
}
