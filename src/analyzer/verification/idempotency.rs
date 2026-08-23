//! Verification of operation idempotency requirements (§9 of the
//! semantics contract; `ARCHSPEC_EFFECT_SAFETY_DRAFT.md`).
//!
//! > Repeated attempts representing the same logical invocation must
//! > not cause externally distinguishable duplicate logical work
//! > beyond what the declared idempotency contract permits.
//!
//! The analysis composes the replay engine across each admitted flow
//! (the same admitted-flow scoping as recoverability), under the
//! governing key's population (§12):
//!
//! - **State leg**: every transaction step must be retry-safe — a
//!   keyed commit over a stable key commits once per class, and a
//!   naturally replayable body reproduces the same logical state on
//!   re-execution. There is no final-step exemption: a duplicate
//!   delivery re-drives the whole flow even after terminal
//!   completion, so every committed transaction may be
//!   re-encountered.
//! - **Effect leg**: duplicate execution is possible at every effect
//!   site (§14 — even a recovered intent may re-execute when a crash
//!   hides a prior success), so each site must be duplicate-safe:
//!   an external effect through its declared `deduplicated_by` over a
//!   stable key; a request by targeting an input whose operation
//!   carries a proven idempotency requirement keyed from that input,
//!   fed by a class-fixed instance; a publication by being the *same
//!   logical message* — a class-fixed instance published to a topic
//!   whose message identity maps the schema — **and** by every modeled
//!   consumer of that message collapsing duplicate deliveries of it:
//!   a proven idempotency requirement keyed from the subscription, or
//!   `at_most_once` delivery of the one logical message.
//!
//! The request and publication legs follow the trigger graph
//! (`trigger`): duplicate work an attempt causes downstream is still
//! work it caused, so a requirement is proven only when the cascade
//! it starts collapses everywhere the model can see. That makes
//! verdicts mutually dependent, so `check` computes a least fixpoint:
//! requirements are re-checked as their request targets and message
//! consumers become proven, and cyclic dependencies settle unproven —
//! the conservative answer.
//!
//! Response consistency is the separate response-replay obligation
//! and is not re-checked here. Vacuous routes: an empty population,
//! no admitted flow (no modeled work to duplicate — recoverability
//! treats the same shape as an obstacle, and the asymmetry is
//! deliberate: progress is impossible, safety is trivial), and
//! single delivery (`at_most_once` with the payload identity-pinned:
//! same-class messages are one logical message delivered at most
//! once, so a class holds at most one attempt).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::analyzer::{Diagnostic, DiagnosticCode, Evidence, Severity, VerificationCode};
use crate::spec::{
    DeliverySemantics, Derivation, Effect, ExternalEffect, FieldPath, FlowStep, Id, IdempotencyGuarantee, IdempotencyKey, Input, InvocationFlow, MessageIdentity, MessageSelector, Model, Operation, PublicationEffect, RequestEffect, TransitionSideEffect, ValueRef, ValueSource,
};

use super::describe::{gap_sentences, governing_key_evidence, stability_sentence};
use super::replay::{
    ArtifactReplay, GoverningKeyDefect, ReplayAnalysis, ReplayGap, StabilityGap, StableRoot,
};
use super::trigger::{ProducerSite, TriggerGraph, collapses_duplicates};

/// The verdict for one declared idempotency requirement's side-effect
/// obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdempotencyCheck {
    pub operation: Id,

    /// Index into `operation.requirements.idempotency`.
    pub requirement: usize,

    /// The governing key, copied so the check is self-contained.
    pub key: IdempotencyKey,

    pub verdict: IdempotencyVerdict,

    /// Set when the proof holds only together with the proofs of the
    /// requirements it reaches through request targets or message
    /// consumers, and theirs hold only with it: the greatest fixpoint
    /// admits such a cycle by the minimal-counterexample argument of
    /// `ARCHSPEC_EFFECT_SAFETY_DRAFT.md` §4.1.
    #[serde(default)]
    pub coinductive: bool,

    /// For a governing key whose population is a subscription's
    /// messages on a topic with a keyed identity: per admitted schema
    /// and modeled producer, whether a declared propagation (§12)
    /// carries a key onto the identity fields the population rests on.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lineage: Vec<IdentityLineage>,
}

/// How a message's declared identity on its topic relates to the key
/// of the declaration that publishes it — the propagation lineage of
/// §12, read from the consumer's side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityLineage {
    pub topic: Id,
    pub schema: Id,
    pub producer: ProducerRef,
    pub fact: LineageFact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProducerRef {
    Operation { operation: Id, effect: Id },
    Transition { machine: Id, transition: Id, effect: Id },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LineageFact {
    /// The producer declares a propagation whose targets cover the
    /// identity fields, so the identity carries `source`; when the
    /// source is the key of one of the producing operation's own
    /// idempotency requirements, `requirement` names it, and distinct
    /// logical invocations of the producer publish distinct messages.
    Propagated {
        source: IdempotencyKey,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        requirement: Option<usize>,
    },

    /// No declared propagation carries a key onto the identity fields:
    /// the identity rests on the topic declaration alone.
    Undeclared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IdempotencyVerdict {
    Proven { proof: IdempotencyProof },
    Unproven { obstacles: Vec<IdempotencyObstacle> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IdempotencyProof {
    /// The triggering subscription admits no message schemas, so no
    /// attempt can bear the key.
    NoAdmittedInvocations { input: Id },

    /// No admitted flow exists for the triggering input: an attempt
    /// performs no modeled work, so nothing can be duplicated.
    NoAdmittedFlows { input: Id },

    /// `at_most_once` delivery with the payload identity-pinned:
    /// same-class messages are one logical message, delivered no more
    /// than once, so repeated attempts cannot exist.
    SingleDelivery { input: Id, topic: Id },

    /// Every admitted flow is retry-safe in both legs.
    RetrySafeFlows { flows: Vec<FlowRetrySafety> },
}

/// The retry-safety argument for one admitted flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowRetrySafety {
    pub flow: Id,

    /// Per transaction step, in flow order.
    pub transactions: Vec<TransactionRetrySafety>,

    /// Per effect-executing step, in flow order.
    pub effects: Vec<EffectRetrySafety>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionRetrySafety {
    pub transaction: Id,
    pub route: RetryRoute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RetryRoute {
    /// At most one commit per class; re-encounters resolve it.
    KeyedCommit { key: Vec<StableRoot> },

    /// Re-execution reproduces the same logical state.
    NaturalReplay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectRetrySafety {
    /// The executed effect (for intent executions, the intent's
    /// effect's site is recorded by the intent id in `safety`).
    pub effect: Id,

    pub safety: EffectSafety,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EffectSafety {
    /// The external boundary deduplicates executions sharing the
    /// stable key.
    ExternallyDeduplicated { key: Vec<StableRoot> },

    /// Every attempt publishes the same logical message — the instance
    /// is class-fixed and the topic's message identity maps the
    /// schema — and every modeled consumer of it collapses duplicate
    /// deliveries, so the cascade the publication starts performs the
    /// work of one logical invocation everywhere the model can see.
    SameLogicalMessage {
        topic: Id,
        schema: Id,
        instance: InstanceStability,
        consumers: Vec<ConsumerCollapse>,
    },

    /// Every attempt sends payload-equal requests into one class of
    /// the target's proven idempotency requirement.
    DeduplicatedByTarget {
        operation: Id,
        input: Id,
        instance: InstanceStability,
    },
}

/// How a modeled consumer of a published message collapses duplicate
/// deliveries of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConsumerCollapse {
    /// The consumer's idempotency requirement keyed from the
    /// subscription is proven, so payload-equal deliveries fall into
    /// one of its classes.
    ProvenRequirement { operation: Id, input: Id },

    /// The subscription's `at_most_once` delivery bounds one logical
    /// message to at most one delivery, however often it is published.
    SingleDelivery { operation: Id, input: Id },
}

/// Why every attempt constructs the same logical effect instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstanceStability {
    /// A direct execution whose derivation is replay-deterministic
    /// over the cited roots.
    ReplayDeterministic { roots: Vec<StableRoot> },

    /// An intent whose values were fixed at establishment and are
    /// recovered or reconstructed on every attempt.
    EstablishedIntent {
        intent: Id,
        replay: ArtifactReplay,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnstableRoot {
    pub root: ValueRef,
    pub gap: StabilityGap,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IdempotencyObstacle {
    /// The governing key defines no pre-execution equivalence class
    /// (§12).
    GoverningKeyInadmissible { defect: GoverningKeyDefect },

    /// A committed transaction may be re-encountered, and neither
    /// retry route holds.
    TransactionNotRetrySafe {
        flow: Id,
        transaction: Id,
        recovery: Vec<ReplayGap>,
        reconstruction: Vec<ReplayGap>,
    },

    /// The external boundary explicitly does not deduplicate: a
    /// duplicate execution is distinguishable duplicate work (§13.3).
    ExternalEffectNotDeduplicated { flow: Id, effect: Id },

    /// No deduplication fact is available for the external boundary.
    ExternalEffectDeduplicationUnknown { flow: Id, effect: Id },

    /// The declared external deduplication key is not replay-stable,
    /// so attempts may execute under different keys.
    ExternalDeduplicationKeyUnstable {
        flow: Id,
        effect: Id,
        roots: Vec<UnstableRoot>,
    },

    /// Duplicate publications are not established to be the same
    /// logical message: the topic declares no message identity for
    /// the published schema.
    PublicationNotIdentified {
        flow: Id,
        effect: Id,
        topic: Id,
        schema: Id,
    },

    /// A modeled consumer of the published message declares no
    /// idempotency requirement keyed from its subscription, so nothing
    /// collapses the duplicate work a duplicate delivery causes there.
    PublicationConsumerNotKeyed {
        flow: Id,
        effect: Id,
        topic: Id,
        schema: Id,
        operation: Id,
        input: Id,
    },

    /// The consumer declares such a requirement, but it is not proven
    /// in this analysis — including cyclic dependencies, which settle
    /// unproven.
    PublicationConsumerRequirementUnproven {
        flow: Id,
        effect: Id,
        topic: Id,
        schema: Id,
        operation: Id,
        input: Id,
    },

    /// A direct execution declares no instance provenance, so the
    /// instances attempts construct are not class-fixed.
    EffectInstanceUnspecified { flow: Id, effect: Id },

    /// A direct execution's instance derivation depends on unstable
    /// roots.
    EffectInstanceRootUnstable {
        flow: Id,
        effect: Id,
        roots: Vec<UnstableRoot>,
    },

    /// The executed intent is established by no earlier step, so no
    /// class-fixed instance exists to argue about.
    IntentNotEstablished { flow: Id, intent: Id },

    /// The executed intent is replay-available through neither route,
    /// so a retry's instance may differ.
    IntentNotReplayAvailable {
        flow: Id,
        intent: Id,
        transaction: Id,
        recovery: Vec<ReplayGap>,
        reconstruction: Vec<ReplayGap>,
    },

    /// The request effect's schema is not the targeted input's
    /// schema, so payload equality does not transfer.
    RequestSchemaMismatch {
        flow: Id,
        effect: Id,
        expected: Id,
        actual: Id,
    },

    /// The target operation declares no idempotency requirement keyed
    /// from the targeted input, so nothing collapses duplicate
    /// invocations.
    RequestTargetHasNoKeyedRequirement {
        flow: Id,
        effect: Id,
        operation: Id,
        input: Id,
    },

    /// The target declares such a requirement, but it is not proven
    /// in this analysis — including cyclic dependencies, which settle
    /// unproven.
    RequestTargetRequirementUnproven {
        flow: Id,
        effect: Id,
        operation: Id,
        input: Id,
    },
}

/// The effect contract behind an execution site, unifying
/// operation-owned effects and transition side effects.
enum Contract<'a> {
    Publication(&'a PublicationEffect),
    Request(&'a RequestEffect),
    External(&'a ExternalEffect),
}

/// What a check reads beyond the operation under analysis: the model,
/// its trigger graph, and the requirements proven so far in the
/// fixpoint, keyed by operation and triggering input.
struct Scope<'a> {
    model: &'a Model,
    graph: &'a TriggerGraph<'a>,
    proven: &'a BTreeSet<(Id, Id)>,
}

/// Checks every idempotency requirement declared by the model, as a
/// fixpoint over cross-operation discharge through request targets and
/// message consumers.
///
/// The verdicts are the greatest fixpoint: every requirement with an
/// admissible governing key is assumed, and whatever fails under that
/// assumption is dropped until nothing more fails, so a cycle of
/// requirements that each collapse the others' duplicates proves
/// (`ARCHSPEC_EFFECT_SAFETY_DRAFT.md` §4.1). The least fixpoint is
/// computed alongside to mark which proofs rest on such a cycle.
pub fn check(model: &Model) -> Vec<IdempotencyCheck> {
    let graph = TriggerGraph::new(model);

    let least = proven_set(&fixpoint(model, &graph, BTreeSet::new()));

    let every: BTreeSet<(Id, Id)> = model
        .operations
        .iter()
        .flat_map(|(operation, declaration)| {
            declaration
                .requirements
                .idempotency
                .iter()
                .filter_map(move |requirement| {
                    Some((operation.clone(), key_input(&requirement.key)?.clone()))
                })
        })
        .collect();

    let mut checks = fixpoint(model, &graph, every);

    for check in &mut checks {
        if matches!(check.verdict, IdempotencyVerdict::Proven { .. })
            && let Some(input) = key_input(&check.key)
            && !least.contains(&(check.operation.clone(), input.clone()))
        {
            check.coinductive = true;
        }
    }

    checks
}

/// Iterates `run` from `assumed` until the proven set is stable. From
/// the empty set the chain ascends to the least fixpoint; from every
/// admissible requirement it descends to the greatest, since fewer
/// assumptions never prove more.
fn fixpoint(model: &Model, graph: &TriggerGraph<'_>, mut assumed: BTreeSet<(Id, Id)>) -> Vec<IdempotencyCheck> {
    loop {
        let checks = run(&Scope {
            model,
            graph,
            proven: &assumed,
        });

        let next = proven_set(&checks);

        if next == assumed {
            return checks;
        }

        assumed = next;
    }
}

fn proven_set(checks: &[IdempotencyCheck]) -> BTreeSet<(Id, Id)> {
    checks
        .iter()
        .filter(|check| matches!(check.verdict, IdempotencyVerdict::Proven { .. }))
        .filter_map(|check| Some((check.operation.clone(), key_input(&check.key)?.clone())))
        .collect()
}

/// The propagation lineage behind a subscription-triggered governing
/// key: for every admitted schema on the input's topic and every
/// modeled producer of it, whether a declared propagation carries a
/// key onto the identity fields the population rests on.
fn lineage(scope: &Scope<'_>, operation: &Operation, key: &IdempotencyKey) -> Vec<IdentityLineage> {
    let Some(input_id) = key_input(key) else {
        return Vec::new();
    };

    let Some(Input::Subscription(subscription)) = operation.inputs.get(input_id) else {
        return Vec::new();
    };

    let Some(topic) = scope.model.topics.get(&subscription.topic) else {
        return Vec::new();
    };

    let MessageIdentity::Keyed { mapping } = &topic.message_identity else {
        return Vec::new();
    };

    let admitted: Vec<&Id> = match &subscription.messages {
        MessageSelector::All => topic.messages.iter().collect(),
        MessageSelector::Only(schemas) => schemas.iter().collect(),
    };

    let mut out = Vec::new();

    for schema in admitted {
        let Some(identity) = mapping.get(schema) else {
            continue;
        };

        for producer in scope.graph.producers(&subscription.topic, schema) {
            let fact = producer
                .publication
                .idempotency_key_propagation
                .iter()
                .find_map(|propagation| {
                    let targets: Vec<&FieldPath> = propagation
                        .target
                        .components
                        .iter()
                        .filter(|component| {
                            component.source == ValueSource::Effect(producer.effect.clone())
                        })
                        .map(|component| &component.path)
                        .collect();

                    identity
                        .iter()
                        .all(|field| targets.contains(&field))
                        .then(|| {
                            let requirement = match producer.site {
                                ProducerSite::Operation { operation } => scope
                                    .model
                                    .operations
                                    .get(operation)
                                    .and_then(|declaration| {
                                        declaration
                                            .requirements
                                            .idempotency
                                            .iter()
                                            .position(|requirement| {
                                                requirement.key == propagation.source
                                            })
                                    }),

                                ProducerSite::Transition { .. } => None,
                            };

                            LineageFact::Propagated {
                                source: propagation.source.clone(),
                                requirement,
                            }
                        })
                })
                .unwrap_or(LineageFact::Undeclared);

            out.push(IdentityLineage {
                topic: subscription.topic.clone(),
                schema: schema.clone(),
                producer: match producer.site {
                    ProducerSite::Operation { operation } => ProducerRef::Operation {
                        operation: operation.clone(),
                        effect: producer.effect.clone(),
                    },

                    ProducerSite::Transition {
                        machine,
                        transition,
                    } => ProducerRef::Transition {
                        machine: machine.clone(),
                        transition: transition.clone(),
                        effect: producer.effect.clone(),
                    },
                },
                fact,
            });
        }
    }

    out
}

/// The triggering input of an admissible governing key.
fn key_input(key: &IdempotencyKey) -> Option<&Id> {
    match &key.components.first()?.source {
        ValueSource::Input(input) => Some(input),
        _ => None,
    }
}

fn run(scope: &Scope<'_>) -> Vec<IdempotencyCheck> {
    let mut checks = Vec::new();

    for (operation_id, operation) in &scope.model.operations {
        for (index, requirement) in operation.requirements.idempotency.iter().enumerate() {
            checks.push(IdempotencyCheck {
                operation: operation_id.clone(),
                requirement: index,
                key: requirement.key.clone(),
                verdict: check_requirement(scope, operation, &requirement.key),
                coinductive: false,
                lineage: lineage(scope, operation, &requirement.key),
            });
        }
    }

    checks
}

fn check_requirement(
    scope: &Scope<'_>,
    operation: &Operation,
    key: &IdempotencyKey,
) -> IdempotencyVerdict {
    let analysis = match ReplayAnalysis::new(scope.model, operation, key) {
        Ok(analysis) => analysis,

        Err(defect) => {
            return IdempotencyVerdict::Unproven {
                obstacles: vec![IdempotencyObstacle::GoverningKeyInadmissible { defect }],
            };
        }
    };

    if analysis.admits_no_attempts() {
        return IdempotencyVerdict::Proven {
            proof: IdempotencyProof::NoAdmittedInvocations {
                input: analysis.input().clone(),
            },
        };
    }

    // The single-delivery vacuous route: at most one attempt per
    // class can exist.
    if let Some(Input::Subscription(subscription)) = operation.inputs.get(analysis.input())
        && subscription.delivery == DeliverySemantics::AtMostOnce
        && analysis.payload_identified()
    {
        return IdempotencyVerdict::Proven {
            proof: IdempotencyProof::SingleDelivery {
                input: analysis.input().clone(),
                topic: subscription.topic.clone(),
            },
        };
    }

    let admitted: Vec<_> = operation
        .flows
        .iter()
        .filter(|(_, flow)| match &flow.response {
            None => true,

            Some(response) => operation
                .responses
                .get(response)
                .is_none_or(|response| &response.request == analysis.input()),
        })
        .collect();

    if admitted.is_empty() {
        return IdempotencyVerdict::Proven {
            proof: IdempotencyProof::NoAdmittedFlows {
                input: analysis.input().clone(),
            },
        };
    }

    let mut obstacles = Vec::new();
    let mut flows = Vec::new();

    for (flow_id, flow) in admitted {
        if let Some(safety) =
            analyze_flow(scope, &analysis, operation, flow_id, flow, &mut obstacles)
        {
            flows.push(safety);
        }
    }

    if obstacles.is_empty() {
        IdempotencyVerdict::Proven {
            proof: IdempotencyProof::RetrySafeFlows { flows },
        }
    } else {
        IdempotencyVerdict::Unproven { obstacles }
    }
}

fn analyze_flow(
    scope: &Scope<'_>,
    analysis: &ReplayAnalysis<'_>,
    operation: &Operation,
    flow_id: &Id,
    flow: &InvocationFlow,
    obstacles: &mut Vec<IdempotencyObstacle>,
) -> Option<FlowRetrySafety> {
    let before = obstacles.len();

    let mut context: BTreeMap<Id, ArtifactReplay> = BTreeMap::new();
    let mut transactions = Vec::new();
    let mut effects = Vec::new();

    for step in &flow.steps {
        match step {
            FlowStep::Transaction { transaction } => {
                let Some(body) = operation.transactions.get(transaction) else {
                    continue;
                };

                let (recovery, natural) =
                    analysis.apply_transaction(&mut context, transaction, body);

                match (recovery, natural) {
                    (Ok(key), _) => transactions.push(TransactionRetrySafety {
                        transaction: transaction.clone(),
                        route: RetryRoute::KeyedCommit { key },
                    }),

                    (Err(_), Ok(())) => transactions.push(TransactionRetrySafety {
                        transaction: transaction.clone(),
                        route: RetryRoute::NaturalReplay,
                    }),

                    (Err(recovery), Err(reconstruction)) => {
                        obstacles.push(IdempotencyObstacle::TransactionNotRetrySafe {
                            flow: flow_id.clone(),
                            transaction: transaction.clone(),
                            recovery,
                            reconstruction,
                        });
                    }
                }
            }

            FlowStep::ExecuteEffect { effect, values } => {
                let Some(contract) = operation_contract(operation, effect) else {
                    continue;
                };

                let instance = |obstacles: &mut Vec<IdempotencyObstacle>| {
                    direct_instance(analysis, &context, flow_id, effect, values, obstacles)
                };

                if let Some(safety) = contract_safety(
                    scope, analysis, &context, flow_id, effect, &contract, instance, obstacles,
                ) {
                    effects.push(EffectRetrySafety {
                        effect: effect.clone(),
                        safety,
                    });
                }
            }

            FlowStep::ExecuteEffectIntent { intent } => {
                let Some(declaration) = operation.effect_intents.get(intent) else {
                    continue;
                };

                let effect = &declaration.effect;

                let Some(contract) = operation_contract(operation, effect)
                    .or_else(|| transition_contract(scope.model, effect))
                else {
                    continue;
                };

                let instance = |obstacles: &mut Vec<IdempotencyObstacle>| {
                    intent_instance(&context, flow_id, intent, obstacles)
                };

                if let Some(safety) = contract_safety(
                    scope, analysis, &context, flow_id, effect, &contract, instance, obstacles,
                ) {
                    effects.push(EffectRetrySafety {
                        effect: effect.clone(),
                        safety,
                    });
                }
            }
        }
    }

    (obstacles.len() == before).then_some(FlowRetrySafety {
        flow: flow_id.clone(),
        transactions,
        effects,
    })
}

fn operation_contract<'a>(operation: &'a Operation, effect: &Id) -> Option<Contract<'a>> {
    operation.effects.get(effect).map(|effect| match effect {
        Effect::Publication(publication) => Contract::Publication(publication),
        Effect::Request(request) => Contract::Request(request),
        Effect::External(external) => Contract::External(external),
    })
}

fn transition_contract<'a>(model: &'a Model, effect: &Id) -> Option<Contract<'a>> {
    for machine in model.state_machines.values() {
        for transition in machine.transitions.values() {
            if let Some(side_effect) = transition.side_effects.get(effect) {
                return Some(match side_effect {
                    TransitionSideEffect::Publication(publication) => {
                        Contract::Publication(publication)
                    }

                    TransitionSideEffect::Request(request) => Contract::Request(request),
                });
            }
        }
    }

    None
}

/// A direct execution's instance: class-fixed iff its derivation is
/// replay-deterministic.
fn direct_instance(
    analysis: &ReplayAnalysis<'_>,
    context: &BTreeMap<Id, ArtifactReplay>,
    flow: &Id,
    effect: &Id,
    values: &Derivation,
    obstacles: &mut Vec<IdempotencyObstacle>,
) -> Option<InstanceStability> {
    match values {
        Derivation::Unspecified => {
            obstacles.push(IdempotencyObstacle::EffectInstanceUnspecified {
                flow: flow.clone(),
                effect: effect.clone(),
            });

            None
        }

        Derivation::Deterministic { from } => {
            let mut stable = Vec::new();
            let mut unstable = Vec::new();

            for root in from {
                match analysis.root_stability(context, root) {
                    Ok(root) => stable.push(root),

                    Err(gap) => unstable.push(UnstableRoot {
                        root: root.clone(),
                        gap,
                    }),
                }
            }

            if unstable.is_empty() {
                Some(InstanceStability::ReplayDeterministic { roots: stable })
            } else {
                obstacles.push(IdempotencyObstacle::EffectInstanceRootUnstable {
                    flow: flow.clone(),
                    effect: effect.clone(),
                    roots: unstable,
                });

                None
            }
        }
    }
}

/// An intent execution's instance: class-fixed iff the intent is
/// replay-available; its values were fixed at establishment.
fn intent_instance(
    context: &BTreeMap<Id, ArtifactReplay>,
    flow: &Id,
    intent: &Id,
    obstacles: &mut Vec<IdempotencyObstacle>,
) -> Option<InstanceStability> {
    match context.get(intent) {
        None => {
            obstacles.push(IdempotencyObstacle::IntentNotEstablished {
                flow: flow.clone(),
                intent: intent.clone(),
            });

            None
        }

        Some(ArtifactReplay::Unavailable {
            transaction,
            recovery,
            reconstruction,
        }) => {
            obstacles.push(IdempotencyObstacle::IntentNotReplayAvailable {
                flow: flow.clone(),
                intent: intent.clone(),
                transaction: transaction.clone(),
                recovery: recovery.clone(),
                reconstruction: reconstruction.clone(),
            });

            None
        }

        Some(replay) => Some(InstanceStability::EstablishedIntent {
            intent: intent.clone(),
            replay: replay.clone(),
        }),
    }
}

/// The per-kind duplicate-execution judgment. `instance` is invoked
/// only for kinds whose discharge needs a class-fixed instance; an
/// external boundary deduplicates by key alone.
#[allow(clippy::too_many_arguments)]
fn contract_safety(
    scope: &Scope<'_>,
    analysis: &ReplayAnalysis<'_>,
    context: &BTreeMap<Id, ArtifactReplay>,
    flow: &Id,
    effect: &Id,
    contract: &Contract<'_>,
    instance: impl FnOnce(&mut Vec<IdempotencyObstacle>) -> Option<InstanceStability>,
    obstacles: &mut Vec<IdempotencyObstacle>,
) -> Option<EffectSafety> {
    match contract {
        Contract::External(external) => match &external.idempotency {
            IdempotencyGuarantee::DeduplicatedBy { key } => {
                let mut stable = Vec::new();
                let mut unstable = Vec::new();

                for component in &key.components {
                    match analysis.root_stability(context, component) {
                        Ok(root) => stable.push(root),

                        Err(gap) => unstable.push(UnstableRoot {
                            root: component.clone(),
                            gap,
                        }),
                    }
                }

                if unstable.is_empty() {
                    Some(EffectSafety::ExternallyDeduplicated { key: stable })
                } else {
                    obstacles.push(IdempotencyObstacle::ExternalDeduplicationKeyUnstable {
                        flow: flow.clone(),
                        effect: effect.clone(),
                        roots: unstable,
                    });

                    None
                }
            }

            IdempotencyGuarantee::NotDeduplicated => {
                obstacles.push(IdempotencyObstacle::ExternalEffectNotDeduplicated {
                    flow: flow.clone(),
                    effect: effect.clone(),
                });

                None
            }

            IdempotencyGuarantee::Unspecified => {
                obstacles.push(IdempotencyObstacle::ExternalEffectDeduplicationUnknown {
                    flow: flow.clone(),
                    effect: effect.clone(),
                });

                None
            }
        },

        Contract::Publication(publication) => {
            let topic = &publication.topic;
            let schema = &publication.schema;

            let identified = scope
                .model
                .topics
                .get(topic)
                .is_some_and(|topic| match &topic.message_identity {
                    MessageIdentity::Keyed { mapping } => mapping.contains_key(schema),
                    MessageIdentity::Unspecified => false,
                });

            if !identified {
                obstacles.push(IdempotencyObstacle::PublicationNotIdentified {
                    flow: flow.clone(),
                    effect: effect.clone(),
                    topic: topic.clone(),
                    schema: schema.clone(),
                });
            }

            // The cascade: one logical message still reaches every
            // modeled consumer, and the duplicate work it would do
            // there is work this attempt caused. Each consumer must
            // collapse duplicate deliveries — by a proven requirement
            // keyed from the subscription, or by never seeing a second
            // delivery of the one message.
            let mut consumers = Vec::new();
            let mut collapsed = true;

            for consumer in scope.graph.consumers(topic, schema) {
                let operation = consumer.operation.clone();
                let input = consumer.input.clone();

                if identified && consumer.subscription.delivery == DeliverySemantics::AtMostOnce {
                    consumers.push(ConsumerCollapse::SingleDelivery { operation, input });
                } else if !scope
                    .model
                    .operations
                    .get(consumer.operation)
                    .is_some_and(|target| collapses_duplicates(target, consumer.input))
                {
                    collapsed = false;

                    obstacles.push(IdempotencyObstacle::PublicationConsumerNotKeyed {
                        flow: flow.clone(),
                        effect: effect.clone(),
                        topic: topic.clone(),
                        schema: schema.clone(),
                        operation,
                        input,
                    });
                } else if !scope.proven.contains(&(operation.clone(), input.clone())) {
                    collapsed = false;

                    obstacles.push(IdempotencyObstacle::PublicationConsumerRequirementUnproven {
                        flow: flow.clone(),
                        effect: effect.clone(),
                        topic: topic.clone(),
                        schema: schema.clone(),
                        operation,
                        input,
                    });
                } else {
                    consumers.push(ConsumerCollapse::ProvenRequirement { operation, input });
                }
            }

            let instance = instance(obstacles)?;

            (identified && collapsed).then_some(EffectSafety::SameLogicalMessage {
                topic: topic.clone(),
                schema: schema.clone(),
                instance,
                consumers,
            })
        }

        Contract::Request(request) => {
            let target_operation = &request.target.operation;
            let target_input = &request.target.input;

            let mut target_ok = false;

            match scope
                .model
                .operations
                .get(target_operation)
                .and_then(|target| target.inputs.get(target_input).map(|input| (target, input)))
            {
                Some((target, Input::Request(declared))) => {
                    if declared.schema != request.schema {
                        obstacles.push(IdempotencyObstacle::RequestSchemaMismatch {
                            flow: flow.clone(),
                            effect: effect.clone(),
                            expected: declared.schema.clone(),
                            actual: request.schema.clone(),
                        });
                    } else if !collapses_duplicates(target, target_input) {
                        obstacles.push(IdempotencyObstacle::RequestTargetHasNoKeyedRequirement {
                            flow: flow.clone(),
                            effect: effect.clone(),
                            operation: target_operation.clone(),
                            input: target_input.clone(),
                        });
                    } else if !scope
                        .proven
                        .contains(&(target_operation.clone(), target_input.clone()))
                    {
                        obstacles.push(IdempotencyObstacle::RequestTargetRequirementUnproven {
                            flow: flow.clone(),
                            effect: effect.clone(),
                            operation: target_operation.clone(),
                            input: target_input.clone(),
                        });
                    } else {
                        target_ok = true;
                    }
                }

                _ => {
                    obstacles.push(IdempotencyObstacle::RequestTargetHasNoKeyedRequirement {
                        flow: flow.clone(),
                        effect: effect.clone(),
                        operation: target_operation.clone(),
                        input: target_input.clone(),
                    });
                }
            }

            let instance = instance(obstacles)?;

            target_ok.then_some(EffectSafety::DeduplicatedByTarget {
                operation: target_operation.clone(),
                input: target_input.clone(),
                instance,
            })
        }
    }
}

impl IdempotencyCheck {
    /// The diagnostic for an unproven requirement; a proven one
    /// produces none.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        let IdempotencyVerdict::Unproven { obstacles } = &self.verdict else {
            return None;
        };

        let operation = &self.operation;
        let requirement = self.requirement;

        Some(Diagnostic {
            code: DiagnosticCode::Verification(VerificationCode::IdempotencyUnproven),
            severity: Severity::Unknown,
            subject: Some(operation.clone()),
            message: format!(
                "Idempotency requirement {requirement} of `{operation}` is not \
                 established: repeated attempts sharing the declared key are not \
                 proven to avoid externally distinguishable duplicate logical \
                 work."
            ),
            evidence: obstacles.iter().map(IdempotencyObstacle::evidence).collect(),
        })
    }
}

fn unstable_roots(roots: &[UnstableRoot]) -> String {
    roots
        .iter()
        .map(|entry| {
            format!(
                "`{}` ({})",
                entry.root.path,
                stability_sentence(&entry.gap)
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

impl IdempotencyObstacle {
    fn evidence(&self) -> Evidence {
        match self {
            Self::GoverningKeyInadmissible { defect } => governing_key_evidence(defect),

            Self::TransactionNotRetrySafe {
                flow,
                transaction,
                recovery,
                reconstruction,
            } => Evidence {
                subject: Some(transaction.clone()),
                message: format!(
                    "A duplicate attempt re-drives flow `{flow}` and re-encounters \
                     `{transaction}`, which is retry-safe by neither route. \
                     Recovery: {}. Reconstruction: {}.",
                    gap_sentences(recovery),
                    gap_sentences(reconstruction),
                ),
            },

            Self::ExternalEffectNotDeduplicated { flow, effect } => Evidence {
                subject: Some(effect.clone()),
                message: format!(
                    "Flow `{flow}` executes external effect `{effect}`, which is \
                     explicitly `not_deduplicated`: a duplicate execution is \
                     distinguishable duplicate work at that boundary."
                ),
            },

            Self::ExternalEffectDeduplicationUnknown { flow, effect } => Evidence {
                subject: Some(effect.clone()),
                message: format!(
                    "Flow `{flow}` executes external effect `{effect}`, and no \
                     deduplication fact is declared for that boundary."
                ),
            },

            Self::ExternalDeduplicationKeyUnstable { flow, effect, roots } => Evidence {
                subject: Some(effect.clone()),
                message: format!(
                    "External effect `{effect}` in flow `{flow}` deduplicates by a \
                     key that is not replay-stable, so attempts may execute under \
                     different keys: {}.",
                    unstable_roots(roots)
                ),
            },

            Self::PublicationNotIdentified {
                flow,
                effect,
                topic,
                schema,
            } => Evidence {
                subject: Some(effect.clone()),
                message: format!(
                    "Flow `{flow}` publishes `{schema}` to `{topic}` through \
                     `{effect}`, but the topic declares no message identity for \
                     that schema, so duplicate publications are not established \
                     to be the same logical message."
                ),
            },

            Self::PublicationConsumerNotKeyed {
                flow,
                effect,
                topic,
                schema,
                operation,
                input,
            } => Evidence {
                subject: Some(operation.clone()),
                message: format!(
                    "Flow `{flow}` publishes `{schema}` to `{topic}` through \
                     `{effect}`, and `{operation}` consumes it through `{input}` \
                     with no idempotency requirement keyed from that input; \
                     nothing collapses the duplicate work a duplicate delivery \
                     causes there."
                ),
            },

            Self::PublicationConsumerRequirementUnproven {
                flow,
                effect,
                topic,
                schema,
                operation,
                input,
            } => Evidence {
                subject: Some(operation.clone()),
                message: format!(
                    "Flow `{flow}` publishes `{schema}` to `{topic}` through \
                     `{effect}`, and `{operation}` consumes it through `{input}`, \
                     whose idempotency requirement is not proven in this \
                     analysis, so the duplicate work a duplicate delivery causes \
                     there is not established to collapse."
                ),
            },

            Self::EffectInstanceUnspecified { flow, effect } => Evidence {
                subject: Some(effect.clone()),
                message: format!(
                    "Flow `{flow}` executes `{effect}` with unspecified instance \
                     provenance, so the instances attempts construct are not \
                     class-fixed."
                ),
            },

            Self::EffectInstanceRootUnstable { flow, effect, roots } => Evidence {
                subject: Some(effect.clone()),
                message: format!(
                    "The instance `{effect}` constructs in flow `{flow}` depends \
                     on roots that are not replay-stable: {}.",
                    unstable_roots(roots)
                ),
            },

            Self::IntentNotEstablished { flow, intent } => Evidence {
                subject: Some(intent.clone()),
                message: format!(
                    "Flow `{flow}` executes intent `{intent}`, but no earlier step \
                     establishes it, so no class-fixed instance exists."
                ),
            },

            Self::IntentNotReplayAvailable {
                flow,
                intent,
                transaction,
                recovery,
                reconstruction,
            } => Evidence {
                subject: Some(transaction.clone()),
                message: format!(
                    "Intent `{intent}` in flow `{flow}` is established by \
                     `{transaction}` but replay-available through neither route, \
                     so a retry's instance may differ. Recovery: {}. \
                     Reconstruction: {}.",
                    gap_sentences(recovery),
                    gap_sentences(reconstruction),
                ),
            },

            Self::RequestSchemaMismatch {
                flow,
                effect,
                expected,
                actual,
            } => Evidence {
                subject: Some(effect.clone()),
                message: format!(
                    "Request effect `{effect}` in flow `{flow}` declares schema \
                     `{actual}`, but the targeted input declares `{expected}`, so \
                     payload equality does not transfer to the target's key."
                ),
            },

            Self::RequestTargetHasNoKeyedRequirement {
                flow,
                effect,
                operation,
                input,
            } => Evidence {
                subject: Some(effect.clone()),
                message: format!(
                    "Request effect `{effect}` in flow `{flow}` targets \
                     `{operation}` through `{input}`, which carries no idempotency \
                     requirement keyed from that input; nothing collapses \
                     duplicate invocations."
                ),
            },

            Self::RequestTargetRequirementUnproven {
                flow,
                effect,
                operation,
                input,
            } => Evidence {
                subject: Some(operation.clone()),
                message: format!(
                    "Request effect `{effect}` in flow `{flow}` targets \
                     `{operation}` through `{input}`, whose idempotency \
                     requirement is not proven in this analysis, so duplicate \
                     invocations are not established to collapse."
                ),
            },
        }
    }
}
