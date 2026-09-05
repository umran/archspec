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

    /// Where the identity of one logical message lives in the payload.
    pub message_identity: MessageIdentity,
}

/// Identity of one logical message among the topic's carried messages.
///
/// This is an implementation guarantee, deliberately distinct from the
/// ordering key: the ordering key sequences messages, the message
/// identity identifies one logical message. They may coincide, and
/// neither implies the other. It is also distinct from object
/// identity: `order_id` identifies the order, not the message about
/// the order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageIdentity {
    /// No fact relates two carried messages sharing any field values.
    Unspecified,

    /// For each mapped message schema, the ordered fields holding that
    /// schema's message identity. As with the ordering key, different
    /// schemas may map differently named fields into the same identity
    /// domain; tuple positions correspond across schemas, so all
    /// mapped tuples must have the same arity.
    ///
    /// The guarantee is one statement over the mapped population: any
    /// two messages carried by the topic, each of a mapped schema,
    /// whose identity tuples are equal are the same logical message —
    /// hence of the same schema, with equal payloads.
    ///
    /// Two publications sharing an identity are attempts at publishing
    /// one logical message; how often that message is delivered
    /// remains the subscription's delivery semantics. The mapping may
    /// cover a subset of the carried schemas — identity is meaningful
    /// knowledge per schema, unlike the ordering key, which must route
    /// every carried message.
    Keyed {
        mapping: BTreeMap<Id, Vec<FieldPath>>,
    },
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
