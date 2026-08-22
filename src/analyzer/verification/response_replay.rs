//! Verification of response replay consistency (§9, §15, §18 of the
//! semantics contract; revision §18).
//!
//! When an idempotency requirement declares
//! `response: replay_consistent`:
//!
//! > Retries for the same logical invocation must resolve the same
//! > logical response.
//!
//! The population is the attempt class defined by the requirement's
//! governing key (§12). The obligation constrains the responses those
//! attempts resolve: the flows terminating with a response declared
//! for the triggering request input. A key triggered by a
//! subscription, or an operation whose admitted flows resolve no such
//! response, leaves nothing to stabilize and is vacuously consistent.
//!
//! For each admitted response site the proof reduces to artifact
//! replay (§17): a response sourced from an invocation result is
//! class-fixed exactly when that result is replay-available at the end
//! of the flow — recovered from a keyed commit over a stable key, or
//! reconstructed by a naturally replayable transaction with a
//! replay-deterministic derivation. A response whose source is
//! `unspecified` supports no proof (§15).
//!
//! When more than one flow resolves a response, V1 requires every site
//! to resolve the same result through the same establishing
//! transaction and the same replay route; equal routes fix one value
//! for the whole class regardless of the flow taken. Differing routes
//! leave cross-flow consistency unproven — which flows a resumed
//! attempt may take is revision question 7, and this checker does not
//! prejudge it.

use serde::{Deserialize, Serialize};

use crate::analyzer::{Diagnostic, DiagnosticCode, Evidence, Severity, VerificationCode};
use crate::spec::{
    Id, IdempotencyKey, Model, Operation, ResponseReplayRequirement, ResponseSource,
};

use super::describe::{gap_sentences, governing_key_evidence};
use super::replay::{ArtifactReplay, GoverningKeyDefect, ReplayAnalysis, ReplayGap};

/// The verdict for one declared response-replay obligation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseReplayCheck {
    pub operation: Id,

    /// Index into `operation.requirements.idempotency`.
    pub requirement: usize,

    /// The governing key, copied so the check is self-contained.
    pub key: IdempotencyKey,

    pub verdict: ResponseReplayVerdict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseReplayVerdict {
    Proven { proof: ResponseReplayProof },
    Unproven { obstacles: Vec<ResponseReplayObstacle> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseReplayProof {
    /// The triggering subscription admits no message schemas, so no
    /// attempt can bear the key.
    NoAdmittedInvocations { input: Id },

    /// No admitted flow resolves a response declared for the
    /// triggering input; there is no response to stabilize.
    NoResolvedResponse { input: Id },

    /// Every admitted response site resolves the same invocation
    /// result through the same establishing transaction and replay
    /// route, so the class observes one logical response.
    ClassFixedResult {
        result: Id,
        transaction: Id,
        replay: ArtifactReplay,
        flows: Vec<Id>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseReplayObstacle {
    /// The governing key defines no pre-execution equivalence class
    /// (§12).
    GoverningKeyInadmissible { defect: GoverningKeyDefect },

    /// The flow's response declares `source: unspecified`; no
    /// replay-consistency proof may be derived from it (§15).
    ResponseSourceUnspecified { flow: Id, response: Id },

    /// The flow resolves its response from a result no step of the
    /// flow establishes.
    ResultNotEstablished { flow: Id, response: Id, result: Id },

    /// The result is established, but replay-available through
    /// neither §17 route.
    ResultNotReplayAvailable {
        flow: Id,
        result: Id,
        transaction: Id,
        recovery: Vec<ReplayGap>,
        reconstruction: Vec<ReplayGap>,
    },

    /// Admitted flows resolve the response through different results,
    /// transactions, or replay routes, so no single class-fixed
    /// response is established.
    DivergentResponseSites { sites: Vec<ResponseSite> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResponseSite {
    pub flow: Id,
    pub result: Id,
    pub transaction: Id,
}

/// Checks every declared response-replay obligation in the model.
pub fn check(model: &Model) -> Vec<ResponseReplayCheck> {
    let mut checks = Vec::new();

    for (operation_id, operation) in &model.operations {
        for (index, requirement) in operation.requirements.idempotency.iter().enumerate() {
            if requirement.response != ResponseReplayRequirement::ReplayConsistent {
                continue;
            }

            checks.push(ResponseReplayCheck {
                operation: operation_id.clone(),
                requirement: index,
                key: requirement.key.clone(),
                verdict: check_requirement(model, operation, &requirement.key),
            });
        }
    }

    checks
}

fn check_requirement(
    model: &Model,
    operation: &Operation,
    key: &IdempotencyKey,
) -> ResponseReplayVerdict {
    let analysis = match ReplayAnalysis::new(model, operation, key) {
        Ok(analysis) => analysis,

        Err(defect) => {
            return ResponseReplayVerdict::Unproven {
                obstacles: vec![ResponseReplayObstacle::GoverningKeyInadmissible { defect }],
            };
        }
    };

    if analysis.admits_no_attempts() {
        return ResponseReplayVerdict::Proven {
            proof: ResponseReplayProof::NoAdmittedInvocations {
                input: analysis.input().clone(),
            },
        };
    }

    // The admitted response sites: flows terminating with a response
    // declared for the triggering input.
    let sites: Vec<_> = operation
        .flows
        .iter()
        .filter_map(|(flow_id, flow)| {
            let response_id = flow.response.as_ref()?;

            let response = operation.responses.get(response_id)?;

            (&response.request == analysis.input()).then_some((flow_id, flow, response_id, response))
        })
        .collect();

    if sites.is_empty() {
        return ResponseReplayVerdict::Proven {
            proof: ResponseReplayProof::NoResolvedResponse {
                input: analysis.input().clone(),
            },
        };
    }

    let mut obstacles = Vec::new();
    let mut resolved: Vec<(Id, Id, ArtifactReplay)> = Vec::new();

    for (flow_id, flow, response_id, response) in sites {
        let ResponseSource::InvocationResult { result } = &response.source else {
            obstacles.push(ResponseReplayObstacle::ResponseSourceUnspecified {
                flow: flow_id.clone(),
                response: response_id.clone(),
            });

            continue;
        };

        let mut context = analysis.flow_artifacts(flow);

        match context.remove(result) {
            None => obstacles.push(ResponseReplayObstacle::ResultNotEstablished {
                flow: flow_id.clone(),
                response: response_id.clone(),
                result: result.clone(),
            }),

            Some(ArtifactReplay::Unavailable {
                transaction,
                recovery,
                reconstruction,
            }) => obstacles.push(ResponseReplayObstacle::ResultNotReplayAvailable {
                flow: flow_id.clone(),
                result: result.clone(),
                transaction,
                recovery,
                reconstruction,
            }),

            Some(replay) => resolved.push((flow_id.clone(), result.clone(), replay)),
        }
    }

    if !obstacles.is_empty() {
        return ResponseReplayVerdict::Unproven { obstacles };
    }

    // Cross-flow consistency: every site must fix the same value,
    // which V1 establishes only through identical routes.
    let (_, first_result, first_replay) = &resolved[0];

    let consistent = resolved
        .iter()
        .all(|(_, result, replay)| result == first_result && replay == first_replay);

    if !consistent {
        return ResponseReplayVerdict::Unproven {
            obstacles: vec![ResponseReplayObstacle::DivergentResponseSites {
                sites: resolved
                    .iter()
                    .map(|(flow, result, replay)| ResponseSite {
                        flow: flow.clone(),
                        result: result.clone(),
                        transaction: replay.transaction().clone(),
                    })
                    .collect(),
            }],
        };
    }

    ResponseReplayVerdict::Proven {
        proof: ResponseReplayProof::ClassFixedResult {
            result: first_result.clone(),
            transaction: first_replay.transaction().clone(),
            replay: first_replay.clone(),
            flows: resolved.iter().map(|(flow, ..)| flow.clone()).collect(),
        },
    }
}

impl ResponseReplayCheck {
    /// The diagnostic for an unproven obligation; a proven one
    /// produces none.
    pub fn diagnostic(&self) -> Option<Diagnostic> {
        let ResponseReplayVerdict::Unproven { obstacles } = &self.verdict else {
            return None;
        };

        let operation = &self.operation;
        let requirement = self.requirement;

        Some(Diagnostic {
            code: DiagnosticCode::Verification(VerificationCode::ResponseReplayUnproven),
            severity: Severity::Unknown,
            subject: Some(operation.clone()),
            message: format!(
                "Response replay for idempotency requirement {requirement} of \
                 `{operation}` is not established: retries sharing the declared \
                 key are not proven to resolve the same logical response."
            ),
            evidence: obstacles.iter().map(ResponseReplayObstacle::evidence).collect(),
        })
    }
}

impl ResponseReplayObstacle {
    fn evidence(&self) -> Evidence {
        match self {
            Self::GoverningKeyInadmissible { defect } => governing_key_evidence(defect),

            Self::ResponseSourceUnspecified { flow, response } => Evidence {
                subject: Some(response.clone()),
                message: format!(
                    "Flow `{flow}` terminates with response `{response}`, whose \
                     source is `unspecified`; no replay-consistency proof may be \
                     derived solely from the response declaration."
                ),
            },

            Self::ResultNotEstablished { flow, result, .. } => Evidence {
                subject: Some(result.clone()),
                message: format!(
                    "Flow `{flow}` resolves its response from result `{result}`, \
                     but no step of the flow establishes it."
                ),
            },

            Self::ResultNotReplayAvailable {
                flow,
                result,
                transaction,
                recovery,
                reconstruction,
            } => Evidence {
                subject: Some(transaction.clone()),
                message: format!(
                    "In flow `{flow}`, result `{result}` is established by \
                     `{transaction}`, but replay-available through neither route. \
                     Recovery: {}. Reconstruction: {}.",
                    gap_sentences(recovery),
                    gap_sentences(reconstruction),
                ),
            },

            Self::DivergentResponseSites { sites } => {
                let rendered = sites
                    .iter()
                    .map(|site| {
                        format!(
                            "flow `{}` resolves `{}` via `{}`",
                            site.flow, site.result, site.transaction
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");

                Evidence {
                    subject: None,
                    message: format!(
                        "The admitted flows resolve the response through different \
                         establishing routes, so no single class-fixed response is \
                         established: {rendered}."
                    ),
                }
            }
        }
    }
}

