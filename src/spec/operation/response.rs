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

    /// The response is obtained from the logical invocation result
    /// available to the current invocation.
    ///
    /// This source does not by itself imply durable memoization;
    /// replay consistency must be proven from the establishing
    /// transaction's replay semantics.
    InvocationResult { result: Id },
}
