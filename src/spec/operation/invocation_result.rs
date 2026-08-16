use crate::spec::{Id, IdempotencyKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationResult {
    pub key: IdempotencyKey,
    pub schema: Id,
}
