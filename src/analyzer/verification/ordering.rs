//! Verification of operation ordering requirements (§9 of the
//! semantics contract).
//!
//! > Same-key invocations for which a meaningful logical precedence
//! > exists must preserve that precedence through the operation's
//! > semantically relevant execution.
//!
//! A proof must say where the precedence comes from and show that the
//! execution mechanism preserves it, including whatever serialization
//! stops a later invocation overtaking an earlier one (§9). V1
//! recognizes one precedence source: the order the key's subscription
//! topic declares (§6). A keyed topic orders same-key messages, which
//! is a precedence *for the ordering key* only when that key is
//! established to carry the topic key for every admitted schema — the
//! key identity the serialization verifier already computes; a global
//! topic orders every message, so any key inherits it.
//!
//! The mechanism is the §8.2 composition: same-key deliveries enter
//! one lane (`by_topic_key`, or `single_lane` for every delivery), a
//! lane dispatches in the order deliveries entered it and does not
//! advance past an incomplete delivery — a failed attempt is
//! re-dispatched at the head of the lane — and lane concurrency one
//! stops overtaking within it. Redelivery therefore cannot invert
//! the precedence: a failure-driven redelivery precedes every later
//! message of the lane, and a duplicate of an already completed
//! message is a repeated attempt at a logical invocation that took
//! effect in order, whose work is the idempotency requirement's
//! obligation rather than ordering's. The proof records which
//! requirement covers it, or that none does.
//!
//! Request inputs carry no ordering fact in the DSL (arrival order of
//! unmodeled callers is not a logical precedence), and a key sourced
//! from anything but an input selects no population; both are
//! unproven, never violated.

use serde::{Deserialize, Serialize};

use crate::analyzer::{Diagnostic, DiagnosticCode, Evidence, Severity, VerificationCode};
use crate::spec::{
    DeliverySemantics, DispatchRouting, FieldPath, Id, Input, LaneConcurrency, Model, Operation,
    OrderingRequirement, TopicOrdering, ValueRef, ValueSource,
};

use super::describe::describe_value_ref;
use super::idempotency::{IdempotencyCheck, IdempotencyVerdict};
use super::serialization::{
    MessageKeyFact, SerializationObstacle, admits_no_messages, is_serial_lane, keyed_lane_facts,
};

/// The verdict for one declared ordering requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderingCheck {
    pub operation: Id,

    /// Index into `operation.requirements.ordering`.
    pub requirement: usize,

    /// The requirement's key, copied so the check is self-contained.
    pub key: ValueRef,

    pub verdict: OrderingVerdict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrderingVerdict {
    Proven { proof: OrderingProof },
    Unproven { obstacles: Vec<OrderingObstacle> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrderingProof {
    /// The key's subscription input admits no message schemas, so no
    /// invocation bears the key and no precedence exists to preserve.
    NoAdmittedInvocations { input: Id },

    /// The topic's declared order is the precedence; one lane at
    /// concurrency one preserves it; duplicates cannot reorder it.
    LaneOrder {
        input: Id,
        topic: Id,
        precedence: PrecedenceSource,
        lane: LaneFact,
        duplicates: DuplicateHandling,
    },
}

/// Where the preserved precedence comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrecedenceSource {
    /// The topic orders same-key messages, and the ordering key is
    /// established to carry the topic key for every admitted schema.
    KeyedTopic { message_keys: Vec<MessageKeyFact> },

    /// The topic orders every message, so same-key messages are
    /// ordered whatever the key.
    GlobalTopic,
}

/// The lane fact that keeps same-key deliveries in one dispatch order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LaneFact {
    /// Same-key deliveries enter one lane through the topic's key
    /// domain.
    ByTopicKey,

    /// Every delivery of the subscription enters one lane.
    SingleLane,
}

/// Why redelivery cannot invert the precedence, and who answers for
/// the work a duplicate does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DuplicateHandling {
    /// `at_most_once` delivery: a logical message is never delivered
    /// again, so neither redelivery nor a duplicate exists.
    SingleDelivery,

    /// A failed delivery is re-dispatched at the head of its lane
    /// (§8.2), so it precedes every later message; a duplicate of a
    /// completed delivery is a repeated attempt at an invocation that
    /// already took effect in order, and what it does is the
    /// idempotency requirement's obligation — `idempotency` names the
    /// requirement keyed from this input when one is declared.
    HeadOfLineRetry {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        idempotency: Option<DuplicateCoverage>,
    },
}

/// The idempotency requirement that answers for duplicate attempts
/// through an input, and its verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateCoverage {
    pub requirement: usize,
    pub proven: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrderingObstacle {
    /// The key is not sourced from an input declared by the
    /// operation, so no dispatch fact selects which invocations share
    /// it.
    KeyNotFromInput { source: ValueSource },

    /// Key-bearing invocations arrive through a request input, and
    /// the DSL declares no precedence fact for requests.
    RequestInputHasNoPrecedenceSource { input: Id },

    /// The subscribed topic declares no order that could serve as the
    /// precedence.
    TopicOrderingProvidesNoPrecedence {
        input: Id,
        topic: Id,
        declared: TopicOrdering,
    },

    /// The keyed topic declares no ordering-key mapping for an
    /// admitted schema.
    TopicKeyMappingMissing { input: Id, topic: Id, schema: Id },

    /// The topic's order is per topic key, and the ordering key is
    /// not established to carry it for this schema, so the declared
    /// order says nothing about same-key invocations.
    KeyIdentityUnestablished {
        input: Id,
        topic: Id,
        schema: Id,
        topic_key: FieldPath,
    },

    /// Routing is `by_topic_key`, but the topic's order is global and
    /// declares no key domain to route by.
    ByTopicKeyWithoutKeyDomain { input: Id, topic: Id },

    /// The subscription's routing provides no lane affinity, so
    /// same-key deliveries may be dispatched out of order.
    RoutingDoesNotPreserveOrder {
        input: Id,
        declared: DispatchRouting,
    },

    /// The declared per-lane concurrency admits overlap, so a later
    /// invocation may overtake an earlier one.
    LaneConcurrencyNotSerial {
        input: Id,
        declared: LaneConcurrency,
    },
}

/// Checks every ordering requirement declared by the model. The
/// idempotency verdicts are read only to record which requirement
/// answers for duplicate attempts; no ordering verdict depends on
/// them.
pub fn check(model: &Model, idempotency: &[IdempotencyCheck]) -> Vec<OrderingCheck> {
    let mut checks = Vec::new();

    for (operation_id, operation) in &model.operations {
        for (index, requirement) in operation.requirements.ordering.iter().enumerate() {
            checks.push(OrderingCheck {
                operation: operation_id.clone(),
                requirement: index,
                key: requirement.key.clone(),
                verdict: check_requirement(
                    model,
                    operation_id,
                    operation,
                    requirement,
                    idempotency,
                ),
            });
        }
    }

    checks
}

fn check_requirement(
    model: &Model,
    operation_id: &Id,
    operation: &Operation,
    requirement: &OrderingRequirement,
    idempotency: &[IdempotencyCheck],
) -> OrderingVerdict {
    let ValueSource::Input(input_id) = &requirement.key.source else {
        return OrderingVerdict::Unproven {
            obstacles: vec![OrderingObstacle::KeyNotFromInput {
                source: requirement.key.source.clone(),
            }],
        };
    };

    let subscription = match operation.inputs.get(input_id) {
        Some(Input::Subscription(subscription)) => subscription,

        Some(Input::Request(_)) => {
            return OrderingVerdict::Unproven {
                obstacles: vec![OrderingObstacle::RequestInputHasNoPrecedenceSource {
                    input: input_id.clone(),
                }],
            };
        }

        None => {
            return OrderingVerdict::Unproven {
                obstacles: vec![OrderingObstacle::KeyNotFromInput {
                    source: requirement.key.source.clone(),
                }],
            };
        }
    };

    if admits_no_messages(model, subscription) {
        return OrderingVerdict::Proven {
            proof: OrderingProof::NoAdmittedInvocations {
                input: input_id.clone(),
            },
        };
    }

    let topic_id = subscription.topic.clone();
    let mut obstacles = Vec::new();

    // The precedence source: the topic's declared order, for this key.
    let ordering = model
        .topics
        .get(&topic_id)
        .map(|topic| topic.ordering.clone())
        .unwrap_or(TopicOrdering::Unspecified);

    let precedence = match &ordering {
        TopicOrdering::Keyed(_) => {
            match keyed_lane_facts(model, input_id, subscription, &requirement.key.path) {
                Ok((_, message_keys)) => Some(PrecedenceSource::KeyedTopic { message_keys }),

                Err(serialization_obstacles) => {
                    for obstacle in serialization_obstacles {
                        obstacles.push(match obstacle {
                            SerializationObstacle::TopicKeyMappingMissing {
                                input,
                                topic,
                                schema,
                            } => OrderingObstacle::TopicKeyMappingMissing {
                                input,
                                topic,
                                schema,
                            },

                            SerializationObstacle::KeyIdentityUnestablished {
                                input,
                                topic,
                                schema,
                                topic_key,
                            } => OrderingObstacle::KeyIdentityUnestablished {
                                input,
                                topic,
                                schema,
                                topic_key,
                            },

                            _ => OrderingObstacle::TopicOrderingProvidesNoPrecedence {
                                input: input_id.clone(),
                                topic: topic_id.clone(),
                                declared: ordering.clone(),
                            },
                        });
                    }

                    None
                }
            }
        }

        TopicOrdering::Global => Some(PrecedenceSource::GlobalTopic),

        TopicOrdering::Unspecified | TopicOrdering::Unordered => {
            obstacles.push(OrderingObstacle::TopicOrderingProvidesNoPrecedence {
                input: input_id.clone(),
                topic: topic_id.clone(),
                declared: ordering.clone(),
            });

            None
        }
    };

    // The mechanism: one lane, dispatching in delivery order.
    let lane = match subscription.dispatch.routing {
        DispatchRouting::SingleLane => Some(LaneFact::SingleLane),

        DispatchRouting::ByTopicKey => match ordering {
            TopicOrdering::Keyed(_) => Some(LaneFact::ByTopicKey),

            // A global order declares no key domain; the routing fact
            // is meaningless without one (§8.2), and the order it
            // would need is already absent or already global.
            TopicOrdering::Global => {
                obstacles.push(OrderingObstacle::ByTopicKeyWithoutKeyDomain {
                    input: input_id.clone(),
                    topic: topic_id.clone(),
                });

                None
            }

            TopicOrdering::Unspecified | TopicOrdering::Unordered => None,
        },

        declared @ (DispatchRouting::Unspecified | DispatchRouting::Unconstrained) => {
            obstacles.push(OrderingObstacle::RoutingDoesNotPreserveOrder {
                input: input_id.clone(),
                declared,
            });

            None
        }
    };

    if !is_serial_lane(subscription.dispatch.lane_concurrency) {
        obstacles.push(OrderingObstacle::LaneConcurrencyNotSerial {
            input: input_id.clone(),
            declared: subscription.dispatch.lane_concurrency,
        });
    }

    // Redelivery: a failed delivery retries at the head of its lane,
    // and a duplicate of a completed one is idempotency's concern. The
    // proof records which requirement answers for it.
    let duplicates = match subscription.delivery {
        DeliverySemantics::AtMostOnce => DuplicateHandling::SingleDelivery,

        DeliverySemantics::AtLeastOnce | DeliverySemantics::Unspecified => {
            let coverage = idempotency
                .iter()
                .find(|check| {
                    &check.operation == operation_id
                        && !check.key.components.is_empty()
                        && check.key.components.iter().all(|component| {
                            component.source == ValueSource::Input(input_id.clone())
                        })
                })
                .map(|check| DuplicateCoverage {
                    requirement: check.requirement,
                    proven: matches!(check.verdict, IdempotencyVerdict::Proven { .. }),
                });

            DuplicateHandling::HeadOfLineRetry {
                idempotency: coverage,
            }
        }
    };

    match (precedence, lane) {
        (Some(precedence), Some(lane)) if obstacles.is_empty() => OrderingVerdict::Proven {
            proof: OrderingProof::LaneOrder {
                input: input_id.clone(),
                topic: topic_id,
                precedence,
                lane,
                duplicates,
            },
        },

        _ => OrderingVerdict::Unproven { obstacles },
    }
}

impl OrderingCheck {
    /// The diagnostic for an unproven requirement; a proven one
    /// produces none.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        let OrderingVerdict::Unproven { obstacles } = &self.verdict else {
            return None;
        };

        let operation = &self.operation;
        let requirement = self.requirement;
        let key = describe_value_ref(&self.key);

        Some(Diagnostic {
            code: DiagnosticCode::Verification(VerificationCode::OrderingUnproven),
            severity: Severity::Unknown,
            subject: Some(operation.clone()),
            message: format!(
                "Ordering requirement {requirement} of `{operation}` is not \
                 established: no declared facts prove that invocations sharing \
                 {key} take effect in their logical precedence."
            ),
            evidence: obstacles
                .iter()
                .map(|obstacle| obstacle.evidence(self))
                .collect(),
        })
    }
}

impl OrderingObstacle {
    fn evidence(&self, check: &OrderingCheck) -> Evidence {
        match self {
            Self::KeyNotFromInput { source } => Evidence {
                subject: Some(check.operation.clone()),
                message: format!(
                    "The ordering key is sourced from {}, not from an input of the \
                     operation, so no dispatch fact selects which invocations \
                     share it.",
                    super::describe::describe_value_source(source)
                ),
            },

            Self::RequestInputHasNoPrecedenceSource { input } => Evidence {
                subject: Some(input.clone()),
                message: format!(
                    "Key-bearing invocations arrive through request input \
                     `{input}`; the DSL declares no precedence among requests, \
                     so there is no logical order to preserve."
                ),
            },

            Self::TopicOrderingProvidesNoPrecedence {
                input,
                topic,
                declared,
            } => Evidence {
                subject: Some(topic.clone()),
                message: match declared {
                    TopicOrdering::Unordered => format!(
                        "`{topic}`, subscribed by `{input}`, is explicitly \
                         `unordered`: it provides no message order to serve as \
                         the precedence."
                    ),

                    _ => format!(
                        "`{topic}`, subscribed by `{input}`, declares no usable \
                         ordering fact to serve as the precedence."
                    ),
                },
            },

            Self::TopicKeyMappingMissing {
                input,
                topic,
                schema,
            } => Evidence {
                subject: Some(topic.clone()),
                message: format!(
                    "`{topic}` orders messages by key but declares no key \
                     mapping for `{schema}`, which `{input}` admits."
                ),
            },

            Self::KeyIdentityUnestablished {
                input,
                topic,
                schema,
                topic_key,
            } => Evidence {
                subject: Some(schema.clone()),
                message: format!(
                    "`{topic}` orders `{schema}` by `{topic_key}`, which is not \
                     established to carry the ordering key of `{input}`; the \
                     topic's order says nothing about same-key invocations."
                ),
            },

            Self::ByTopicKeyWithoutKeyDomain { input, topic } => Evidence {
                subject: Some(input.clone()),
                message: format!(
                    "`{input}` routes `by_topic_key`, but `{topic}` orders \
                     globally and declares no key domain to route by; the lane \
                     assignment of same-key deliveries is undetermined."
                ),
            },

            Self::RoutingDoesNotPreserveOrder { input, declared } => Evidence {
                subject: Some(input.clone()),
                message: match declared {
                    DispatchRouting::Unconstrained => format!(
                        "`{input}` dispatches with `unconstrained` routing: same-key \
                         deliveries may enter different lanes and be processed \
                         out of order."
                    ),

                    _ => format!(
                        "`{input}` declares no dispatch routing fact, so nothing \
                         keeps same-key deliveries in one lane."
                    ),
                },
            },

            Self::LaneConcurrencyNotSerial { input, declared } => Evidence {
                subject: Some(input.clone()),
                message: match declared {
                    LaneConcurrency::Bounded(bound) => format!(
                        "`{input}` admits bounded({bound}) invocations per lane: a \
                         later invocation may overtake an earlier one."
                    ),

                    LaneConcurrency::Unbounded => format!(
                        "`{input}` declares `unbounded` lane concurrency: a later \
                         invocation may overtake an earlier one."
                    ),

                    LaneConcurrency::Unspecified => format!(
                        "`{input}` declares no lane concurrency fact; overtaking \
                         within a lane cannot be excluded."
                    ),
                },
            },
        }
    }
}
