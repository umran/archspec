//! The replay engine: root stability, natural transaction
//! replayability, and artifact replay availability (§17, §18).
//!
//! Everything here is judged relative to a **governing key** — the
//! `IdempotencyKey` of the obligation under proof — and the attempt
//! population it defines (§12). The three judgments form one
//! simultaneous induction, computed in a single forward pass over a
//! flow: every rule consumes either roots or facts established at
//! earlier steps, and transaction-read dependence, the only
//! backward-looking observation, is excluded outright.
//!
//! ## Root stability
//!
//! A `ValueRef` is replay-stable when any two attempts in the same
//! class that evaluate it obtain equal logical values. The §18 rules:
//! stability is definitional (governing-key components), declared (a
//! request or message identity pinned by the key), or derived
//! (recovered or reconstructed artifacts, congruence). Everything
//! else is a recorded gap, never an assumption.
//!
//! ## Natural replayability
//!
//! V1 proves a transaction naturally replayable only when re-executing
//! its body for the same logical invocation reproduces the same
//! committed state: no `Transition` (revision §22.1), no `Insert`
//! (duplicate-identity insert outcomes are undefined — revision §27
//! question 4), no `Delete` (§20), and every `Write` with a stable
//! target and a replay-deterministic derivation. Reads and locks do
//! not mutate state and do not block the natural route; read-dependent
//! provenance is blocked where it is consumed, because a
//! transaction-read root is never stable.
//!
//! Natural replayability is judged from the transaction-entry artifact
//! context. Artifact derivations see artifacts established earlier in
//! the same transaction, in step order.
//!
//! ## Artifact availability
//!
//! For each artifact a retry may need, availability follows §17:
//! **recovery** (route B) when the establishing transaction declares
//! `deduplicated_by` over a stable key — the single successful commit
//! durably retains the exact artifact, so its derivation needs no
//! determinism at all; otherwise **reconstruction** (route A) when the
//! transaction is naturally replayable and the artifact's derivation
//! is replay-deterministic. Otherwise the artifact is unavailable,
//! with the gaps of both routes recorded.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::spec::{
    Derivation, FieldPath, FlowStep, Id, IdempotencyGuarantee, IdempotencyKey, Input,
    InvocationFlow, MessageIdentity, MessageSelector, Model, Operation, RequestIdentity,
    SelectorPredicate, SelectorValue, Transaction, TransactionStep, ValueRef, ValueSource,
};

use super::value_identity::canonical_value_path;

/// Why a governing key cannot define a pre-execution equivalence
/// class (§12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GoverningKeyDefect {
    /// An empty key places every attempt in one class; essentially
    /// nothing is replay-stable relative to it.
    Empty,

    /// A component is sourced from mutable state or from an artifact
    /// the invocation itself produces.
    ComponentNotFromInput { source: ValueSource },

    /// Components name more than one input; no single triggering
    /// input defines the population.
    ComponentsFromMultipleInputs { first: Id, second: Id },

    /// The named input is not declared by the operation.
    InputNotDeclared { input: Id },
}

/// The rule that established a root as replay-stable. A proof carries
/// these so it states the facts it depends on (§25).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StabilityRule {
    /// Pinned by the governing key: class membership fixes it.
    KeyComponent,

    /// Covered by a declared request or message identity that the
    /// governing key pins; same-class attempts present one logical
    /// stimulus.
    IdentifiedPayload,

    /// A reference into an artifact recovered from the single keyed
    /// commit of the named transaction.
    RecoveredArtifact { transaction: Id },

    /// A reference into an artifact reconstructed by natural replay of
    /// the named transaction.
    ReconstructedArtifact { transaction: Id },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StableRoot {
    pub root: ValueRef,
    pub rule: StabilityRule,
}

/// Why no V1 rule establishes a root as replay-stable. Epistemic
/// (§1.1): instability is not proven.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StabilityGap {
    /// A non-key field of the triggering payload with no identity
    /// coverage.
    UnidentifiedPayloadField {
        input: Id,
        identity: PayloadIdentityGap,
    },

    /// The root belongs to an input that does not trigger the
    /// key-bearing invocations.
    NotTriggeringInput { input: Id },

    /// Mutable persistent state; V1 attempts no invariance analysis.
    MutableSubjectState { machine: Id },

    /// Effect payloads are not stable roots in V1.
    EffectPayloadRoot { effect: Id },

    /// A transaction-read result is never replay-stable (§18).
    TransactionReadRoot { read: Id },

    /// The referenced artifact is established, but replay-available
    /// through neither route.
    ArtifactUnavailable { artifact: Id },

    /// The referenced artifact is not established before this point
    /// of the flow.
    ArtifactNotInContext { artifact: Id },
}

/// Why the declared identities do not make the triggering payload
/// stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PayloadIdentityGap {
    /// No request identity, or no topic message identity, is declared
    /// for the triggering input.
    NotDeclared,

    /// The topic declares message identity, but not for this admitted
    /// schema.
    SchemaNotMapped { schema: Id },

    /// The identity is declared but the governing key does not pin
    /// this identity field in every admitted schema.
    NotPinnedByKey {
        schema: Option<Id>,
        field: FieldPath,
    },
}

/// A missing premise of a replay route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReplayGap {
    /// Route B: the transaction declares no keyed commit
    /// deduplication.
    NoKeyedCommit,

    /// Route B: a commit-key component is not replay-stable, so
    /// attempts may address different commits.
    CommitKeyRootUnstable { root: ValueRef, gap: StabilityGap },

    /// Route A: the transaction applies a state transition, which V1
    /// never replays naturally.
    ContainsTransition,

    /// Route A: the transaction inserts an object; duplicate-identity
    /// insert outcomes are not yet defined (revision §27 question 4).
    ContainsInsert,

    /// Route A: the transaction deletes objects; deletion replay
    /// outcomes are not defined (§20).
    ContainsDelete,

    /// Route A: a mutation target depends on an unstable root.
    MutationTargetRootUnstable { root: ValueRef, gap: StabilityGap },

    /// Route A: a mutation declares no value provenance.
    MutationDerivationUnspecified,

    /// Route A: a mutation value depends on an unstable root.
    MutationDerivationRootUnstable { root: ValueRef, gap: StabilityGap },

    /// Route A: the artifact's establishment declares no value
    /// provenance.
    ArtifactDerivationUnspecified,

    /// Route A: the artifact's values depend on an unstable root.
    ArtifactDerivationRootUnstable { root: ValueRef, gap: StabilityGap },
}

/// How an artifact reaches a retry, or why it does not (§17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactReplay {
    /// Route B: recovered from the single `Commit(T,K)` of the
    /// establishing transaction, whose key is stable through the
    /// cited rules.
    Recovered { transaction: Id, key: Vec<StableRoot> },

    /// Route A: reconstructed by natural replay, with the artifact's
    /// derivation replay-deterministic over the cited roots.
    Reconstructed {
        transaction: Id,
        derivation: Vec<StableRoot>,
    },

    /// Neither route holds; both routes' gaps are recorded.
    Unavailable {
        transaction: Id,
        recovery: Vec<ReplayGap>,
        reconstruction: Vec<ReplayGap>,
    },
}

impl ArtifactReplay {
    pub fn transaction(&self) -> &Id {
        match self {
            Self::Recovered { transaction, .. }
            | Self::Reconstructed { transaction, .. }
            | Self::Unavailable { transaction, .. } => transaction,
        }
    }

    pub fn is_available(&self) -> bool {
        !matches!(self, Self::Unavailable { .. })
    }
}

/// The replay engine for one operation and governing key.
pub struct ReplayAnalysis<'a> {
    model: &'a Model,
    operation: &'a Operation,
    input: &'a Id,
    key: &'a IdempotencyKey,

    /// Payload schemas of the triggering input: the request schema, or
    /// the subscription's admitted message schemas. `None` when the
    /// subscribed topic is unresolvable, in which case only syntactic
    /// path equality is trusted.
    schemas: Option<Vec<&'a Id>>,

    /// Whether the whole triggering payload is stable under a declared
    /// identity pinned by the key (§18 rule 3).
    payload: Result<(), PayloadIdentityGap>,
}

impl<'a> ReplayAnalysis<'a> {
    /// Resolves the governing key (§12): every component must name one
    /// declared input of the operation.
    pub fn new(
        model: &'a Model,
        operation: &'a Operation,
        key: &'a IdempotencyKey,
    ) -> Result<Self, GoverningKeyDefect> {
        let mut input: Option<&Id> = None;

        for component in &key.components {
            let ValueSource::Input(id) = &component.source else {
                return Err(GoverningKeyDefect::ComponentNotFromInput {
                    source: component.source.clone(),
                });
            };

            match input {
                None => input = Some(id),

                Some(first) if first != id => {
                    return Err(GoverningKeyDefect::ComponentsFromMultipleInputs {
                        first: first.clone(),
                        second: id.clone(),
                    });
                }

                Some(_) => {}
            }
        }

        let Some(input) = input else {
            return Err(GoverningKeyDefect::Empty);
        };

        let Some(declaration) = operation.inputs.get(input) else {
            return Err(GoverningKeyDefect::InputNotDeclared {
                input: input.clone(),
            });
        };

        let schemas = match declaration {
            Input::Request(request) => Some(vec![&request.schema]),

            Input::Subscription(subscription) => match &subscription.messages {
                MessageSelector::Only(messages) => Some(messages.iter().collect()),

                MessageSelector::All => model
                    .topics
                    .get(&subscription.topic)
                    .map(|topic| topic.messages.iter().collect()),
            },
        };

        let mut analysis = Self {
            model,
            operation,
            input,
            key,
            schemas,
            payload: Err(PayloadIdentityGap::NotDeclared),
        };

        analysis.payload = analysis.payload_stability(declaration);

        Ok(analysis)
    }

    pub fn input(&self) -> &Id {
        self.input
    }

    /// Whether the whole triggering payload is identity-pinned by the
    /// governing key (§18 rule 3), making same-class stimuli one
    /// logical stimulus.
    pub fn payload_identified(&self) -> bool {
        self.payload.is_ok()
    }

    /// Whether the population is empty by declaration: the triggering
    /// subscription admits no message schemas. An unresolvable topic
    /// leaves the admitted set unknown and must not become a vacuous
    /// proof.
    pub fn admits_no_attempts(&self) -> bool {
        matches!(&self.schemas, Some(schemas) if schemas.is_empty())
    }

    /// §18 rule 3: is every field of the triggering payload stable
    /// under a declared identity pinned by the governing key?
    fn payload_stability(&self, declaration: &Input) -> Result<(), PayloadIdentityGap> {
        match declaration {
            Input::Request(request) => {
                let RequestIdentity::Keyed { fields } = &request.identity else {
                    return Err(PayloadIdentityGap::NotDeclared);
                };

                if fields.is_empty() {
                    return Err(PayloadIdentityGap::NotDeclared);
                }

                for field in fields {
                    if !self.pinned_by_key(field) {
                        return Err(PayloadIdentityGap::NotPinnedByKey {
                            schema: None,
                            field: field.clone(),
                        });
                    }
                }

                Ok(())
            }

            Input::Subscription(subscription) => {
                let identity = self
                    .model
                    .topics
                    .get(&subscription.topic)
                    .map(|topic| &topic.message_identity);

                let Some(MessageIdentity::Keyed { mapping }) = identity else {
                    return Err(PayloadIdentityGap::NotDeclared);
                };

                let Some(schemas) = &self.schemas else {
                    return Err(PayloadIdentityGap::NotDeclared);
                };

                let mut tuples = Vec::new();

                for schema in schemas {
                    match mapping.get(*schema) {
                        Some(tuple) if !tuple.is_empty() => tuples.push((*schema, tuple)),

                        _ => {
                            return Err(PayloadIdentityGap::SchemaNotMapped {
                                schema: (*schema).clone(),
                            });
                        }
                    }
                }

                let Some((_, first)) = tuples.first() else {
                    return Err(PayloadIdentityGap::NotDeclared);
                };

                // For each identity position, one key component must
                // pin that position's field in every admitted schema;
                // this is what carries key equality across schemas.
                for (position, field) in first.iter().enumerate() {
                    let pinned = self.key.components.iter().any(|component| {
                        tuples.iter().all(|(schema, tuple)| {
                            tuple.get(position).is_some_and(|identity_field| {
                                self.same_value(schema, &component.path, identity_field)
                            })
                        })
                    });

                    if !pinned {
                        let unpinned = tuples
                            .iter()
                            .find(|(_, tuple)| tuple.get(position).is_none());

                        return Err(PayloadIdentityGap::NotPinnedByKey {
                            schema: unpinned.map(|(schema, _)| (*schema).clone()),
                            field: field.clone(),
                        });
                    }
                }

                Ok(())
            }
        }
    }

    /// Whether a payload path is pinned by the governing key in every
    /// admitted schema.
    fn pinned_by_key(&self, path: &FieldPath) -> bool {
        self.key.components.iter().any(|component| {
            match &self.schemas {
                Some(schemas) => schemas
                    .iter()
                    .all(|schema| self.same_value(schema, &component.path, path)),

                // Without resolvable schemas only syntactic equality
                // is trusted.
                None => &component.path == path,
            }
        })
    }

    /// Whether two paths denote the same logical value in any instance
    /// of the schema (§4 fragment identity).
    fn same_value(&self, schema: &Id, first: &FieldPath, second: &FieldPath) -> bool {
        if first == second {
            return true;
        }

        match (
            canonical_value_path(self.model, schema, first),
            canonical_value_path(self.model, schema, second),
        ) {
            (Some(first), Some(second)) => first == second,
            _ => false,
        }
    }

    /// The §18 root-stability judgment, resolved against the artifact
    /// context accumulated so far.
    pub fn root_stability(
        &self,
        context: &BTreeMap<Id, ArtifactReplay>,
        root: &ValueRef,
    ) -> Result<StableRoot, StabilityGap> {
        let rule = match &root.source {
            ValueSource::Input(input) if input == self.input => {
                if self.pinned_by_key(&root.path) {
                    Ok(StabilityRule::KeyComponent)
                } else {
                    match &self.payload {
                        Ok(()) => Ok(StabilityRule::IdentifiedPayload),

                        Err(identity) => Err(StabilityGap::UnidentifiedPayloadField {
                            input: input.clone(),
                            identity: identity.clone(),
                        }),
                    }
                }
            }

            ValueSource::Input(input) => Err(StabilityGap::NotTriggeringInput {
                input: input.clone(),
            }),

            ValueSource::StateMachineSubject(machine) => Err(StabilityGap::MutableSubjectState {
                machine: machine.clone(),
            }),

            ValueSource::Effect(effect) => Err(StabilityGap::EffectPayloadRoot {
                effect: effect.clone(),
            }),

            ValueSource::TransactionRead(read) => Err(StabilityGap::TransactionReadRoot {
                read: read.clone(),
            }),

            ValueSource::InvocationResult(artifact) => match context.get(artifact) {
                None => Err(StabilityGap::ArtifactNotInContext {
                    artifact: artifact.clone(),
                }),

                Some(ArtifactReplay::Unavailable { .. }) => Err(StabilityGap::ArtifactUnavailable {
                    artifact: artifact.clone(),
                }),

                Some(ArtifactReplay::Recovered { transaction, .. }) => {
                    Ok(StabilityRule::RecoveredArtifact {
                        transaction: transaction.clone(),
                    })
                }

                Some(ArtifactReplay::Reconstructed { transaction, .. }) => {
                    Ok(StabilityRule::ReconstructedArtifact {
                        transaction: transaction.clone(),
                    })
                }
            },
        }?;

        Ok(StableRoot {
            root: root.clone(),
            rule,
        })
    }

    /// Walks a flow in step order, producing the artifact context at
    /// its end: every established artifact with its replay route.
    ///
    /// An artifact established more than once resolves to its latest
    /// establishment, matching flow order.
    pub fn flow_artifacts(&self, flow: &InvocationFlow) -> BTreeMap<Id, ArtifactReplay> {
        let mut context: BTreeMap<Id, ArtifactReplay> = BTreeMap::new();

        for step in &flow.steps {
            let FlowStep::Transaction { transaction } = step else {
                continue;
            };

            let Some(body) = self.operation.transactions.get(transaction) else {
                continue;
            };

            let _ = self.apply_transaction(&mut context, transaction, body);
        }

        context
    }

    /// Judges one transaction's replay routes and folds the artifacts
    /// it establishes into the context, returning the routes.
    ///
    /// Both route legs are judged from the transaction-entry context;
    /// a commit key may not observe transaction state, and natural
    /// replay concerns the body as a whole. Artifact derivations see
    /// artifacts established earlier in the same transaction, in step
    /// order.
    #[allow(clippy::type_complexity)]
    pub(crate) fn apply_transaction(
        &self,
        context: &mut BTreeMap<Id, ArtifactReplay>,
        transaction: &Id,
        body: &Transaction,
    ) -> (
        Result<Vec<StableRoot>, Vec<ReplayGap>>,
        Result<(), Vec<ReplayGap>>,
    ) {
        let recovery = self.recovery_route(context, body);
        let natural = self.natural_route(context, body);

        for inner in &body.steps {
            match inner {
                TransactionStep::EstablishInvocationResult(establish) => {
                    let replay = self.artifact_replay(
                        transaction,
                        &recovery,
                        &natural,
                        &establish.values,
                        context,
                    );

                    context.insert(establish.result.clone(), replay);
                }

                TransactionStep::EstablishEffectIntent(establish) => {
                    let replay = self.artifact_replay(
                        transaction,
                        &recovery,
                        &natural,
                        &establish.values,
                        context,
                    );

                    context.insert(establish.intent.clone(), replay);
                }

                TransactionStep::Transition(transition) => {
                    for (effect, values) in &transition.effect_values {
                        let Some(intent) = self.transition_intent(effect) else {
                            continue;
                        };

                        let replay =
                            self.artifact_replay(transaction, &recovery, &natural, values, context);

                        context.insert(intent.clone(), replay);
                    }
                }

                _ => {}
            }
        }

        (recovery, natural)
    }

    /// The operation-level intent naming a transition side effect —
    /// the stable identity of the implicitly established artifact
    /// (§22).
    fn transition_intent(&self, effect: &Id) -> Option<&Id> {
        self.operation
            .effect_intents
            .iter()
            .find(|(_, intent)| &intent.effect == effect)
            .map(|(id, _)| id)
    }

    /// Route B: keyed commit deduplication over a stable key (§17).
    fn recovery_route(
        &self,
        context: &BTreeMap<Id, ArtifactReplay>,
        transaction: &Transaction,
    ) -> Result<Vec<StableRoot>, Vec<ReplayGap>> {
        let IdempotencyGuarantee::DeduplicatedBy { key } = &transaction.idempotency else {
            return Err(vec![ReplayGap::NoKeyedCommit]);
        };

        let mut roots = Vec::new();
        let mut gaps = Vec::new();

        for component in &key.components {
            match self.root_stability(context, component) {
                Ok(root) => roots.push(root),

                Err(gap) => gaps.push(ReplayGap::CommitKeyRootUnstable {
                    root: component.clone(),
                    gap,
                }),
            }
        }

        if gaps.is_empty() { Ok(roots) } else { Err(gaps) }
    }

    /// Route A precondition: the V1 natural-replay judgment over the
    /// transaction body.
    fn natural_route(
        &self,
        context: &BTreeMap<Id, ArtifactReplay>,
        transaction: &Transaction,
    ) -> Result<(), Vec<ReplayGap>> {
        let mut gaps = Vec::new();

        let push = |gaps: &mut Vec<ReplayGap>, gap: ReplayGap| {
            if !gaps.contains(&gap) {
                gaps.push(gap);
            }
        };

        for step in &transaction.steps {
            match step {
                TransactionStep::Transition(_) => push(&mut gaps, ReplayGap::ContainsTransition),

                TransactionStep::Insert(_) => push(&mut gaps, ReplayGap::ContainsInsert),

                TransactionStep::Delete(_) => push(&mut gaps, ReplayGap::ContainsDelete),

                TransactionStep::Write(write) => {
                    for root in predicate_roots(&write.target.predicate) {
                        if let Err(gap) = self.root_stability(context, root) {
                            push(
                                &mut gaps,
                                ReplayGap::MutationTargetRootUnstable {
                                    root: root.clone(),
                                    gap,
                                },
                            );
                        }
                    }

                    match &write.values {
                        Derivation::Unspecified => {
                            push(&mut gaps, ReplayGap::MutationDerivationUnspecified);
                        }

                        Derivation::Deterministic { from } => {
                            for root in from {
                                if let Err(gap) = self.root_stability(context, root) {
                                    push(
                                        &mut gaps,
                                        ReplayGap::MutationDerivationRootUnstable {
                                            root: root.clone(),
                                            gap,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }

                TransactionStep::Read(_)
                | TransactionStep::Lock(_)
                | TransactionStep::EstablishEffectIntent(_)
                | TransactionStep::EstablishInvocationResult(_) => {}
            }
        }

        if gaps.is_empty() { Ok(()) } else { Err(gaps) }
    }

    /// §17 route selection for one established artifact. Recovery
    /// needs no derivation determinism: the retained artifact is the
    /// exact original.
    fn artifact_replay(
        &self,
        transaction: &Id,
        recovery: &Result<Vec<StableRoot>, Vec<ReplayGap>>,
        natural: &Result<(), Vec<ReplayGap>>,
        derivation: &Derivation,
        context: &BTreeMap<Id, ArtifactReplay>,
    ) -> ArtifactReplay {
        if let Ok(key) = recovery {
            return ArtifactReplay::Recovered {
                transaction: transaction.clone(),
                key: key.clone(),
            };
        }

        let mut reconstruction = natural.clone().err().unwrap_or_default();

        let mut roots = Vec::new();

        match derivation {
            Derivation::Unspecified => {
                reconstruction.push(ReplayGap::ArtifactDerivationUnspecified);
            }

            Derivation::Deterministic { from } => {
                for root in from {
                    match self.root_stability(context, root) {
                        Ok(root) => roots.push(root),

                        Err(gap) => reconstruction.push(ReplayGap::ArtifactDerivationRootUnstable {
                            root: root.clone(),
                            gap,
                        }),
                    }
                }
            }
        }

        if reconstruction.is_empty() {
            ArtifactReplay::Reconstructed {
                transaction: transaction.clone(),
                derivation: roots,
            }
        } else {
            ArtifactReplay::Unavailable {
                transaction: transaction.clone(),
                recovery: recovery.clone().err().unwrap_or_default(),
                reconstruction,
            }
        }
    }
}

/// Every `ValueRef` a selector predicate constrains against. Literals
/// are trivially stable and contribute nothing.
pub(crate) fn predicate_roots(predicate: &SelectorPredicate) -> Vec<&ValueRef> {
    match predicate {
        SelectorPredicate::All => Vec::new(),

        SelectorPredicate::Eq { value, .. } => match value {
            SelectorValue::Value(root) => vec![root],
            SelectorValue::Literal(_) => Vec::new(),
        },

        SelectorPredicate::And { predicates } => {
            predicates.iter().flat_map(predicate_roots).collect()
        }
    }
}
