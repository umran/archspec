use crate::spec::Id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub request: Id,
    pub schema: Id,
    pub source: ResponseSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseSource {
    /// No replay-stability claim can be derived.
    Unspecified,

    /// The logical response is obtained from an immutable,
    /// durable invocation result.
    InvocationResult { result: Id },
}
