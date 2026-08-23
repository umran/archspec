//! The obligation report: the flattened, presentation-oriented
//! projection of verification results.
//!
//! Obligations are enumerated per declared requirement. A proven
//! obligation carries the declared facts its proof relies on — per
//! §25, a proof is conditional on the implementation conforming to
//! them. An unproven obligation carries the checker's evidence:
//! exactly which facts are missing or insufficient. `unknown` is
//! epistemic (§1.2), never a violation; V1 produces no `disproven`
//! verdicts, though the format admits them for future checkers.
//!
//! `scaffold` enumerates every obligation the declared requirements
//! imply, all `unknown` — executable documentation of the shape.
//! `obligations` fills the same enumeration in from a real
//! `VerificationReport`; families V1 does not verify (ordering,
//! object history) stay `unknown` with a note saying so.

use serde::{Deserialize, Serialize};

use crate::analyzer::verification::{
    DuplicateHandling, LaneFact, LineageFact, ModelNote, OrderingProof, OrderingVerdict,
    PrecedenceSource,
};
use crate::analyzer::verification::{
    self, ArtifactReplay, ConsumerCollapse, EffectSafety, IdempotencyProof, IdempotencyVerdict,
    InstanceStability, KeyIdentity, RecoverabilityProof, RecoverabilityVerdict, Resolution,
    ResponseReplayProof, ResponseReplayVerdict, RetryDriver, RetryRoute, SerializationProof,
    SerializationVerdict, StableRoot, VerificationReport,
};
use crate::spec::{
    CompletionRequirement, Id, Model, ObjectHistoryRequirement, ValueRef, ValueSource,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProverReport {
    /// Version of this report format, not of the model.
    pub format: u32,

    /// Revision of the model the report was produced against.
    ///
    /// The visualization warns when this disagrees with the rendered
    /// model's revision.
    pub model_revision: Option<u64>,

    pub obligations: Vec<Obligation>,

    /// Model-wide notes that belong to no single obligation: warnings
    /// the checker raises about gaps no declaration covers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<EvidenceItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Obligation {
    /// Stable identity of the obligation within the report.
    pub id: String,

    pub property: Property,
    pub subject: Subject,
    pub status: Status,

    /// One-line human-readable statement of the obligation.
    pub summary: String,

    /// Declared model facts the verdict relies on. A proof is
    /// conditional on the implementation conforming to these.
    #[serde(default)]
    pub assumptions: Vec<String>,

    /// Model facts explaining how the verdict was reached.
    #[serde(default)]
    pub evidence: Vec<EvidenceItem>,

    /// Present only when `status` is `disproven`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counterexample: Option<Counterexample>,
}

/// The correctness property an obligation discharges.
///
/// The first four mirror `OperationRequirements`; `response_replay`
/// splits out the response half of an idempotency requirement;
/// `object_history` mirrors `ObjectHistoryRequirement`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Property {
    Serialization,
    Ordering,
    Idempotency,
    Recoverability,
    ResponseReplay,
    ObjectHistory,
    Custom { name: String },
}

/// The model entity an obligation is anchored to.
///
/// `requirement` indexes into the corresponding requirement list on
/// the operation, tying the obligation back to the declaration that
/// produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Subject {
    Operation {
        operation: Id,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        requirement: Option<usize>,
    },
    Flow {
        operation: Id,
        flow: Id,
    },
    Transaction {
        operation: Id,
        transaction: Id,
    },
    Object {
        data_model: Id,
        object: Id,
    },
    StateMachine {
        machine: Id,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transition: Option<Id>,
    },
    Topic {
        topic: Id,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The property follows for all executions admitted by the model.
    Proven,

    /// The solver found an admitted execution violating the property.
    Disproven,

    /// The solver could not decide, typically because a required fact
    /// is `unspecified`. Not evidence of a violation.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceItem {
    /// Model entity the fact concerns, when one applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<Id>,

    pub message: String,
}

/// A concrete admitted execution that violates the property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counterexample {
    pub trace: Vec<TraceStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceStep {
    /// Entity performing the step (an operation, topic, or the
    /// environment), when one applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<Id>,

    pub description: String,
}

/// Builds a scaffold report enumerating every obligation the declared
/// requirements imply, all with status `unknown`.
///
/// Doubles as executable documentation of the format and as the shape
/// `obligations` fills in from real verification results.
pub fn scaffold(model: &Model) -> ProverReport {
    let mut obligations = Vec::new();

    fn requirement_obligation(
        op_id: &Id,
        property: Property,
        index: usize,
        summary: String,
    ) -> Obligation {
        Obligation {
            id: format!("oblig.{}.{}.{}", op_id, property_slug(&property), index),
            property,
            subject: Subject::Operation {
                operation: op_id.clone(),
                requirement: Some(index),
            },
            status: Status::Unknown,
            summary,
            assumptions: Vec::new(),
            evidence: Vec::new(),
            counterexample: None,
        }
    }

    for (op_id, op) in &model.operations {
        let push = |obligations: &mut Vec<Obligation>,
                    property: Property,
                    index: usize,
                    summary: String| {
            obligations.push(requirement_obligation(op_id, property, index, summary));
        };

        for (i, r) in op.requirements.serialization.iter().enumerate() {
            push(
                &mut obligations,
                Property::Serialization,
                i,
                format!(
                    "Invocations of {op_id} sharing key {} never overlap.",
                    value_ref_label(&r.key)
                ),
            );
        }

        for (i, r) in op.requirements.ordering.iter().enumerate() {
            push(
                &mut obligations,
                Property::Ordering,
                i,
                format!(
                    "Invocations of {op_id} sharing key {} take effect in \
                     their semantic order.",
                    value_ref_label(&r.key)
                ),
            );
        }

        for (i, r) in op.requirements.idempotency.iter().enumerate() {
            push(
                &mut obligations,
                Property::Idempotency,
                i,
                format!(
                    "Repeated attempts at {op_id} sharing the declared key \
                     produce the effects of a single invocation.",
                ),
            );

            if r.response == crate::spec::ResponseReplayRequirement::ReplayConsistent {
                obligations.push(Obligation {
                    id: format!("oblig.{op_id}.response_replay.{i}"),
                    property: Property::ResponseReplay,
                    subject: Subject::Operation {
                        operation: op_id.clone(),
                        requirement: Some(i),
                    },
                    status: Status::Unknown,
                    summary: format!(
                        "Every attempt at {op_id} sharing the declared key \
                         observes an equivalent response."
                    ),
                    assumptions: Vec::new(),
                    evidence: Vec::new(),
                    counterexample: None,
                });
            }
        }

        for (i, r) in op.requirements.recoverability.iter().enumerate() {
            push(
                &mut obligations,
                Property::Recoverability,
                i,
                format!(
                    "An interrupted invocation of {op_id} {} a declared \
                     flow's terminal step.",
                    match r.completion {
                        CompletionRequirement::Resumable => "can be resumed to reach",
                        CompletionRequirement::Guaranteed => "is re-driven until it reaches",
                    }
                ),
            );
        }
    }

    for (dm_id, dm) in &model.data_models {
        for (obj_id, obj) in &dm.objects {
            for req in &obj.requirements.history {
                let name = match req {
                    ObjectHistoryRequirement::Linearizable => "linearizable",
                };

                obligations.push(Obligation {
                    id: format!("oblig.{dm_id}.{obj_id}.history.{name}"),
                    property: Property::ObjectHistory,
                    subject: Subject::Object {
                        data_model: dm_id.clone(),
                        object: obj_id.clone(),
                    },
                    status: Status::Unknown,
                    summary: format!(
                        "Accesses to {obj_id} admit a legal sequential \
                         history respecting real-time precedence."
                    ),
                    assumptions: Vec::new(),
                    evidence: Vec::new(),
                    counterexample: None,
                });
            }
        }
    }

    ProverReport {
        format: 1,
        model_revision: Some(model.revision.0),
        obligations,
        notes: Vec::new(),
    }
}

/// Builds the obligation report from real verification results.
///
/// Every declared obligation appears. The families V1 verifies carry
/// their verdicts — proofs rendered as assumptions, obstacles as
/// evidence; ordering and object-history obligations stay `unknown`
/// with a note that no V1 verifier attempts them.
pub fn obligations(model: &Model, verification: &VerificationReport) -> ProverReport {
    let mut report = scaffold(model);

    for obligation in &mut report.obligations {
        match obligation.property {
            Property::ObjectHistory => obligation.evidence.push(EvidenceItem {
                subject: None,
                message: "No V1 verifier attempts object-history obligations."
                    .to_string(),
            }),

            _ => {}
        }
    }

    for check in &verification.serialization {
        let id = obligation_id(&check.operation, "serialization", check.requirement);

        patch(&mut report, &id, || match &check.verdict {
            SerializationVerdict::Proven { proof } => Ok(serialization_assumptions(proof)),
            SerializationVerdict::Unproven { .. } => Err(check.diagnostic()),
        });
    }

    for check in &verification.ordering {
        let id = obligation_id(&check.operation, "ordering", check.requirement);

        patch(&mut report, &id, || match &check.verdict {
            OrderingVerdict::Proven { proof } => Ok(ordering_assumptions(proof)),
            OrderingVerdict::Unproven { .. } => Err(check.diagnostic()),
        });
    }

    for check in &verification.idempotency {
        let id = obligation_id(&check.operation, "idempotency", check.requirement);

        patch(&mut report, &id, || match &check.verdict {
            IdempotencyVerdict::Proven { proof } => Ok(idempotency_assumptions(proof)),
            IdempotencyVerdict::Unproven { .. } => Err(check.diagnostic()),
        });

        if let Some(obligation) = report
            .obligations
            .iter_mut()
            .find(|obligation| obligation.id == id)
        {
            if check.coinductive {
                obligation.assumptions.insert(
                    0,
                    "proven coinductively: this requirement and the ones it reaches \
                     through request targets or message consumers collapse each \
                     other's duplicates, and the greatest fixpoint admits the cycle \
                     (effect-safety draft §4.1)"
                        .to_string(),
                );
            }

            // Lineage facts ride along: declared propagations become
            // assumptions the identity-based population rests on, and
            // their absence is evidence a reader wants next to it.
            for lineage in &check.lineage {
                let producer = match &lineage.producer {
                    verification::ProducerRef::Operation { operation, effect } => {
                        format!("{operation} through {effect}")
                    }
                    verification::ProducerRef::Transition {
                        machine,
                        transition,
                        effect,
                    } => format!("{machine}'s {transition} through {effect}"),
                };

                match &lineage.fact {
                    LineageFact::Propagated { source, requirement } => {
                        let key = source
                            .components
                            .iter()
                            .map(value_ref_label)
                            .collect::<Vec<_>>()
                            .join(" + ");

                        obligation.assumptions.push(match requirement {
                            Some(index) => format!(
                                "the identity of {} on {} is carried by its producer's \
                                 idempotency key ({key}, requirement #{index}): declared \
                                 propagation from {producer}",
                                lineage.schema, lineage.topic
                            ),
                            None => format!(
                                "the identity of {} on {} carries {key} by declared \
                                 propagation from {producer}",
                                lineage.schema, lineage.topic
                            ),
                        });
                    }

                    LineageFact::Undeclared => obligation.evidence.push(EvidenceItem {
                        subject: match &lineage.producer {
                            verification::ProducerRef::Operation { effect, .. }
                            | verification::ProducerRef::Transition { effect, .. } => {
                                Some(effect.clone())
                            }
                        },
                        message: format!(
                            "{producer} publishes {} to {} without a declared \
                             propagation onto its identity fields; the identity this \
                             population rests on is the topic declaration alone.",
                            lineage.schema, lineage.topic
                        ),
                    }),
                }
            }
        }
    }

    for check in &verification.response_replay {
        let id = obligation_id(&check.operation, "response_replay", check.requirement);

        patch(&mut report, &id, || match &check.verdict {
            ResponseReplayVerdict::Proven { proof } => Ok(response_replay_assumptions(proof)),
            ResponseReplayVerdict::Unproven { .. } => Err(check.diagnostic()),
        });
    }

    for check in &verification.recoverability {
        let id = obligation_id(&check.operation, "recoverability", check.requirement);

        patch(&mut report, &id, || match &check.verdict {
            RecoverabilityVerdict::Proven { proof } => Ok(recoverability_assumptions(proof)),
            RecoverabilityVerdict::Unproven { .. } => Err(check.diagnostic()),
        });

        // Notes ride along as evidence: facts a reader wants next to
        // the verdict, which they do not change.
        if let Some(obligation) = report
            .obligations
            .iter_mut()
            .find(|obligation| obligation.id == id)
        {
            obligation.evidence.extend(check.notes.iter().map(|note| {
                let evidence = note.evidence();

                EvidenceItem {
                    subject: evidence.subject,
                    message: evidence.message,
                }
            }));
        }
    }

    report.notes = verification
        .notes
        .iter()
        .map(|note: &ModelNote| EvidenceItem {
            subject: note.subject(),
            message: note.message(),
        })
        .collect();

    report
}

fn obligation_id(operation: &Id, slug: &str, requirement: usize) -> String {
    format!("oblig.{operation}.{slug}.{requirement}")
}

/// Applies one check's verdict to its scaffolded obligation: proven
/// verdicts contribute assumptions, unproven ones contribute the
/// diagnostic's evidence.
fn patch(
    report: &mut ProverReport,
    id: &str,
    verdict: impl FnOnce() -> Result<Vec<String>, Option<crate::analyzer::Diagnostic>>,
) {
    let Some(obligation) = report
        .obligations
        .iter_mut()
        .find(|obligation| obligation.id == id)
    else {
        return;
    };

    match verdict() {
        Ok(assumptions) => {
            obligation.status = Status::Proven;
            obligation.assumptions = assumptions;
        }

        Err(diagnostic) => {
            obligation.status = Status::Unknown;

            if let Some(diagnostic) = diagnostic {
                obligation.evidence = diagnostic
                    .evidence
                    .into_iter()
                    .map(|evidence| EvidenceItem {
                        subject: evidence.subject,
                        message: evidence.message,
                    })
                    .collect();
            }
        }
    }
}

fn property_slug(property: &Property) -> &str {
    match property {
        Property::Serialization => "serialization",
        Property::Ordering => "ordering",
        Property::Idempotency => "idempotency",
        Property::Recoverability => "recoverability",
        Property::ResponseReplay => "response_replay",
        Property::ObjectHistory => "object_history",
        Property::Custom { name } => name,
    }
}

fn value_ref_label(value: &ValueRef) -> String {
    let source = match &value.source {
        ValueSource::Input(id)
        | ValueSource::Effect(id)
        | ValueSource::InvocationResult(id)
        | ValueSource::StateMachineSubject(id)
        | ValueSource::TransactionRead(id) => id,
    };

    format!("{source}.{}", value.path)
}

fn serialization_assumptions(proof: &SerializationProof) -> Vec<String> {
    match proof {
        SerializationProof::OperationSerial => vec![
            "operation concurrency is bounded(1): no two invocations are \
             simultaneously active"
                .to_string(),
        ],

        SerializationProof::NoAdmittedInvocations { input } => vec![format!(
            "{input} admits no message schemas; the requirement constrains no \
             invocations"
        )],

        SerializationProof::SubscriptionSerial { input } => vec![
            format!("every delivery of {input} enters one logical lane (single_lane)"),
            "lane concurrency bounded(1) prevents overlap within the lane".to_string(),
        ],

        SerializationProof::KeyedLaneSerial {
            input,
            topic,
            message_keys,
        } => {
            let mut assumptions = vec![format!(
                "{topic} routes same-key deliveries of {input} onto one lane \
                 (keyed ordering + by_topic_key dispatch)"
            )];

            for key in message_keys {
                assumptions.push(match &key.identity {
                    KeyIdentity::SamePath => format!(
                        "for {}, the topic key {} is the serialization key field",
                        key.schema, key.topic_key
                    ),

                    KeyIdentity::SameCanonicalValue { schema, path } => format!(
                        "for {}, the topic key {} carries the serialization key's \
                         value ({schema}.{path} via fragment aliasing)",
                        key.schema, key.topic_key
                    ),
                });
            }

            assumptions
                .push("lane concurrency bounded(1) prevents overlap within the lane".to_string());

            assumptions
        }
    }
}

fn ordering_assumptions(proof: &OrderingProof) -> Vec<String> {
    match proof {
        OrderingProof::NoAdmittedInvocations { input } => vec![format!(
            "{input} admits no message schemas; no invocation bears the key and \
             no precedence exists to preserve"
        )],

        OrderingProof::LaneOrder {
            input,
            topic,
            precedence,
            lane,
            duplicates,
        } => {
            let mut assumptions = Vec::new();

            match precedence {
                PrecedenceSource::KeyedTopic { message_keys } => {
                    assumptions.push(format!(
                        "{topic} orders same-key messages (keyed ordering); that order \
                         is the precedence"
                    ));

                    for key in message_keys {
                        assumptions.push(match &key.identity {
                            KeyIdentity::SamePath => format!(
                                "for {}, the topic key {} is the ordering key field",
                                key.schema, key.topic_key
                            ),

                            KeyIdentity::SameCanonicalValue { schema, path } => format!(
                                "for {}, the topic key {} and the ordering key both \
                                 denote {schema}.{path} through declared fragment mappings",
                                key.schema, key.topic_key
                            ),
                        });
                    }
                }

                PrecedenceSource::GlobalTopic => assumptions.push(format!(
                    "{topic} orders every message (global ordering); that order is \
                     the precedence for any key"
                )),
            }

            assumptions.push(match lane {
                LaneFact::ByTopicKey => format!(
                    "by_topic_key dispatch keeps same-key deliveries of {input} in one \
                     lane, which dispatches them in delivery order"
                ),

                LaneFact::SingleLane => format!(
                    "every delivery of {input} enters one lane (single_lane), which \
                     dispatches them in delivery order"
                ),
            });

            assumptions.push(
                "lane concurrency bounded(1) stops a later invocation overtaking an \
                 earlier one"
                    .to_string(),
            );

            assumptions.push(match duplicates {
                DuplicateHandling::SingleDelivery => format!(
                    "{input} receives each logical message at most once, so no late \
                     duplicate can reorder"
                ),

                DuplicateHandling::CollapsedByIdempotency { requirement } => format!(
                    "a redelivered earlier message does no distinguishable work: \
                     idempotency requirement #{requirement} keyed from {input} is proven"
                ),
            });

            assumptions
        }
    }
}

fn idempotency_assumptions(proof: &IdempotencyProof) -> Vec<String> {
    match proof {
        IdempotencyProof::NoAdmittedInvocations { input } => vec![format!(
            "{input} admits no message schemas; no attempt can bear the key"
        )],

        IdempotencyProof::NoAdmittedFlows { input } => vec![format!(
            "no admitted flow exists for {input}; an attempt performs no \
             modeled work"
        )],

        IdempotencyProof::SingleDelivery { input, topic } => vec![format!(
            "{input} receives at-most-once delivery from {topic}, whose message \
             identity is pinned by the key: a class holds at most one attempt"
        )],

        IdempotencyProof::RetrySafeFlows { flows } => {
            let mut assumptions = Vec::new();

            for flow in flows {
                let prefix = flow_prefix(flows.len(), &flow.flow);

                for transaction in &flow.transactions {
                    assumptions.push(match &transaction.route {
                        RetryRoute::KeyedCommit { key } => format!(
                            "{prefix}{} commits are deduplicated by {}, stable \
                             across the attempt class",
                            transaction.transaction,
                            root_labels(key)
                        ),

                        RetryRoute::NaturalReplay => format!(
                            "{prefix}{} is naturally replayable: re-execution \
                             reproduces the same logical state",
                            transaction.transaction
                        ),
                    });
                }

                for effect in &flow.effects {
                    match &effect.safety {
                        EffectSafety::ExternallyDeduplicated { key } => assumptions.push(format!(
                            "{prefix}the external boundary of {} deduplicates \
                             executions sharing {}",
                            effect.effect,
                            root_labels(key)
                        )),

                        EffectSafety::SameLogicalMessage {
                            topic,
                            schema,
                            instance,
                            consumers,
                        } => {
                            assumptions.push(format!(
                                "{prefix}duplicate executions of {} publish the same \
                                 logical message under {topic}'s message identity \
                                 ({})",
                                effect.effect,
                                instance_label(instance)
                            ));

                            if consumers.is_empty() {
                                assumptions.push(format!(
                                    "{prefix}no modeled subscription on {topic} admits \
                                     {schema}; the cascade ends at the topic"
                                ));
                            }

                            for consumer in consumers {
                                assumptions.push(match consumer {
                                    ConsumerCollapse::ProvenRequirement { operation, input } => {
                                        format!(
                                            "{prefix}duplicate deliveries of {schema} to \
                                             {operation} via {input} fall into one proven \
                                             idempotency class"
                                        )
                                    }

                                    ConsumerCollapse::SingleDelivery { operation, input } => {
                                        format!(
                                            "{prefix}{operation} via {input} receives \
                                             {schema} at most once: one logical message \
                                             under at-most-once delivery"
                                        )
                                    }
                                });
                            }
                        }

                        EffectSafety::DeduplicatedByTarget {
                            operation,
                            input,
                            instance,
                        } => assumptions.push(format!(
                            "{prefix}duplicate requests of {} fall into one \
                             proven idempotency class of {operation} via {input} \
                             ({})",
                            effect.effect,
                            instance_label(instance)
                        )),
                    }
                }
            }

            assumptions
        }
    }
}

fn response_replay_assumptions(proof: &ResponseReplayProof) -> Vec<String> {
    match proof {
        ResponseReplayProof::NoAdmittedInvocations { input } => vec![format!(
            "{input} admits no message schemas; no attempt can bear the key"
        )],

        ResponseReplayProof::NoResolvedResponse { input } => vec![format!(
            "no admitted flow resolves a response for {input}; there is nothing \
             to stabilize"
        )],

        ResponseReplayProof::ClassFixedResult {
            result,
            transaction,
            replay,
            ..
        } => vec![match replay {
            ArtifactReplay::Recovered { .. } => format!(
                "the response resolves {result}, retained by {transaction}'s \
                 keyed commit and recovered on every retry"
            ),

            ArtifactReplay::Reconstructed { .. } => format!(
                "the response resolves {result}, reconstructed deterministically \
                 by naturally replaying {transaction}"
            ),

            ArtifactReplay::Unavailable { .. } => format!(
                "the response resolves {result} via {transaction}"
            ),
        }],
    }
}

fn recoverability_assumptions(proof: &RecoverabilityProof) -> Vec<String> {
    match proof {
        RecoverabilityProof::NoAdmittedInvocations { input } => vec![format!(
            "{input} admits no message schemas; no attempt can bear the key"
        )],

        RecoverabilityProof::Resumable { flows } => resumption_assumptions(flows),

        RecoverabilityProof::Guaranteed { driver, flows } => {
            let mut assumptions = vec![match driver {
                RetryDriver::AtLeastOnceDelivery { input, topic } => format!(
                    "{input} redelivers via {topic} at least once, re-driving \
                     interrupted invocations"
                ),

                RetryDriver::InboundRepeatableRequest { operation, effect } => format!(
                    "{operation} may repeat its request through {effect}, \
                     re-driving interrupted invocations"
                ),

                RetryDriver::InboundRepeatableTransitionEffect {
                    machine,
                    transition,
                    effect,
                } => format!(
                    "transition {transition} of {machine} may repeat its request \
                     through {effect}, re-driving interrupted invocations"
                ),
            }];

            assumptions.extend(resumption_assumptions(flows));

            assumptions
        }
    }
}

fn resumption_assumptions(flows: &[verification::FlowResumption]) -> Vec<String> {
    let mut assumptions = Vec::new();

    for flow in flows {
        let prefix = flow_prefix(flows.len(), &flow.flow);

        for transaction in &flow.transactions {
            assumptions.push(match &transaction.resolution {
                Resolution::KeyedCommit { key } => format!(
                    "{prefix}{} resolves on re-encounter through its keyed \
                     commit ({})",
                    transaction.transaction,
                    root_labels(key)
                ),

                Resolution::NaturalReplay => format!(
                    "{prefix}{} re-executes safely by natural replay",
                    transaction.transaction
                ),

                Resolution::TerminalStep => format!(
                    "{prefix}{} is the flow's terminal step; no failing prefix \
                     follows its commit",
                    transaction.transaction
                ),
            });
        }

        for artifact in &flow.artifacts {
            assumptions.push(match &artifact.replay {
                ArtifactReplay::Recovered { transaction, .. } => format!(
                    "{prefix}artifact {} is recovered from {transaction}'s keyed \
                     commit on resumption",
                    artifact.artifact
                ),

                ArtifactReplay::Reconstructed { transaction, .. } => format!(
                    "{prefix}artifact {} is reconstructed by naturally replaying \
                     {transaction}",
                    artifact.artifact
                ),

                ArtifactReplay::Unavailable { transaction, .. } => format!(
                    "{prefix}artifact {} is supplied by {transaction}",
                    artifact.artifact
                ),
            });
        }
    }

    assumptions
}

fn flow_prefix(flow_count: usize, flow: &Id) -> String {
    if flow_count > 1 {
        format!("in {flow}: ")
    } else {
        String::new()
    }
}

fn root_labels(roots: &[StableRoot]) -> String {
    if roots.is_empty() {
        return "its declared key".to_string();
    }

    roots
        .iter()
        .map(|root| value_ref_label(&root.root))
        .collect::<Vec<_>>()
        .join(", ")
}

fn instance_label(instance: &InstanceStability) -> String {
    match instance {
        InstanceStability::ReplayDeterministic { .. } => {
            "the instance is replay-deterministic".to_string()
        }

        InstanceStability::EstablishedIntent { intent, replay } => match replay {
            ArtifactReplay::Recovered { transaction, .. } => format!(
                "intent {intent} is recovered from {transaction}'s keyed commit"
            ),

            ArtifactReplay::Reconstructed { transaction, .. } => format!(
                "intent {intent} is reconstructed by naturally replaying {transaction}"
            ),

            ArtifactReplay::Unavailable { .. } => format!("intent {intent}"),
        },
    }
}
