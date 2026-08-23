//! Verification: discharging declared requirements from declared
//! facts.
//!
//! Validation (`analyzer::validation`) establishes that a model is
//! structurally coherent. Verification establishes whether the
//! requirements the model declares actually follow from its facts and
//! structure — the distinction drawn in §1 of the semantics contract.
//! A requirement is a proof obligation, not a guarantee: declaring it
//! does not assert that the operation already satisfies it (§9).
//!
//! This module is the model checker. It grew one requirement family
//! at a time and now discharges all five of §9: operation
//! serialization (`serialization`), ordering (`ordering`), response
//! replay consistency (`response_replay`), recoverability
//! (`recoverability`), and operation idempotency (`idempotency`). The
//! replay-based three share the replay engine (`replay`): root
//! stability, natural transaction replayability, and artifact replay
//! availability. Verifiers that follow effects into other operations
//! share the trigger graph (`trigger`); ordering rests on the
//! serialization verifier's key identity and on idempotency's
//! verdicts for redelivery. Beyond §9, a model-wide deadlock
//! checker is earmarked (revision draft §27, question 9), gated on
//! the locking facts the DSL cannot yet state (question 8); no
//! verifier here reasons about locks.
//!
//! Two rules govern every verdict:
//!
//! 1. A verdict is proven only when the argument rests entirely on
//!    declared facts. An unknown fact cannot be used as evidence
//!    (§1.1).
//! 2. A requirement that cannot be proven is unproven, never
//!    "violated": absence of a guarantee is not evidence of a
//!    violation (§1.2). An unproven verdict records exactly which
//!    facts are missing or insufficient, preserving the distinction
//!    between an explicitly negative declaration (`unbounded`,
//!    `unconstrained`) and an absent one (`unspecified`).
//!
//! Every proof is conditional (§1.3, §25): it holds only if the
//! concrete implementation conforms to the declarations it cites.
//! Proofs therefore carry the facts they consumed.
//!
//! `verify` expects a model that `validation::validate` accepts. On a
//! model that fails validation it stays total and conservative:
//! lookups that fail produce unproven verdicts, never panics and
//! never unsound proofs.

mod describe;
pub mod idempotency;
pub mod ordering;
pub mod recoverability;
pub mod replay;
pub mod response_replay;
pub mod serialization;
pub mod trigger;
pub mod value_identity;

pub use idempotency::{
    ConsumerCollapse, EffectRetrySafety, EffectSafety, FlowRetrySafety, IdempotencyCheck,
    IdempotencyObstacle, IdempotencyProof, IdempotencyVerdict, IdentityLineage, InstanceStability,
    LineageFact, ProducerRef, RetryRoute, TransactionRetrySafety, UnstableRoot,
};
pub use ordering::{
    DuplicateCoverage, DuplicateHandling, LaneFact, OrderingCheck, OrderingObstacle, OrderingProof,
    OrderingVerdict, PrecedenceSource,
};
pub use recoverability::{
    ArtifactAvailability, FlowResumption, RecoverabilityCheck, RecoverabilityNote,
    RecoverabilityObstacle, RecoverabilityProof, RecoverabilityVerdict, Resolution, RetryDriver,
    TransactionResolution,
};
pub use replay::{
    ArtifactReplay, GoverningKeyDefect, PayloadIdentityGap, ReplayAnalysis, ReplayGap,
    StabilityGap, StabilityRule, StableRoot,
};
pub use response_replay::{
    ResponseReplayCheck, ResponseReplayObstacle, ResponseReplayProof, ResponseReplayVerdict,
    ResponseSite,
};
pub use serialization::{
    KeyIdentity, MessageKeyFact, SerializationCheck, SerializationObstacle, SerializationProof,
    SerializationVerdict,
};
pub use trigger::{Consumer, Producer, ProducerSite, TriggerGraph, collapses_duplicates};
pub use value_identity::{CanonicalValuePath, canonical_value_path};

use crate::analyzer::{Diagnostic, DiagnosticCode, Severity, VerificationCode};
use crate::spec::{DeliverySemantics, Id, Input, Model};

/// A model-wide observation raised next to the verdicts. Not an
/// obligation — no declaration asks for it — but a gap no verdict
/// would otherwise point out.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelNote {
    /// A subscription admits duplicate deliveries — at-least-once or
    /// unspecified delivery — and its operation declares no
    /// idempotency requirement keyed from it. The topic contract
    /// admits the duplicate invocation and its safety is nobody's
    /// obligation, so the work it repeats is checked by nothing.
    DuplicateDeliveryUnchecked {
        operation: Id,
        input: Id,
        topic: Id,
        delivery: DeliverySemantics,
    },
}

impl ModelNote {
    pub fn subject(&self) -> Option<Id> {
        match self {
            Self::DuplicateDeliveryUnchecked { input, .. } => Some(input.clone()),
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::DuplicateDeliveryUnchecked {
                operation,
                input,
                topic,
                delivery,
            } => {
                let admits = match delivery {
                    DeliverySemantics::AtLeastOnce => {
                        "declares at-least-once delivery, so a logical message may invoke it more than once"
                    }
                    _ => "declares no delivery fact, so duplicate invocations cannot be excluded",
                };

                format!(
                    "`{input}` of `{operation}` subscribes to `{topic}` and {admits}; the                      operation declares no idempotency requirement keyed from that input, so                      the work a duplicate delivery repeats is checked by nothing."
                )
            }
        }
    }

    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic {
            code: DiagnosticCode::Verification(VerificationCode::DuplicateDeliveryUnchecked),
            severity: Severity::Warning,
            subject: self.subject(),
            message: self.message(),
            evidence: Vec::new(),
        }
    }
}

/// The model-wide notes: every subscription that admits duplicate
/// deliveries without an idempotency requirement keyed from it.
pub fn notes(model: &Model) -> Vec<ModelNote> {
    let mut notes = Vec::new();

    for (operation_id, operation) in &model.operations {
        for (input_id, input) in &operation.inputs {
            let Input::Subscription(subscription) = input else {
                continue;
            };

            if subscription.delivery == DeliverySemantics::AtMostOnce {
                continue;
            }

            if !collapses_duplicates(operation, input_id) {
                notes.push(ModelNote::DuplicateDeliveryUnchecked {
                    operation: operation_id.clone(),
                    input: input_id.clone(),
                    topic: subscription.topic.clone(),
                    delivery: subscription.delivery,
                });
            }
        }
    }

    notes
}

/// Verdicts for every requirement the model declares, in deterministic
/// model order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReport {
    pub serialization: Vec<SerializationCheck>,
    pub ordering: Vec<OrderingCheck>,
    pub idempotency: Vec<IdempotencyCheck>,
    pub response_replay: Vec<ResponseReplayCheck>,
    pub recoverability: Vec<RecoverabilityCheck>,

    /// Model-wide notes, raised as warnings.
    #[serde(default)]
    pub notes: Vec<ModelNote>,
}

impl VerificationReport {
    /// Diagnostics for the requirements the model does not establish,
    /// and warnings raised next to proven ones.
    ///
    /// A proven requirement's argument lives in its structured
    /// verdict; it produces a diagnostic only for a note worth
    /// raising alongside, such as guaranteed retries whose safety no
    /// requirement declares.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.serialization
            .iter()
            .filter_map(SerializationCheck::diagnostic)
            .chain(self.ordering.iter().filter_map(OrderingCheck::diagnostic))
            .chain(
                self.idempotency
                    .iter()
                    .filter_map(IdempotencyCheck::diagnostic),
            )
            .chain(
                self.response_replay
                    .iter()
                    .filter_map(ResponseReplayCheck::diagnostic),
            )
            .chain(
                self.recoverability
                    .iter()
                    .filter_map(RecoverabilityCheck::diagnostic),
            )
            .chain(
                self.recoverability
                    .iter()
                    .flat_map(RecoverabilityCheck::note_diagnostics),
            )
            .chain(self.notes.iter().map(ModelNote::diagnostic))
            .collect()
    }

    pub fn all_proven(&self) -> bool {
        self.serialization
            .iter()
            .all(|entry| matches!(entry.verdict, SerializationVerdict::Proven { .. }))
            && self
                .ordering
                .iter()
                .all(|entry| matches!(entry.verdict, OrderingVerdict::Proven { .. }))
            && self
                .idempotency
                .iter()
                .all(|entry| matches!(entry.verdict, IdempotencyVerdict::Proven { .. }))
            && self
                .response_replay
                .iter()
                .all(|entry| matches!(entry.verdict, ResponseReplayVerdict::Proven { .. }))
            && self
                .recoverability
                .iter()
                .all(|entry| matches!(entry.verdict, RecoverabilityVerdict::Proven { .. }))
    }
}

/// Verifies every declared requirement the checker currently supports.
pub fn verify(model: &Model) -> VerificationReport {
    let idempotency = idempotency::check(model);
    let ordering = ordering::check(model, &idempotency);

    VerificationReport {
        serialization: serialization::check(model),
        ordering,
        idempotency,
        response_replay: response_replay::check(model),
        recoverability: recoverability::check(model),
        notes: notes(model),
    }
}
