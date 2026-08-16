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

    /// Durable-effect-intent declarations available to this operation.
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
