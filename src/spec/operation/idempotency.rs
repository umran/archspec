use crate::spec::ValueRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKey {
    pub components: Vec<ValueRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKeyPropagation {
    pub source: Vec<ValueRef>,
    pub target: Vec<ValueRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyGuarantee {
    Unspecified,
    NotDeduplicated,

    DeduplicatedBy { key: IdempotencyKey },
}
