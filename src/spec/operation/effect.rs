use serde::{Deserialize, Serialize};

use crate::spec::{Id, IdempotencyGuarantee, IdempotencyKeyPropagation, ResultType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Effect {
    Publication(PublicationEffect),
    Request(RequestEffect),
    External(ExternalEffect),
}

/// Publishes one schema to one topic. A publication has no synchronous
/// result and cannot bind one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationEffect {
    pub topic: Id,
    pub schema: Id,

    pub idempotency_key_propagation: Vec<IdempotencyKeyPropagation>,
}

/// Invokes a specific request input of another operation.
///
/// Its synchronous result contract is inherited from the targeted
/// input's declared `result`; it is never redeclared here.
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

    /// The synchronous result the boundary returns, declared for the
    /// same reason: Archspec cannot inspect beyond it. `None` means no
    /// synchronous result is modeled.
    ///
    /// Declaring the contract says what the returned outcome is shaped
    /// like. It says nothing about whether repeated executions return
    /// the same outcome; no declared fact establishes that, so an
    /// external result is never replay-stable in V1.
    pub result: Option<ResultType>,
}

impl Effect {
    /// Every value reference the effect's declaration evaluates when
    /// the effect executes: an external deduplication key, and the
    /// source and target of each propagation.
    pub fn roots(&self) -> Vec<&crate::spec::ValueRef> {
        let mut roots = Vec::new();

        let propagations = match self {
            Self::Publication(effect) => &effect.idempotency_key_propagation,
            Self::Request(effect) => &effect.idempotency_key_propagation,
            Self::External(effect) => {
                if let IdempotencyGuarantee::DeduplicatedBy { key } = &effect.idempotency {
                    roots.extend(key.components.iter());
                }

                return roots;
            }
        };

        for propagation in propagations {
            roots.extend(propagation.source.components.iter());
            roots.extend(propagation.target.components.iter());
        }

        roots
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrySemantics {
    Unspecified,
    Never,
    MayRepeat,
}
