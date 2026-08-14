use std::collections::BTreeSet;

use crate::spec::{FieldPath, Id};

use super::ValueRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub data_model: Id,
    pub isolation: TransactionIsolation,
    pub steps: Vec<TransactionStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionIsolation {
    Unspecified,
    ReadCommitted,
    Snapshot,
    Serializable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStep {
    Read(Read),
    Write(Write),
    Insert(Insert),
    Delete(Delete),
    Lock(Lock),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    pub target: ObjectSelector,
    pub fields: FieldSelection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Write {
    pub target: ObjectSelector,
    pub fields: BTreeSet<FieldPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insert {
    pub object: Id,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delete {
    pub target: ObjectSelector,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lock {
    pub target: ObjectSelector,
    pub mode: LockMode,
    pub order: LockOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockOrder {
    Unspecified,
    By(Vec<OrderingTerm>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderingTerm {
    pub field: FieldPath,
    pub direction: OrderingDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderingDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldSelection {
    All,
    Only(BTreeSet<FieldPath>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSelector {
    pub object: Id,
    pub predicate: SelectorPredicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorPredicate {
    All,

    Eq {
        field: FieldPath,
        value: SelectorValue,
    },

    And(Vec<SelectorPredicate>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectorValue {
    Input(ValueRef),
    Literal(Literal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    String(String),
    Bool(bool),
    Int(i64),
}
