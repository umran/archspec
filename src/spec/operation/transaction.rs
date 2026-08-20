use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::spec::{FieldPath, Id};

use super::{Derivation, IdempotencyGuarantee, ValueRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transaction {
    /// None is permitted when the transaction performs no application
    /// DataObject access and only produces or consumes framework
    /// transaction artifacts.
    pub data_model: Option<Id>,

    pub isolation: TransactionIsolation,

    /// Explicit durable keyed commit deduplication provided by the
    /// execution environment.
    ///
    /// This is independent of any invocation-result or effect-intent
    /// declaration. `Unspecified` and `NotDeduplicated` leave the
    /// analyzer free to prove natural replayability from the body.
    pub idempotency: IdempotencyGuarantee,

    pub steps: Vec<TransactionStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionIsolation {
    Unspecified,
    ReadCommitted,
    Snapshot,
    Serializable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransactionStep {
    Read(Read),
    Write(Write),
    Insert(Insert),
    Delete(Delete),
    Lock(Lock),

    Transition(StateTransition),
    EstablishEffectIntent(EstablishEffectIntent),
    EstablishInvocationResult(EstablishInvocationResult),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    /// Transaction-local identity of this observation.
    ///
    /// Later steps in the same transaction may reference the observed
    /// values through `ValueSource::TransactionRead`.
    pub result: Id,

    pub target: ObjectSelector,
    pub fields: FieldSelection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Write {
    pub target: ObjectSelector,
    pub fields: BTreeSet<FieldPath>,

    /// Provenance of the values written.
    pub values: Derivation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Insert {
    pub object: Id,

    /// Provenance of the inserted contents.
    ///
    /// An insert never redeclares object identity: `DataObject.identity`
    /// is already the complete logical identity of every instance.
    pub values: Derivation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    pub target: ObjectSelector,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    pub target: ObjectSelector,
    pub mode: LockMode,
    pub order: LockOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "terms", rename_all = "snake_case")]
pub enum LockOrder {
    Unspecified,
    By(Vec<OrderingTerm>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderingTerm {
    pub field: FieldPath,
    pub direction: OrderingDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderingDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "fields", rename_all = "snake_case")]
pub enum FieldSelection {
    All,
    Only(BTreeSet<FieldPath>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectSelector {
    pub object: Id,
    pub predicate: SelectorPredicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectorPredicate {
    All,

    Eq {
        field: FieldPath,
        value: SelectorValue,
    },

    And {
        predicates: Vec<SelectorPredicate>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SelectorValue {
    Value(ValueRef),
    Literal(Literal),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Literal {
    String(String),
    Bool(bool),
    Int(i64),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateTransition {
    pub machine: Id,
    pub transition: Id,

    /// Selects the concrete persistent machine instance.
    pub subject: ObjectSelector,

    /// Provenance of each side-effect instance implicitly established
    /// by applying the transition, keyed by side effect.
    ///
    /// The keys must exactly match the transition's declared side
    /// effects; a transition without side effects uses an empty map.
    /// The derivations are evaluated in the enclosing transaction
    /// context at this step, so they may reference preceding
    /// transaction reads.
    pub effect_values: BTreeMap<Id, Derivation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstablishEffectIntent {
    pub intent: Id,

    /// Provenance of the intent's logical contents.
    pub values: Derivation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstablishInvocationResult {
    pub result: Id,

    /// Provenance of the result's logical contents.
    pub values: Derivation,
}
