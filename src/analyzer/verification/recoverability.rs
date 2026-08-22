//! Verification of recoverability requirements (§9 of the semantics
//! contract; `ARCHSPEC_FLOW_RESUMPTION_DRAFT.md`).
//!
//! > The logical invocation identified by the key must reach terminal
//! > execution of a declared flow.
//!
//! Recoverability is a progress obligation, deliberately separate
//! from idempotency's safety obligation. V1 discharges it by
//! **same-flow continuation**: for every prefix at which an attempt
//! may fail, re-driving the same flow from its first step reaches the
//! flow's terminal completion. This is a sufficient route that does
//! not prejudge revision question 7 (which *other* flows a resumed
//! attempt may take).
//!
//! The population is the attempt class of the requirement's governing
//! key (§12), and the analyzed flows are those an invocation of the
//! triggering input can complete: flows with no response, or flows
//! whose response is declared for that input.
//!
//! Per admitted flow, three conditions establish the continuation:
//!
//! 1. every committed transaction resolves on re-encounter — by keyed
//!    commit over a stable key, or by natural replay — except a
//!    transaction that is the final step of a response-less flow,
//!    after which no failing prefix exists;
//! 2. every artifact a later step consumes is replay-available by
//!    route A or route B of §17, judged by the replay engine —
//!    including intents the flow executes (which must be established
//!    at all), invocation results referenced by later transaction
//!    bodies or flow-level effect derivations, and the result a
//!    declared response resolves; references within the establishing
//!    transaction itself are exempt by atomicity, and a commit key is
//!    judged by condition 1, not double-counted here;
//! 3. nothing else blocks, by construction: transactions are atomic,
//!    effect executions can be re-attempted (their duplicate-safety
//!    is idempotency's concern), and consumption does not remove an
//!    artifact from the context.
//!
//! Re-executing a committed transaction that resolves by neither
//! route is not a continuation of the same logical invocation, so V1
//! refuses to discharge a progress obligation through it.
//!
//! `completion: guaranteed` additionally requires a modeled retry
//! driver on the triggering input: `at_least_once` delivery on the
//! subscription, or a modeled caller declaring a `may_repeat` request
//! effect targeting the input — whether among an operation's effects
//! or as a state-machine transition side effect (§22). Driver facts
//! are duplicate-delivery facts, not bounded-liveness facts; the
//! proof is conditional on the abstraction genuinely re-driving until
//! success (§1.3).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::analyzer::{Diagnostic, DiagnosticCode, Evidence, Severity, VerificationCode};
use crate::spec::{
    CompletionRequirement, DeliverySemantics, Derivation, Effect, FlowStep, Id, IdempotencyKey,
    Input, InvocationFlow, Model, Operation, ResponseSource, RetrySemantics, TransactionStep,
    TransitionSideEffect, ValueRef, ValueSource,
};

use super::describe::{gap_sentences, governing_key_evidence};
use super::replay::{
    ArtifactReplay, GoverningKeyDefect, ReplayAnalysis, ReplayGap, StableRoot, predicate_roots,
};
use super::trigger::collapses_duplicates;

/// The verdict for one declared recoverability requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverabilityCheck {
    pub operation: Id,

    /// Index into `operation.requirements.recoverability`.
    pub requirement: usize,

    /// The governing key, copied so the check is self-contained.
    pub key: IdempotencyKey,

    pub completion: CompletionRequirement,

    pub verdict: RecoverabilityVerdict,

    /// Facts that do not bear on the verdict but belong next to it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<RecoverabilityNote>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecoverabilityVerdict {
    Proven { proof: RecoverabilityProof },
    Unproven { obstacles: Vec<RecoverabilityObstacle> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecoverabilityProof {
    /// The triggering subscription admits no message schemas, so no
    /// attempt can bear the key.
    NoAdmittedInvocations { input: Id },

    /// Every admitted flow resumes from every failing prefix.
    Resumable { flows: Vec<FlowResumption> },

    /// Resumable, and a modeled driver re-drives interrupted
    /// invocations.
    Guaranteed {
        driver: RetryDriver,
        flows: Vec<FlowResumption>,
    },
}

/// The same-flow continuation argument for one admitted flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlowResumption {
    pub flow: Id,

    /// Re-encounter resolution per transaction step, in flow order.
    pub transactions: Vec<TransactionResolution>,

    /// Every artifact a later step consumes, with its replay route.
    pub artifacts: Vec<ArtifactAvailability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionResolution {
    pub transaction: Id,
    pub resolution: Resolution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Resolution {
    /// Route B: the re-encounter resolves the single keyed commit,
    /// whose key is stable through the cited rules.
    KeyedCommit { key: Vec<StableRoot> },

    /// Route A: the re-encounter safely re-executes the naturally
    /// replayable body.
    NaturalReplay,

    /// The transaction is the final step of a response-less flow: no
    /// failing prefix follows its commit, so it is never
    /// re-encountered by a resumption.
    TerminalStep,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactAvailability {
    pub artifact: Id,
    pub replay: ArtifactReplay,
}

/// The modeled fact that re-drives interrupted invocations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RetryDriver {
    /// The triggering subscription declares at-least-once delivery.
    AtLeastOnceDelivery { input: Id, topic: Id },

    /// A modeled caller declares a repeatable request effect
    /// targeting the triggering input.
    InboundRepeatableRequest { operation: Id, effect: Id },

    /// A state-machine transition side effect is a repeatable request
    /// targeting the triggering input.
    InboundRepeatableTransitionEffect {
        machine: Id,
        transition: Id,
        effect: Id,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecoverabilityObstacle {
    /// The governing key defines no pre-execution equivalence class
    /// (§12).
    GoverningKeyInadmissible { defect: GoverningKeyDefect },

    /// No declared flow is completable by invocations of the
    /// triggering input.
    NoAdmittedFlow { input: Id },

    /// A committed transaction the resumption re-encounters resolves
    /// by neither route.
    TransactionNotResolvable {
        flow: Id,
        transaction: Id,
        recovery: Vec<ReplayGap>,
        reconstruction: Vec<ReplayGap>,
    },

    /// A later step consumes an artifact no earlier step establishes.
    ArtifactNotEstablished {
        flow: Id,
        artifact: Id,
        consumer: Id,
    },

    /// A consumed artifact is replay-available through neither route.
    ArtifactNotReplayAvailable {
        flow: Id,
        artifact: Id,
        transaction: Id,
        recovery: Vec<ReplayGap>,
        reconstruction: Vec<ReplayGap>,
    },

    /// Completion is `guaranteed`, but no modeled driver re-drives
    /// interrupted invocations of the triggering input.
    NoModeledRetryDriver {
        input: Id,

        /// The subscription's declared delivery, or `None` for a
        /// request input with no modeled repeatable caller.
        delivery: Option<DeliverySemantics>,
    },
}

/// A fact that does not bear on a verdict but belongs next to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecoverabilityNote {
    /// Completion is guaranteed by a retry driver, so repeated
    /// attempts are expected — yet the operation declares no
    /// idempotency requirement keyed from the triggering input, so
    /// the safety of those retries is undeclared and unverified.
    /// Recoverability discharges progress only (§9); this is where a
    /// reader would look for the safety half.
    RetrySafetyUndeclared { input: Id },
}

/// Checks every recoverability requirement declared by the model.
pub fn check(model: &Model) -> Vec<RecoverabilityCheck> {
    let mut checks = Vec::new();

    for (operation_id, operation) in &model.operations {
        for (index, requirement) in operation.requirements.recoverability.iter().enumerate() {
            let (verdict, notes) = check_requirement(
                model,
                operation_id,
                operation,
                &requirement.key,
                requirement.completion,
            );

            checks.push(RecoverabilityCheck {
                operation: operation_id.clone(),
                requirement: index,
                key: requirement.key.clone(),
                completion: requirement.completion,
                verdict,
                notes,
            });
        }
    }

    checks
}

fn check_requirement(
    model: &Model,
    operation_id: &Id,
    operation: &Operation,
    key: &IdempotencyKey,
    completion: CompletionRequirement,
) -> (RecoverabilityVerdict, Vec<RecoverabilityNote>) {
    let analysis = match ReplayAnalysis::new(model, operation, key) {
        Ok(analysis) => analysis,

        Err(defect) => {
            return (
                RecoverabilityVerdict::Unproven {
                    obstacles: vec![RecoverabilityObstacle::GoverningKeyInadmissible { defect }],
                },
                Vec::new(),
            );
        }
    };

    if analysis.admits_no_attempts() {
        return (
            RecoverabilityVerdict::Proven {
                proof: RecoverabilityProof::NoAdmittedInvocations {
                    input: analysis.input().clone(),
                },
            },
            Vec::new(),
        );
    }

    // Flows an invocation of the triggering input can complete.
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

    let mut obstacles = Vec::new();

    if admitted.is_empty() {
        obstacles.push(RecoverabilityObstacle::NoAdmittedFlow {
            input: analysis.input().clone(),
        });
    }

    let mut flows = Vec::new();

    for (flow_id, flow) in admitted {
        if let Some(resumption) = analyze_flow(&analysis, operation, flow_id, flow, &mut obstacles)
        {
            flows.push(resumption);
        }
    }

    let driver = match completion {
        CompletionRequirement::Resumable => None,

        CompletionRequirement::Guaranteed => {
            match find_driver(model, operation_id, operation, analysis.input()) {
                Ok(driver) => Some(driver),

                Err(delivery) => {
                    obstacles.push(RecoverabilityObstacle::NoModeledRetryDriver {
                        input: analysis.input().clone(),
                        delivery,
                    });

                    None
                }
            }
        }
    };

    if !obstacles.is_empty() {
        return (RecoverabilityVerdict::Unproven { obstacles }, Vec::new());
    }

    // A driver makes retries expected. Recoverability says nothing
    // about their safety, so when no idempotency requirement keyed
    // from the triggering input does either, say so next to the proof.
    let mut notes = Vec::new();

    if driver.is_some() && !collapses_duplicates(operation, analysis.input()) {
        notes.push(RecoverabilityNote::RetrySafetyUndeclared {
            input: analysis.input().clone(),
        });
    }

    (
        RecoverabilityVerdict::Proven {
            proof: match driver {
                None => RecoverabilityProof::Resumable { flows },
                Some(driver) => RecoverabilityProof::Guaranteed { driver, flows },
            },
        },
        notes,
    )
}

/// The same-flow continuation analysis for one admitted flow. Returns
/// the resumption argument, or `None` after pushing the flow's
/// obstacles.
fn analyze_flow(
    analysis: &ReplayAnalysis<'_>,
    operation: &Operation,
    flow_id: &Id,
    flow: &InvocationFlow,
    obstacles: &mut Vec<RecoverabilityObstacle>,
) -> Option<FlowResumption> {
    let before = obstacles.len();

    let mut context: BTreeMap<Id, ArtifactReplay> = BTreeMap::new();
    let mut transactions = Vec::new();
    let mut artifacts = Vec::new();
    let mut reported: BTreeSet<Id> = BTreeSet::new();

    let response = flow
        .response
        .as_ref()
        .and_then(|id| operation.responses.get(id).map(|response| (id, response)));

    let last = flow.steps.len().saturating_sub(1);

    for (position, step) in flow.steps.iter().enumerate() {
        match step {
            FlowStep::Transaction { transaction } => {
                let Some(body) = operation.transactions.get(transaction) else {
                    continue;
                };

                // Cross-step consumption is judged against the
                // context before this transaction; same-transaction
                // references are exempt by atomicity.
                let mut established_here: BTreeSet<&Id> = BTreeSet::new();

                for inner in &body.steps {
                    for root in step_value_refs(inner) {
                        let ValueSource::InvocationResult(artifact) = &root.source else {
                            continue;
                        };

                        if !established_here.contains(artifact) {
                            require_artifact(
                                flow_id,
                                transaction,
                                artifact,
                                &context,
                                &mut artifacts,
                                &mut reported,
                                obstacles,
                            );
                        }
                    }

                    if let TransactionStep::EstablishInvocationResult(establish) = inner {
                        established_here.insert(&establish.result);
                    }
                }

                let (recovery, natural) =
                    analysis.apply_transaction(&mut context, transaction, body);

                // A transaction that is the final step of a
                // response-less flow has no failing prefix after it.
                if position < last || response.is_some() {
                    let resolution = match (recovery, natural) {
                        (Ok(key), _) => Some(Resolution::KeyedCommit { key }),

                        (Err(_), Ok(())) => Some(Resolution::NaturalReplay),

                        (Err(recovery), Err(reconstruction)) => {
                            obstacles.push(RecoverabilityObstacle::TransactionNotResolvable {
                                flow: flow_id.clone(),
                                transaction: transaction.clone(),
                                recovery,
                                reconstruction,
                            });

                            None
                        }
                    };

                    if let Some(resolution) = resolution {
                        transactions.push(TransactionResolution {
                            transaction: transaction.clone(),
                            resolution,
                        });
                    }
                } else {
                    transactions.push(TransactionResolution {
                        transaction: transaction.clone(),
                        resolution: Resolution::TerminalStep,
                    });
                }
            }

            FlowStep::ExecuteEffect { effect, values } => {
                let Derivation::Deterministic { from } = values else {
                    continue;
                };

                for root in from {
                    if let ValueSource::InvocationResult(artifact) = &root.source {
                        require_artifact(
                            flow_id,
                            effect,
                            artifact,
                            &context,
                            &mut artifacts,
                            &mut reported,
                            obstacles,
                        );
                    }
                }
            }

            FlowStep::ExecuteEffectIntent { intent } => {
                require_artifact(
                    flow_id,
                    intent,
                    intent,
                    &context,
                    &mut artifacts,
                    &mut reported,
                    obstacles,
                );
            }
        }
    }

    if let Some((response_id, response)) = response
        && let ResponseSource::InvocationResult { result } = &response.source
    {
        require_artifact(
            flow_id,
            response_id,
            result,
            &context,
            &mut artifacts,
            &mut reported,
            obstacles,
        );
    }

    (obstacles.len() == before).then_some(FlowResumption {
        flow: flow_id.clone(),
        transactions,
        artifacts,
    })
}

/// Records a consumed artifact's availability, or the obstacle
/// explaining why the resumption cannot supply it. Each artifact is
/// judged once per flow.
fn require_artifact(
    flow: &Id,
    consumer: &Id,
    artifact: &Id,
    context: &BTreeMap<Id, ArtifactReplay>,
    artifacts: &mut Vec<ArtifactAvailability>,
    reported: &mut BTreeSet<Id>,
    obstacles: &mut Vec<RecoverabilityObstacle>,
) {
    if !reported.insert(artifact.clone()) {
        return;
    }

    match context.get(artifact) {
        None => obstacles.push(RecoverabilityObstacle::ArtifactNotEstablished {
            flow: flow.clone(),
            artifact: artifact.clone(),
            consumer: consumer.clone(),
        }),

        Some(ArtifactReplay::Unavailable {
            transaction,
            recovery,
            reconstruction,
        }) => obstacles.push(RecoverabilityObstacle::ArtifactNotReplayAvailable {
            flow: flow.clone(),
            artifact: artifact.clone(),
            transaction: transaction.clone(),
            recovery: recovery.clone(),
            reconstruction: reconstruction.clone(),
        }),

        Some(replay) => artifacts.push(ArtifactAvailability {
            artifact: artifact.clone(),
            replay: replay.clone(),
        }),
    }
}

/// Every `ValueRef` a transaction step evaluates, except the commit
/// key, which the re-encounter analysis judges.
fn step_value_refs(step: &TransactionStep) -> Vec<&ValueRef> {
    match step {
        TransactionStep::Read(read) => predicate_roots(&read.target.predicate),

        TransactionStep::Write(write) => {
            let mut refs = predicate_roots(&write.target.predicate);

            refs.extend(derivation_refs(&write.values));

            refs
        }

        TransactionStep::Insert(insert) => derivation_refs(&insert.values),

        TransactionStep::Delete(delete) => predicate_roots(&delete.target.predicate),

        TransactionStep::Lock(lock) => predicate_roots(&lock.target.predicate),

        TransactionStep::Transition(transition) => {
            let mut refs = predicate_roots(&transition.subject.predicate);

            for values in transition.effect_values.values() {
                refs.extend(derivation_refs(values));
            }

            refs
        }

        TransactionStep::EstablishEffectIntent(establish) => derivation_refs(&establish.values),

        TransactionStep::EstablishInvocationResult(establish) => {
            derivation_refs(&establish.values)
        }
    }
}

fn derivation_refs(derivation: &Derivation) -> Vec<&ValueRef> {
    match derivation {
        Derivation::Unspecified => Vec::new(),
        Derivation::Deterministic { from } => from.iter().collect(),
    }
}

/// The modeled retry driver for the triggering input, or the declared
/// delivery fact that fails to be one.
fn find_driver(
    model: &Model,
    operation_id: &Id,
    operation: &Operation,
    input: &Id,
) -> Result<RetryDriver, Option<DeliverySemantics>> {
    match operation.inputs.get(input) {
        Some(Input::Subscription(subscription)) => {
            if subscription.delivery == DeliverySemantics::AtLeastOnce {
                Ok(RetryDriver::AtLeastOnceDelivery {
                    input: input.clone(),
                    topic: subscription.topic.clone(),
                })
            } else {
                Err(Some(subscription.delivery))
            }
        }

        Some(Input::Request(_)) => {
            for (caller_id, caller) in &model.operations {
                for (effect_id, effect) in &caller.effects {
                    if let Effect::Request(request) = effect
                        && &request.target.operation == operation_id
                        && &request.target.input == input
                        && request.retry == RetrySemantics::MayRepeat
                    {
                        return Ok(RetryDriver::InboundRepeatableRequest {
                            operation: caller_id.clone(),
                            effect: effect_id.clone(),
                        });
                    }
                }
            }

            for (machine_id, machine) in &model.state_machines {
                for (transition_id, transition) in &machine.transitions {
                    for (effect_id, effect) in &transition.side_effects {
                        if let TransitionSideEffect::Request(request) = effect
                            && &request.target.operation == operation_id
                            && &request.target.input == input
                            && request.retry == RetrySemantics::MayRepeat
                        {
                            return Ok(RetryDriver::InboundRepeatableTransitionEffect {
                                machine: machine_id.clone(),
                                transition: transition_id.clone(),
                                effect: effect_id.clone(),
                            });
                        }
                    }
                }
            }

            Err(None)
        }

        None => Err(None),
    }
}

impl RecoverabilityNote {
    pub fn evidence(&self) -> Evidence {
        match self {
            Self::RetrySafetyUndeclared { input } => Evidence {
                subject: Some(input.clone()),
                message: format!(
                    "Completion is guaranteed by re-driving invocations of \
                     `{input}`, so repeated attempts are expected, but no \
                     idempotency requirement keyed from `{input}` declares them \
                     safe; recoverability establishes progress only."
                ),
            },
        }
    }

    fn diagnostic(&self, operation: &Id, requirement: usize) -> Diagnostic {
        match self {
            Self::RetrySafetyUndeclared { input } => Diagnostic {
                code: DiagnosticCode::Verification(
                    VerificationCode::RecoverabilityRetrySafetyUndeclared,
                ),
                severity: Severity::Warning,
                subject: Some(operation.clone()),
                message: format!(
                    "Recoverability requirement {requirement} of `{operation}` \
                     guarantees completion through retries, but `{operation}` \
                     declares no idempotency requirement keyed from `{input}`: \
                     the retries are expected, and their safety is undeclared \
                     and unverified."
                ),
                evidence: vec![self.evidence()],
            },
        }
    }
}

impl RecoverabilityCheck {
    /// Warnings worth raising next to a proven verdict.
    pub fn note_diagnostics(&self) -> Vec<Diagnostic> {
        self.notes
            .iter()
            .map(|note| note.diagnostic(&self.operation, self.requirement))
            .collect()
    }

    /// The diagnostic for an unproven requirement; a proven one
    /// produces none.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        let RecoverabilityVerdict::Unproven { obstacles } = &self.verdict else {
            return None;
        };

        let operation = &self.operation;
        let requirement = self.requirement;

        let completion = match self.completion {
            CompletionRequirement::Resumable => "resumable",
            CompletionRequirement::Guaranteed => "guaranteed",
        };

        Some(Diagnostic {
            code: DiagnosticCode::Verification(VerificationCode::RecoverabilityUnproven),
            severity: Severity::Unknown,
            subject: Some(operation.clone()),
            message: format!(
                "Recoverability requirement {requirement} of `{operation}` \
                 (`{completion}`) is not established: interrupted invocations \
                 sharing the declared key are not proven to reach terminal \
                 execution of a declared flow."
            ),
            evidence: obstacles
                .iter()
                .map(RecoverabilityObstacle::evidence)
                .collect(),
        })
    }
}

impl RecoverabilityObstacle {
    fn evidence(&self) -> Evidence {
        match self {
            Self::GoverningKeyInadmissible { defect } => governing_key_evidence(defect),

            Self::NoAdmittedFlow { input } => Evidence {
                subject: Some(input.clone()),
                message: format!(
                    "No declared flow is completable by invocations of `{input}`: \
                     every flow terminates with another input's response."
                ),
            },

            Self::TransactionNotResolvable {
                flow,
                transaction,
                recovery,
                reconstruction,
            } => Evidence {
                subject: Some(transaction.clone()),
                message: format!(
                    "Resuming flow `{flow}` re-encounters `{transaction}` after \
                     it may have committed, but the commit resolves by neither \
                     route. Recovery: {}. Reconstruction: {}.",
                    gap_sentences(recovery),
                    gap_sentences(reconstruction),
                ),
            },

            Self::ArtifactNotEstablished {
                flow,
                artifact,
                consumer,
            } => Evidence {
                subject: Some(artifact.clone()),
                message: format!(
                    "Flow `{flow}` consumes artifact `{artifact}` at `{consumer}`, \
                     but no earlier step establishes it."
                ),
            },

            Self::ArtifactNotReplayAvailable {
                flow,
                artifact,
                transaction,
                recovery,
                reconstruction,
            } => Evidence {
                subject: Some(transaction.clone()),
                message: format!(
                    "Flow `{flow}` consumes artifact `{artifact}`, established by \
                     `{transaction}`, but a resumption can supply it through \
                     neither route. Recovery: {}. Reconstruction: {}.",
                    gap_sentences(recovery),
                    gap_sentences(reconstruction),
                ),
            },

            Self::NoModeledRetryDriver { input, delivery } => Evidence {
                subject: Some(input.clone()),
                message: match delivery {
                    Some(DeliverySemantics::AtMostOnce) => format!(
                        "Completion is guaranteed, but subscription `{input}` \
                         declares `at_most_once` delivery: interrupted \
                         invocations are not redelivered."
                    ),

                    Some(_) => format!(
                        "Completion is guaranteed, but subscription `{input}` \
                         declares no delivery fact that re-drives interrupted \
                         invocations."
                    ),

                    None => format!(
                        "Completion is guaranteed, but request input `{input}` \
                         has no modeled caller declaring a `may_repeat` request \
                         effect, so nothing in the model re-drives interrupted \
                         invocations."
                    ),
                },
            },
        }
    }
}
