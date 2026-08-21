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
//!   stable key; a publication by being the *same logical message* —
//!   a class-fixed instance published to a topic whose message
//!   identity maps the schema; a request by targeting an input whose
//!   operation carries a proven idempotency requirement keyed from
//!   that input, fed by a class-fixed instance.
//!
//! Request discharge makes verdicts mutually dependent, so `check`
//! computes a least fixpoint: requirements are re-checked as their
//! request targets become proven, and cyclic dependencies settle
//! unproven — the conservative answer.
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
    DeliverySemantics, Derivation, Effect, ExternalEffect, FlowStep, Id, IdempotencyGuarantee,
    IdempotencyKey, Input, InvocationFlow, MessageIdentity, Model, Operation, PublicationEffect,
    RequestEffect, TransitionSideEffect, ValueRef, ValueSource,
};

use super::describe::{gap_sentences, governing_key_evidence, stability_sentence};
use super::replay::{
    ArtifactReplay, GoverningKeyDefect, ReplayAnalysis, ReplayGap, StabilityGap, StableRoot,
};

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

    /// Every attempt publishes the same logical message: the instance
    /// is class-fixed and the topic's message identity maps the
    /// schema.
    SameLogicalMessage { topic: Id, instance: InstanceStability },

    /// Every attempt sends payload-equal requests into one class of
    /// the target's proven idempotency requirement.
    DeduplicatedByTarget {
        operation: Id,
        input: Id,
        instance: InstanceStability,
    },
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

/// Checks every idempotency requirement declared by the model, as a
/// least fixpoint over cross-operation request discharge.
pub fn check(model: &Model) -> Vec<IdempotencyCheck> {
    let mut proven: BTreeSet<(Id, Id)> = BTreeSet::new();

    loop {
        let checks = run(model, &proven);

        let next: BTreeSet<(Id, Id)> = checks
            .iter()
            .filter(|check| matches!(check.verdict, IdempotencyVerdict::Proven { .. }))
            .filter_map(|check| Some((check.operation.clone(), key_input(&check.key)?.clone())))
            .collect();

        if next == proven {
            return checks;
        }

        proven = next;
    }
}

/// The triggering input of an admissible governing key.
fn key_input(key: &IdempotencyKey) -> Option<&Id> {
    match &key.components.first()?.source {
        ValueSource::Input(input) => Some(input),
        _ => None,
    }
}

fn run(model: &Model, proven: &BTreeSet<(Id, Id)>) -> Vec<IdempotencyCheck> {
    let mut checks = Vec::new();

    for (operation_id, operation) in &model.operations {
        for (index, requirement) in operation.requirements.idempotency.iter().enumerate() {
            checks.push(IdempotencyCheck {
                operation: operation_id.clone(),
                requirement: index,
                key: requirement.key.clone(),
                verdict: check_requirement(model, operation, &requirement.key, proven),
            });
        }
    }

    checks
}

fn check_requirement(
    model: &Model,
    operation: &Operation,
    key: &IdempotencyKey,
    proven: &BTreeSet<(Id, Id)>,
) -> IdempotencyVerdict {
    let analysis = match ReplayAnalysis::new(model, operation, key) {
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
            analyze_flow(model, &analysis, operation, flow_id, flow, proven, &mut obstacles)
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
    model: &Model,
    analysis: &ReplayAnalysis<'_>,
    operation: &Operation,
    flow_id: &Id,
    flow: &InvocationFlow,
    proven: &BTreeSet<(Id, Id)>,
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
                    model, analysis, &context, proven, flow_id, effect, &contract, instance,
                    obstacles,
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
                    .or_else(|| transition_contract(model, effect))
                else {
                    continue;
                };

                let instance = |obstacles: &mut Vec<IdempotencyObstacle>| {
                    intent_instance(&context, flow_id, intent, obstacles)
                };

                if let Some(safety) = contract_safety(
                    model, analysis, &context, proven, flow_id, effect, &contract, instance,
                    obstacles,
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
    model: &Model,
    analysis: &ReplayAnalysis<'_>,
    context: &BTreeMap<Id, ArtifactReplay>,
    proven: &BTreeSet<(Id, Id)>,
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
            let identified = model
                .topics
                .get(&publication.topic)
                .is_some_and(|topic| match &topic.message_identity {
                    MessageIdentity::Keyed { mapping } => {
                        mapping.contains_key(&publication.schema)
                    }

                    MessageIdentity::Unspecified => false,
                });

            if !identified {
                obstacles.push(IdempotencyObstacle::PublicationNotIdentified {
                    flow: flow.clone(),
                    effect: effect.clone(),
                    topic: publication.topic.clone(),
                    schema: publication.schema.clone(),
                });
            }

            let instance = instance(obstacles)?;

            identified.then_some(EffectSafety::SameLogicalMessage {
                topic: publication.topic.clone(),
                instance,
            })
        }

        Contract::Request(request) => {
            let target_operation = &request.target.operation;
            let target_input = &request.target.input;

            let mut target_ok = false;

            match model
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
                    } else if !target.requirements.idempotency.iter().any(|requirement| {
                        !requirement.key.components.is_empty()
                            && requirement.key.components.iter().all(|component| {
                                component.source == ValueSource::Input(target_input.clone())
                            })
                    }) {
                        obstacles.push(IdempotencyObstacle::RequestTargetHasNoKeyedRequirement {
                            flow: flow.clone(),
                            effect: effect.clone(),
                            operation: target_operation.clone(),
                            input: target_input.clone(),
                        });
                    } else if !proven
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
