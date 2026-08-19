pub mod effect;
pub mod effect_intent;
pub mod flow;
pub mod idempotency;
pub mod input;
pub mod invocation_result;
pub mod response;
pub mod state_machine;
pub mod transaction;
pub mod value;

pub use effect::*;
pub use effect_intent::*;
pub use flow::*;
pub use idempotency::*;
pub use input::*;
pub use invocation_result::*;
pub use response::*;
use serde::{Deserialize, Serialize};
pub use state_machine::*;
pub use transaction::*;
pub use value::*;

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use super::Id;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Operation {
    pub service: Id,
    pub description: Option<String>,

    pub inputs: BTreeMap<Id, Input>,

    /// Logical effects available to this operation.
    /// Declaration alone does not imply execution.
    pub effects: BTreeMap<Id, Effect>,

    /// Logical effect-intent artifacts available to this operation.
    ///
    /// An intent naming a transition-owned effect is the stable
    /// identity of the intent that transition implicitly establishes.
    pub effect_intents: BTreeMap<Id, EffectIntent>,

    pub invocation_results: BTreeMap<Id, InvocationResult>,
    pub responses: BTreeMap<Id, Response>,

    /// Atomic units reusable by invocation flows.
    pub transactions: BTreeMap<Id, Transaction>,

    /// Alternative valid terminal invocation paths.
    pub flows: BTreeMap<Id, InvocationFlow>,

    pub requirements: OperationRequirements,
    pub execution: ExecutionSemantics,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRequirements {
    pub serialization: Vec<SerializationRequirement>,
    pub ordering: Vec<OrderingRequirement>,
    pub idempotency: Vec<IdempotencyRequirement>,
    pub recoverability: Vec<RecoverabilityRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerializationRequirement {
    pub key: ValueRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderingRequirement {
    pub key: ValueRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdempotencyRequirement {
    pub key: IdempotencyKey,
    pub response: ResponseReplayRequirement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseReplayRequirement {
    Unspecified,
    ReplayConsistent,
}

/// An obligation that a logical invocation reaches terminal execution
/// of a declared flow.
///
/// This is a progress obligation and is deliberately separate from
/// `IdempotencyRequirement`, which is a safety obligation. Idempotency
/// constrains what repeated attempts may do; it is satisfied vacuously
/// by never retrying, and therefore says nothing about whether the
/// remaining steps of an interrupted flow ever execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverabilityRequirement {
    /// Identity of the logical invocation that must reach terminal
    /// execution.
    ///
    /// Attempts sharing this key are attempts at the same logical
    /// invocation, so re-driving one of them continues that invocation
    /// rather than starting a new one.
    pub key: IdempotencyKey,

    /// How strongly completion must be established.
    pub completion: CompletionRequirement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionRequirement {
    /// An interrupted attempt must be able to resume and drive a
    /// declared flow to its terminal step.
    ///
    /// The solver must establish that every prefix at which the
    /// invocation may fail admits a continuation: already-committed
    /// transactions resolve on re-encounter, and every artifact a
    /// later step consumes is replay-available.
    ///
    /// This does not oblige the architecture to actually re-drive the
    /// invocation.
    Resumable,

    /// In addition to resumability, the architecture must guarantee
    /// that the logical invocation is re-driven until a declared flow
    /// terminates.
    ///
    /// This is a liveness obligation and additionally requires a
    /// modeled retry driver, such as at-least-once delivery on the
    /// triggering subscription or an inbound request effect that may
    /// repeat.
    Guaranteed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionSemantics {
    pub concurrency: OperationConcurrency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum OperationConcurrency {
    Unspecified,

    /// Maximum number of simultaneously active invocations
    /// across the logical deployed operation.
    Bounded(NonZeroU32),

    /// No finite global concurrency bound is declared.
    Unbounded,
}
