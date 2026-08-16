use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{FieldPath, Id};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Topic {
    /// Schemas that may be published to this topic.
    pub messages: BTreeSet<Id>,

    /// Ordering semantics guaranteed by the topic.
    pub ordering: TopicOrdering,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TopicOrdering {
    /// The model does not provide enough information about ordering.
    Unspecified,

    /// The topic provides no ordering guarantee.
    Unordered,

    /// All messages published to the topic are observed in one
    /// globally ordered sequence.
    Global,

    /// Messages sharing the same logical key are observed in order.
    Keyed(TopicKey),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopicKey {
    /// For each message schema carried by the topic, identifies the
    /// field representing this topic's logical ordering key.
    ///
    /// Different schemas may use different field names while still
    /// participating in the same logical key domain.
    pub mapping: BTreeMap<Id, FieldPath>,
}
