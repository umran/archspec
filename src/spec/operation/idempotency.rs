use crate::spec::ValueRef;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKey {
    pub components: Vec<ValueRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKeyPropagation {
    pub source: IdempotencyKey,
    pub target: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyGuarantee {
    Unspecified,

    NotDeduplicated,

    DeduplicatedBy { key: IdempotencyKey },
}
