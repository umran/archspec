use serde::{Deserialize, Serialize};

use crate::spec::{Id, IdempotencyKey};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationResult {
    pub key: IdempotencyKey,
    pub schema: Id,
}
