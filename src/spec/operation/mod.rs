pub mod idempotency;
pub mod input;
pub mod output;
pub mod transaction;

pub use idempotency::*;
pub use input::*;
pub use output::*;
pub use transaction::*;

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use super::{FieldPath, Id};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    pub service: Id,
    pub description: Option<String>,

    pub inputs: BTreeMap<Id, Input>,
    pub outputs: BTreeMap<Id, Output>,
    pub transactions: BTreeMap<Id, Transaction>,

    pub requirements: OperationRequirements,
    pub execution: ExecutionSemantics,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OperationRequirements {
    pub serialization: Vec<SerializationRequirement>,
    pub ordering: Vec<OrderingRequirement>,
    pub idempotency: Vec<IdempotencyRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializationRequirement {
    pub key: ValueRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderingRequirement {
    pub key: ValueRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyRequirement {
    pub key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueRef {
    pub source: ValueSource,
    pub path: FieldPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueSource {
    Input(Id),
    Output(Id),
    DataObject(Id),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionSemantics {
    pub concurrency: OperationConcurrency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationConcurrency {
    Unspecified,

    /// Maximum number of simultaneously active invocations
    /// across the logical deployed operation.
    Bounded(NonZeroU32),

    /// No finite global concurrency bound is declared.
    Unbounded,
}
