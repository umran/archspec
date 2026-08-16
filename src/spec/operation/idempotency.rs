use serde::{Deserialize, Serialize};

use crate::spec::ValueRef;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdempotencyKey {
    pub components: Vec<ValueRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdempotencyKeyPropagation {
    pub source: IdempotencyKey,
    pub target: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IdempotencyGuarantee {
    Unspecified,

    NotDeduplicated,

    DeduplicatedBy { key: IdempotencyKey },
}
