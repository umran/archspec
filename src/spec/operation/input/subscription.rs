use std::collections::BTreeSet;
use std::num::NonZeroU32;

use crate::spec::Id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionInput {
    /// Topic from which this subscription receives messages.
    pub topic: Id,

    /// Message schemas from the topic that may invoke this operation.
    pub messages: MessageSelector,

    /// Delivery semantics of the logical subscription.
    pub delivery: DeliverySemantics,

    /// How deliveries are mapped onto concurrent operation execution.
    pub dispatch: DispatchSemantics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageSelector {
    /// Consume every message schema declared by the topic.
    All,

    /// Consume only these message schemas.
    Only(BTreeSet<Id>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverySemantics {
    /// The specification does not provide enough information
    /// to determine duplicate/loss behaviour.
    Unspecified,

    /// A message is delivered no more than once.
    ///
    /// Loss may be possible, but redelivery of the same logical
    /// message is not.
    AtMostOnce,

    /// A successfully published logical message may be delivered
    /// more than once.
    AtLeastOnce,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchSemantics {
    /// How deliveries are grouped into logical execution lanes.
    pub routing: DispatchRouting,

    /// Maximum concurrency within each logical lane.
    pub lane_concurrency: LaneConcurrency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchRouting {
    /// The mapping from deliveries to execution lanes is unknown.
    Unspecified,

    /// No useful affinity between related deliveries and lanes
    /// is guaranteed.
    Unconstrained,

    /// Every delivery goes through the same logical lane.
    SingleLane,

    /// Deliveries sharing the topic's logical ordering key are
    /// guaranteed to enter the same logical lane.
    ByTopicKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneConcurrency {
    /// No concurrency fact has been declared.
    Unspecified,

    /// At most this many deliveries from the same logical lane
    /// may have active operation invocations simultaneously.
    Bounded(NonZeroU32),

    /// No finite per-lane concurrency bound is declared.
    Unbounded,
}
