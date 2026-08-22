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
//! This module is the model checker. It grows one requirement family
//! at a time, and currently discharges four: operation serialization
//! (`serialization`), response replay consistency (`response_replay`),
//! recoverability (`recoverability`), and operation idempotency
//! (`idempotency`). The latter three share the replay engine
//! (`replay`): root stability, natural transaction replayability, and
//! artifact replay availability. Verifiers that follow effects into
//! other operations share the trigger graph (`trigger`). Of the §9
//! requirement families only ordering remains, pending its
//! precedence-source semantics. Beyond §9, a model-wide deadlock
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
pub mod recoverability;
pub mod replay;
pub mod response_replay;
pub mod serialization;
pub mod trigger;
pub mod value_identity;

pub use idempotency::{
    ConsumerCollapse, EffectRetrySafety, EffectSafety, FlowRetrySafety, IdempotencyCheck,
    IdempotencyObstacle, IdempotencyProof, IdempotencyVerdict, InstanceStability, RetryRoute,
    TransactionRetrySafety, UnstableRoot,
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
pub use trigger::{Consumer, TriggerGraph, collapses_duplicates};
pub use value_identity::{CanonicalValuePath, canonical_value_path};

use crate::analyzer::Diagnostic;
use crate::spec::Model;

/// Verdicts for every requirement the model declares, in deterministic
/// model order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationReport {
    pub serialization: Vec<SerializationCheck>,
    pub idempotency: Vec<IdempotencyCheck>,
    pub response_replay: Vec<ResponseReplayCheck>,
    pub recoverability: Vec<RecoverabilityCheck>,
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
            .collect()
    }

    pub fn all_proven(&self) -> bool {
        self.serialization
            .iter()
            .all(|entry| matches!(entry.verdict, SerializationVerdict::Proven { .. }))
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
    VerificationReport {
        serialization: serialization::check(model),
        idempotency: idempotency::check(model),
        response_replay: response_replay::check(model),
        recoverability: recoverability::check(model),
    }
}
