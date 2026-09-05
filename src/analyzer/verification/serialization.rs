//! Verification of operation serialization requirements (§9).
//!
//! A serialization requirement keyed by a `ValueRef` means:
//!
//! > Invocations with the same logical key must not execute
//! > concurrently.
//!
//! Serialization establishes mutual exclusion of same-key invocations.
//! It does not establish which same-key invocation should come first;
//! that is an ordering requirement and is deliberately out of scope
//! here (§9 "Serialization versus ordering").
//!
//! ## The constrained population
//!
//! A concrete invocation is associated with the input that triggered
//! it, and a `ValueRef` sourcing an input refers to the payload of
//! that triggering input (§7). An invocation triggered by a different
//! input has no value for the key, so it is not "an invocation with
//! the same logical key" as any other and the requirement does not
//! constrain it. For a key sourced from input `i`, the population is
//! therefore the invocations triggered by `i`. For a key sourced from
//! anything other than an input, no dispatch fact identifies which
//! invocations share a key, so the population is conservatively every
//! invocation of the operation, and only a global execution bound of
//! one can serialize it.
//!
//! ## Accepted proof routes
//!
//! 1. **Operation-serial** (§10): `execution.concurrency = bounded(1)`
//!    admits at most one simultaneously active invocation across the
//!    operation, so no two invocations overlap at all.
//! 2. **Vacuous population**: the key's subscription input admits no
//!    message schemas, so the population is empty by declaration.
//! 3. **Subscription-serial**: `single_lane` routing puts every
//!    delivery of the key's subscription into one logical lane, and
//!    lane concurrency `bounded(1)` prevents overlap within it, so no
//!    two invocations from that input overlap.
//! 4. **Keyed-lane-serial** (§8.2): the topic declares a keyed
//!    ordering domain, `by_topic_key` routing sends same-topic-key
//!    deliveries to one lane, lane concurrency `bounded(1)` prevents
//!    overlap within a lane, and the serialization key is established
//!    to carry the same logical value as the topic key for every
//!    admitted message schema — so same-key invocations share a lane
//!    and cannot overlap. Each declaration contributes a different
//!    fact and none is silently substituted for another.
//!
//! ## Routes deliberately not credited
//!
//! - **Locks** (§21). A lock protects the object instances its
//!   selector selects. Whether two same-key invocations conflict on a
//!   common instance depends on such an instance existing at lock
//!   time, which is runtime state the model cannot declare, and a
//!   lock serializes only the span from acquisition to transaction
//!   end, not the invocation's whole execution. Crediting a lock here
//!   would rest a proof on unknown facts (§1.1).
//! - **Serializable isolation** (§17). An equivalent serial commit
//!   order does not prevent concurrent execution.
//! - **Topic ordering alone** (§6). Delivery order does not serialize
//!   consumer execution; the keyed-lane route uses the keyed
//!   declaration only for the key domain that routing references.
//! - **`bounded(n)` with `n > 1`** anywhere (§10): it permits
//!   overlap.
//!
//! A requirement no route establishes is `Unproven`, never violated:
//! concurrency declarations are upper bounds, and nothing in the
//! model can prove that an overlapping same-key pair actually occurs
//! (§1.2).

use serde::{Deserialize, Serialize};

use crate::analyzer::{Diagnostic, DiagnosticCode, Evidence, Severity, VerificationCode};
use crate::spec::{
    DispatchRouting, FieldPath, Id, Input, LaneConcurrency, MessageSelector, Model, Operation,
    OperationConcurrency, SerializationRequirement, SubscriptionInput, TopicOrdering, ValueRef,
    ValueSource,
};

use super::describe::{describe_value_ref, describe_value_source, value_source_id};
use super::value_identity::canonical_value_path;

/// The verdict for one declared serialization requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SerializationCheck {
    pub operation: Id,

    /// Index into `operation.requirements.serialization`.
    pub requirement: usize,

    /// The requirement's key, copied so the check is self-contained.
    pub key: ValueRef,

    pub verdict: SerializationVerdict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SerializationVerdict {
    /// The requirement follows from the cited declared facts, subject
    /// to implementation conformance with those facts (§1.3).
    Proven { proof: SerializationProof },

    /// The declared facts do not establish the requirement. This is
    /// epistemic: it records which facts are missing or insufficient,
    /// not that a violation occurs (§1.2).
    Unproven {
        obstacles: Vec<SerializationObstacle>,
    },
}

/// A successful serialization argument and the facts it consumed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SerializationProof {
    /// `execution.concurrency = bounded(1)`: at most one invocation
    /// of the operation is active at any time, so no two invocations
    /// — same-key or otherwise — overlap (§10).
    OperationSerial,

    /// The key's subscription input admits no message schemas, so no
    /// invocation can bear the key and the requirement constrains an
    /// empty population.
    NoAdmittedInvocations { input: Id },

    /// `single_lane` routing plus lane concurrency `bounded(1)` on
    /// the key's subscription input: every key-bearing invocation
    /// passes through one lane admitting one active invocation.
    SubscriptionSerial { input: Id },

    /// The §8.2 composition: same-key deliveries of the key's
    /// subscription share a lane because the serialization key
    /// carries the same logical value as the keyed topic's ordering
    /// key for every admitted schema and routing is `by_topic_key`;
    /// lane concurrency `bounded(1)` prevents overlap within the
    /// lane.
    KeyedLaneSerial {
        input: Id,
        topic: Id,

        /// Per admitted message schema, the fact identifying the
        /// topic key with the serialization key.
        message_keys: Vec<MessageKeyFact>,
    },
}

/// For one admitted message schema, how the topic's ordering key was
/// identified with the serialization key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MessageKeyFact {
    pub schema: Id,

    /// The topic's declared ordering-key path for this schema.
    pub topic_key: FieldPath,

    pub identity: KeyIdentity,
}

/// Why two field paths of one message schema denote the same logical
/// value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KeyIdentity {
    /// The paths are identical, so they name the same field.
    SamePath,

    /// The paths differ but expand to the same canonical value path
    /// through declared fragment mappings, which assert semantic
    /// identity across the fragment boundary (§4).
    SameCanonicalValue { schema: Id, path: FieldPath },
}

/// A fact that is missing or insufficient for one candidate proof
/// route.
///
/// Obstacles preserve the declared value where one exists, so an
/// explicitly negative declaration (`unbounded`, `unconstrained`) is
/// distinguishable from an absent one (`unspecified`) (§1.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SerializationObstacle {
    /// The declared global operation concurrency does not bound
    /// simultaneous invocations to one.
    OperationConcurrencyNotSerial { declared: OperationConcurrency },

    /// The key is not sourced from an input declared by the
    /// operation, so no dispatch or lane fact selects which
    /// invocations share it.
    KeyNotFromInput { source: ValueSource },

    /// Key-bearing invocations arrive through a request input, and
    /// the DSL declares no request-side dispatch or concurrency
    /// facts.
    RequestInputHasNoDispatchFacts { input: Id },

    /// The subscription's routing provides no usable affinity between
    /// same-key deliveries and lanes.
    RoutingProvidesNoAffinity {
        input: Id,
        declared: DispatchRouting,
    },

    /// Routing is `by_topic_key`, but the subscribed topic declares
    /// no keyed ordering domain to route by.
    TopicNotKeyed { input: Id, topic: Id },

    /// The keyed topic declares no ordering-key mapping for an
    /// admitted schema. Validation rejects this shape; verification
    /// records it rather than assuming a mapping.
    TopicKeyMappingMissing { input: Id, topic: Id, schema: Id },

    /// The topic's ordering key for this schema is not established to
    /// carry the same logical value as the serialization key, so
    /// same-key deliveries may enter different lanes.
    KeyIdentityUnestablished {
        input: Id,
        topic: Id,
        schema: Id,
        topic_key: FieldPath,
    },

    /// The declared per-lane concurrency does not bound same-lane
    /// invocations to one.
    LaneConcurrencyNotSerial {
        input: Id,
        declared: LaneConcurrency,
    },
}

/// Checks every serialization requirement declared by the model.
pub fn check(model: &Model) -> Vec<SerializationCheck> {
    let mut checks = Vec::new();

    for (operation_id, operation) in &model.operations {
        for (index, requirement) in operation.requirements.serialization.iter().enumerate() {
            checks.push(SerializationCheck {
                operation: operation_id.clone(),
                requirement: index,
                key: requirement.key.clone(),
                verdict: check_requirement(model, operation, requirement),
            });
        }
    }

    checks
}

fn check_requirement(
    model: &Model,
    operation: &Operation,
    requirement: &SerializationRequirement,
) -> SerializationVerdict {
    // Route 1: a global execution bound of one serializes every pair
    // of invocations, whatever population the key defines.
    if is_serial_bound(operation.execution.concurrency) {
        return SerializationVerdict::Proven {
            proof: SerializationProof::OperationSerial,
        };
    }

    let mut obstacles = vec![SerializationObstacle::OperationConcurrencyNotSerial {
        declared: operation.execution.concurrency,
    }];

    // The remaining routes serialize the population of key-bearing
    // invocations, which is defined by the key's source input.
    let ValueSource::Input(input_id) = &requirement.key.source else {
        obstacles.push(SerializationObstacle::KeyNotFromInput {
            source: requirement.key.source.clone(),
        });

        return SerializationVerdict::Unproven { obstacles };
    };

    let Some(input) = operation.inputs.get(input_id) else {
        obstacles.push(SerializationObstacle::KeyNotFromInput {
            source: requirement.key.source.clone(),
        });

        return SerializationVerdict::Unproven { obstacles };
    };

    let subscription = match input {
        Input::Request(_) => {
            obstacles.push(SerializationObstacle::RequestInputHasNoDispatchFacts {
                input: input_id.clone(),
            });

            return SerializationVerdict::Unproven { obstacles };
        }

        Input::Subscription(subscription) => subscription,
    };

    // Route 2: a population empty by declaration is serialized
    // vacuously.
    if admits_no_messages(model, subscription) {
        return SerializationVerdict::Proven {
            proof: SerializationProof::NoAdmittedInvocations {
                input: input_id.clone(),
            },
        };
    }

    let lane_serial = is_serial_lane(subscription.dispatch.lane_concurrency);

    match subscription.dispatch.routing {
        // Route 3: one lane for every delivery of this subscription.
        DispatchRouting::SingleLane => {
            if lane_serial {
                return SerializationVerdict::Proven {
                    proof: SerializationProof::SubscriptionSerial {
                        input: input_id.clone(),
                    },
                };
            }
        }

        // Route 4: same-key deliveries share a lane through the
        // topic's key domain.
        DispatchRouting::ByTopicKey => {
            match keyed_lane_facts(model, input_id, subscription, &requirement.key.path) {
                Ok((topic, message_keys)) => {
                    if lane_serial {
                        return SerializationVerdict::Proven {
                            proof: SerializationProof::KeyedLaneSerial {
                                input: input_id.clone(),
                                topic,
                                message_keys,
                            },
                        };
                    }
                }

                Err(affinity_obstacles) => obstacles.extend(affinity_obstacles),
            }
        }

        DispatchRouting::Unspecified | DispatchRouting::Unconstrained => {
            obstacles.push(SerializationObstacle::RoutingProvidesNoAffinity {
                input: input_id.clone(),
                declared: subscription.dispatch.routing,
            });
        }
    }

    if !lane_serial {
        obstacles.push(SerializationObstacle::LaneConcurrencyNotSerial {
            input: input_id.clone(),
            declared: subscription.dispatch.lane_concurrency,
        });
    }

    SerializationVerdict::Unproven { obstacles }
}

fn is_serial_bound(concurrency: OperationConcurrency) -> bool {
    matches!(concurrency, OperationConcurrency::Bounded(bound) if bound.get() == 1)
}

pub(super) fn is_serial_lane(concurrency: LaneConcurrency) -> bool {
    matches!(concurrency, LaneConcurrency::Bounded(bound) if bound.get() == 1)
}

/// Whether the subscription's admitted message set is empty by
/// declaration.
///
/// `only []` admits nothing regardless of the topic. `all` admits the
/// topic's declared messages, so it is empty only when the topic is
/// resolvable and declares none; an unresolvable topic leaves the
/// admitted set unknown, which must not become a vacuous proof.
pub(super) fn admits_no_messages(model: &Model, subscription: &SubscriptionInput) -> bool {
    match &subscription.messages {
        MessageSelector::Only(messages) => messages.is_empty(),

        MessageSelector::All => model
            .topics
            .get(&subscription.topic)
            .is_some_and(|topic| topic.messages.is_empty()),
    }
}

/// Establishes the affinity leg of the keyed-lane route: for every
/// admitted message schema, the topic's ordering key must carry the
/// same logical value as the serialization key.
pub(super) fn keyed_lane_facts(
    model: &Model,
    input_id: &Id,
    subscription: &SubscriptionInput,
    serialization_key: &FieldPath,
) -> Result<(Id, Vec<MessageKeyFact>), Vec<SerializationObstacle>> {
    let topic_id = subscription.topic.clone();

    let keyed = model
        .topics
        .get(&topic_id)
        .and_then(|topic| match &topic.ordering {
            TopicOrdering::Keyed(key) => Some((topic, key)),

            TopicOrdering::Unspecified | TopicOrdering::Unordered | TopicOrdering::Global => None,
        });

    let Some((topic, topic_key)) = keyed else {
        return Err(vec![SerializationObstacle::TopicNotKeyed {
            input: input_id.clone(),
            topic: topic_id,
        }]);
    };

    let admitted: Vec<&Id> = match &subscription.messages {
        MessageSelector::All => topic.messages.iter().collect(),
        MessageSelector::Only(messages) => messages.iter().collect(),
    };

    let mut facts = Vec::new();
    let mut obstacles = Vec::new();

    for schema in admitted {
        let Some(mapped) = topic_key.mapping.get(schema) else {
            obstacles.push(SerializationObstacle::TopicKeyMappingMissing {
                input: input_id.clone(),
                topic: topic_id.clone(),
                schema: schema.clone(),
            });

            continue;
        };

        match key_identity(model, schema, mapped, serialization_key) {
            Some(identity) => facts.push(MessageKeyFact {
                schema: schema.clone(),
                topic_key: mapped.clone(),
                identity,
            }),

            None => obstacles.push(SerializationObstacle::KeyIdentityUnestablished {
                input: input_id.clone(),
                topic: topic_id.clone(),
                schema: schema.clone(),
                topic_key: mapped.clone(),
            }),
        }
    }

    if obstacles.is_empty() {
        Ok((topic_id, facts))
    } else {
        Err(obstacles)
    }
}

/// Whether two paths denote the same logical value in any instance of
/// the schema.
fn key_identity(
    model: &Model,
    schema: &Id,
    topic_key: &FieldPath,
    serialization_key: &FieldPath,
) -> Option<KeyIdentity> {
    if topic_key == serialization_key {
        return Some(KeyIdentity::SamePath);
    }

    let topic_canonical = canonical_value_path(model, schema, topic_key)?;
    let serialization_canonical = canonical_value_path(model, schema, serialization_key)?;

    (topic_canonical == serialization_canonical).then_some(KeyIdentity::SameCanonicalValue {
        schema: serialization_canonical.schema,
        path: serialization_canonical.path,
    })
}

impl SerializationCheck {
    /// The diagnostic for an unproven requirement.
    ///
    /// A proven requirement produces no diagnostic; its argument
    /// lives in the structured verdict.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        let SerializationVerdict::Unproven { obstacles } = &self.verdict else {
            return None;
        };

        let operation = &self.operation;
        let requirement = self.requirement;
        let key = describe_value_ref(&self.key);

        Some(Diagnostic {
            code: DiagnosticCode::Verification(VerificationCode::SerializationUnproven),
            severity: Severity::Unknown,
            subject: Some(operation.clone()),
            message: format!(
                "Serialization requirement {requirement} of `{operation}` is not \
                 established: no declared facts prove that invocations sharing \
                 {key} never execute concurrently."
            ),
            evidence: obstacles
                .iter()
                .map(|obstacle| obstacle.evidence(self))
                .collect(),
        })
    }
}

impl SerializationObstacle {
    fn evidence(&self, check: &SerializationCheck) -> Evidence {
        match self {
            Self::OperationConcurrencyNotSerial { declared } => Evidence {
                subject: Some(check.operation.clone()),
                message: match declared {
                    OperationConcurrency::Unspecified => {
                        "No global operation concurrency fact is declared, so a \
                         global execution bound of one cannot be inferred."
                            .to_string()
                    }

                    OperationConcurrency::Unbounded => {
                        "Operation concurrency is explicitly `unbounded`: no \
                         finite global bound limits simultaneous invocations."
                            .to_string()
                    }

                    OperationConcurrency::Bounded(bound) => format!(
                        "Operation concurrency `bounded({bound})` permits \
                         {bound} simultaneous invocations; only a bound of one \
                         serializes all invocations."
                    ),
                },
            },

            Self::KeyNotFromInput { source } => Evidence {
                subject: value_source_id(source).cloned(),
                message: format!(
                    "The serialization key is sourced from {}, not from an \
                     input declared by the operation; no dispatch or lane fact \
                     selects which invocations share such a key, so only a \
                     global operation concurrency bound of one could serialize \
                     them.",
                    describe_value_source(source)
                ),
            },

            Self::RequestInputHasNoDispatchFacts { input } => Evidence {
                subject: Some(input.clone()),
                message: format!(
                    "Key-bearing invocations arrive through request input \
                     `{input}`, and the model declares no request-side dispatch \
                     or concurrency fact that could serialize same-key \
                     requests."
                ),
            },

            Self::RoutingProvidesNoAffinity { input, declared } => Evidence {
                subject: Some(input.clone()),
                message: match declared {
                    DispatchRouting::Unspecified => format!(
                        "No dispatch-routing fact is declared for `{input}`: \
                         nothing routes same-key deliveries to one lane."
                    ),

                    _ => format!(
                        "Dispatch routing for `{input}` is explicitly \
                         `unconstrained`: same-key deliveries are not \
                         guaranteed to share a lane."
                    ),
                },
            },

            Self::TopicNotKeyed { input, topic } => Evidence {
                subject: Some(topic.clone()),
                message: format!(
                    "Topic `{topic}` does not declare keyed ordering, so the \
                     `by_topic_key` routing of `{input}` has no key domain to \
                     route by."
                ),
            },

            Self::TopicKeyMappingMissing { topic, schema, .. } => Evidence {
                subject: Some(schema.clone()),
                message: format!(
                    "Keyed topic `{topic}` declares no ordering-key mapping \
                     for admitted schema `{schema}`."
                ),
            },

            Self::KeyIdentityUnestablished {
                schema, topic_key, ..
            } => Evidence {
                subject: Some(schema.clone()),
                message: format!(
                    "For messages of `{schema}`, the topic's ordering key \
                     `{topic_key}` is not established to carry the same \
                     logical value as the serialization key `{}`, so same-key \
                     deliveries may enter different lanes.",
                    check.key.path
                ),
            },

            Self::LaneConcurrencyNotSerial { input, declared } => Evidence {
                subject: Some(input.clone()),
                message: match declared {
                    LaneConcurrency::Unspecified => format!(
                        "No per-lane concurrency fact is declared for \
                         `{input}`."
                    ),

                    LaneConcurrency::Unbounded => format!(
                        "Lane concurrency for `{input}` is explicitly \
                         `unbounded`: same-lane invocations may overlap."
                    ),

                    LaneConcurrency::Bounded(bound) => format!(
                        "Lane concurrency `bounded({bound})` for `{input}` \
                         permits {bound} overlapping invocations per lane; \
                         serialization needs a bound of one."
                    ),
                },
            },
        }
    }
}
