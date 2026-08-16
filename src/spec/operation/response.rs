use serde::{Deserialize, Serialize};

use crate::spec::Id;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub request: Id,
    pub schema: Id,
    pub source: ResponseSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseSource {
    /// No replay-stability claim can be derived.
    Unspecified,

    /// The logical response is obtained from an immutable,
    /// durable invocation result.
    InvocationResult { result: Id },
}
