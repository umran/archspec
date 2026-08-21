use crate::analyzer::{
    Diagnostic, DiagnosticCode, Evidence, IdDeclaration, Severity, ValidationCode,
};
use crate::spec::{FieldPath, Id};

use super::{InputKind, ReferenceKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    DuplicateId {
        id: Id,
        first: IdDeclaration,
        second: IdDeclaration,
    },

    UnknownReference {
        subject: Id,
        reference: Id,
        expected: ReferenceKind,
    },

    InvalidReferenceKind {
        subject: Id,
        reference: Id,
        expected: ReferenceKind,
        actual: ReferenceKind,
    },

    InvalidReferenceOwner {
        subject: Id,
        reference: Id,
        expected_owner: Id,
        actual_owner: Option<Id>,
    },

    InvalidFieldPath {
        subject: Id,
        schema: Id,
        path: FieldPath,
    },

    /// A ValueRef attempts to dereference a source for which
    /// the model defines no structural payload schema.
    ValueSourceHasNoSchema {
        subject: Id,
        source: Id,
    },

    /// A ValueRef names a source that the invocations evaluating it
    /// cannot observe, such as another operation's input.
    ValueSourceOutOfScope {
        subject: Id,
        source: Id,
        owner: Id,
    },

    FragmentCycle {
        cycle: Vec<Id>,
    },

    DataObjectSchemaNotCanonical {
        object: Id,
        schema: Id,
    },

    SubscriptionMessageNotOnTopic {
        input: Id,
        topic: Id,
        schema: Id,
    },

    PublicationEffectMessageNotOnTopic {
        effect: Id,
        topic: Id,
        schema: Id,
    },

    TopicKeySchemaNotOnTopic {
        topic: Id,
        schema: Id,
    },

    TopicKeyMissingSchema {
        topic: Id,
        schema: Id,
    },

    /// A message-identity mapping names a schema the topic does not
    /// carry.
    MessageIdentitySchemaNotOnTopic {
        topic: Id,
        schema: Id,
    },

    /// A message-identity mapping declares an empty identity tuple.
    EmptyMessageIdentity {
        topic: Id,
        schema: Id,
    },

    /// Message-identity tuple positions correspond across schemas, so
    /// every mapped tuple must have the same arity.
    MessageIdentityArityMismatch {
        topic: Id,
        schema: Id,
        expected: usize,
        actual: usize,
    },

    /// A request input declares a keyed identity with no fields.
    EmptyRequestIdentity {
        input: Id,
    },

    TransactionObjectOutsideDataModel {
        transaction: Id,
        data_model: Id,
        object: Id,
    },

    TransactionMissingDataModel {
        transaction: Id,
        object: Id,
    },

    /// A ValueRef names a transaction-read result outside the
    /// transaction execution that produces it.
    TransactionReadOutsideTransaction {
        subject: Id,
        read: Id,
    },

    /// A ValueRef names a transaction-read result that does not
    /// precede its use in transaction program order.
    TransactionReadOutOfOrder {
        transaction: Id,
        read: Id,
    },

    /// A ValueRef names a field the read did not select.
    TransactionReadFieldNotSelected {
        transaction: Id,
        read: Id,
        path: FieldPath,
    },

    StateTransitionSubjectMismatch {
        transaction: Id,
        machine: Id,
        expected_object: Id,
        actual_object: Id,
    },

    /// V1 requires every transition-containing transaction to declare
    /// durable keyed commit deduplication.
    TransitionTransactionNotDeduplicated {
        transaction: Id,
        machine: Id,
        transition: Id,
    },

    /// A `StateTransition` step's `effect_values` keys do not exactly
    /// match the side effects declared by the applied transition.
    TransitionEffectValuesMismatch {
        transaction: Id,
        transition: Id,
        missing: Vec<Id>,
        unexpected: Vec<Id>,
    },

    /// A transition side effect is established implicitly by the
    /// transition and must not be established explicitly.
    TransitionEffectIntentExplicitlyEstablished {
        transaction: Id,
        intent: Id,
        effect: Id,
    },

    /// An operation declares more than one effect intent for the same
    /// transition side effect, leaving the implicitly established
    /// artifact without a single logical identity.
    AmbiguousTransitionEffectIntent {
        operation: Id,
        effect: Id,
        intents: Vec<Id>,
    },

    /// An operation declares an effect intent for a transition side
    /// effect, but applies no transition that could establish it.
    UnestablishableTransitionEffectIntent {
        operation: Id,
        intent: Id,
        effect: Id,
        transition: Id,
    },

    EmptyObjectIdentity {
        object: Id,
    },

    /// An operation requires that its invocations reach terminal
    /// execution, but declares no flow that could terminate.
    RecoverabilityRequiresFlow {
        operation: Id,
    },

    ResponseInvocationResultSchemaMismatch {
        response: Id,
        response_schema: Id,
        result: Id,
        result_schema: Id,
    },

    InvalidInputKind {
        subject: Id,
        input: Id,
        expected: InputKind,
        actual: InputKind,
    },
}

impl From<ValidationError> for Diagnostic {
    fn from(error: ValidationError) -> Self {
        match error {
            ValidationError::DuplicateId {
                id,
                first,
                second,
            } => {
                let first_subject =
                    first.owner.clone().or_else(|| Some(id.clone()));

                let second_subject =
                    second.owner.clone().or_else(|| Some(id.clone()));

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::DuplicateId,
                    ),
                    severity: Severity::Error,
                    subject: Some(id.clone()),
                    message: format!(
                        "ID `{id}` is declared more than once in the global model namespace."
                    ),
                    evidence: vec![
                        Evidence {
                            subject: first_subject,
                            message: first.describe(),
                        },
                        Evidence {
                            subject: second_subject,
                            message: second.describe(),
                        },
                    ],
                }
            }

            ValidationError::UnknownReference {
                subject,
                reference,
                expected,
            } => {
                let message = format!(
                    "`{subject}` references unknown {expected} `{reference}`."
                );

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::UnknownReference,
                    ),
                    severity: Severity::Error,
                    subject: Some(subject),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(reference),
                        message: format!(
                            "Expected this ID to resolve to a {expected}."
                        ),
                    }],
                }
            }

            ValidationError::InvalidReferenceKind {
                subject,
                reference,
                expected,
                actual,
            } => {
                let message = format!(
                    "`{subject}` references `{reference}` as a {expected}, \
                     but it is a {actual}."
                );

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::InvalidReferenceKind,
                    ),
                    severity: Severity::Error,
                    subject: Some(subject),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(reference),
                        message: format!(
                            "Expected {expected}, found {actual}."
                        ),
                    }],
                }
            }

            ValidationError::InvalidReferenceOwner {
                subject,
                reference,
                expected_owner,
                actual_owner,
            } => {
                let actual_owner_description =
                    match &actual_owner {
                        Some(owner) => {
                            format!("`{owner}`")
                        }

                        None => {
                            "no owner".to_string()
                        }
                    };

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::InvalidReferenceOwner,
                    ),
                    severity: Severity::Error,
                    subject: Some(subject),
                    message: format!(
                        "`{reference}` is referenced through `{expected_owner}`, \
                         but is not owned by it."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(reference),
                        message: format!(
                            "Expected owner `{expected_owner}`, found \
                             {actual_owner_description}."
                        ),
                    }],
                }
            }

            ValidationError::InvalidFieldPath {
                subject,
                schema,
                path,
            } => {
                let message = format!(
                    "Field path `{path}` does not resolve against schema `{schema}`."
                );

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::InvalidFieldPath,
                    ),
                    severity: Severity::Error,
                    subject: Some(subject),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(schema),
                        message: format!(
                            "Could not resolve field path `{path}` from this schema."
                        ),
                    }],
                }
            }

            ValidationError::ValueSourceHasNoSchema {
                subject,
                source,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::ValueSourceHasNoSchema,
                    ),
                    severity: Severity::Error,
                    subject: Some(subject.clone()),
                    message: format!(
                        "`{subject}` references fields of `{source}`, \
                         but that value source has no modeled payload schema."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(source),
                        message:
                            "A field path can only be resolved from a structurally typed value source."
                                .to_string(),
                    }],
                }
            }

            ValidationError::ValueSourceOutOfScope {
                subject,
                source,
                owner,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::ValueSourceOutOfScope,
                    ),
                    severity: Severity::Error,
                    subject: Some(subject.clone()),
                    message: format!(
                        "`{subject}` references `{source}`, which belongs to \
                         `{owner}` and is not observable by the invocations \
                         that evaluate this reference."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(owner),
                        message:
                            "A value reference may only name inputs, effects, and invocation results reachable from the operation whose invocation evaluates it."
                                .to_string(),
                    }],
                }
            }

            ValidationError::FragmentCycle {
                cycle,
            } => {
                let rendered_cycle = cycle
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" -> ");

                let subject = cycle.first().cloned();

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::FragmentCycle,
                    ),
                    severity: Severity::Error,
                    subject,
                    message: format!(
                        "Schema fragment derivation contains a cycle: \
                         {rendered_cycle}."
                    ),
                    evidence: vec![],
                }
            }

            ValidationError::DataObjectSchemaNotCanonical {
                object,
                schema,
            } => {
                let message = format!(
                    "Data object `{object}` references non-canonical \
                     schema `{schema}`."
                );

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::DataObjectSchemaNotCanonical,
                    ),
                    severity: Severity::Error,
                    subject: Some(object),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(schema),
                        message:
                            "A data object's state must be described by a canonical schema."
                                .to_string(),
                    }],
                }
            }

            ValidationError::SubscriptionMessageNotOnTopic {
                input,
                topic,
                schema,
            } => {
                let message = format!(
                    "Subscription input `{input}` selects schema `{schema}`, \
                     which is not carried by topic `{topic}`."
                );

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::SubscriptionMessageNotOnTopic,
                    ),
                    severity: Severity::Error,
                    subject: Some(input),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(topic),
                        message: format!(
                            "Topic does not declare schema `{schema}` as a message."
                        ),
                    }],
                }
            }

            ValidationError::PublicationEffectMessageNotOnTopic {
                effect,
                topic,
                schema,
            } => {
                let message = format!(
                    "Publication effect `{effect}` publishes schema `{schema}` \
                     to topic `{topic}`, but the topic does not carry that schema."
                );

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::PublicationEffectMessageNotOnTopic,
                    ),
                    severity: Severity::Error,
                    subject: Some(effect),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(topic),
                        message: format!(
                            "Topic does not declare schema `{schema}` as a message."
                        ),
                    }],
                }
            }

            ValidationError::TopicKeySchemaNotOnTopic {
                topic,
                schema,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::TopicKeySchemaNotOnTopic,
                    ),
                    severity: Severity::Error,
                    subject: Some(topic.clone()),
                    message: format!(
                        "Topic `{topic}` defines an ordering key for schema \
                         `{schema}`, but does not carry that schema."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(schema),
                        message:
                            "Ordering-key mappings may only reference message schemas carried by the topic."
                                .to_string(),
                    }],
                }
            }

            ValidationError::TopicKeyMissingSchema {
                topic,
                schema,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::TopicKeyMissingSchema,
                    ),
                    severity: Severity::Error,
                    subject: Some(topic.clone()),
                    message: format!(
                        "Keyed topic `{topic}` carries schema `{schema}` \
                         but defines no ordering-key mapping for it."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(schema),
                        message:
                            "Every message schema carried by a keyed topic must define how its ordering key is obtained."
                                .to_string(),
                    }],
                }
            }

            ValidationError::MessageIdentitySchemaNotOnTopic {
                topic,
                schema,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::MessageIdentitySchemaNotOnTopic,
                    ),
                    severity: Severity::Error,
                    subject: Some(topic.clone()),
                    message: format!(
                        "Topic `{topic}` declares a message identity for schema \
                         `{schema}`, but does not carry that schema."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(schema),
                        message:
                            "Message-identity mappings may only reference message schemas carried by the topic."
                                .to_string(),
                    }],
                }
            }

            ValidationError::EmptyMessageIdentity {
                topic,
                schema,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::EmptyMessageIdentity,
                    ),
                    severity: Severity::Error,
                    subject: Some(topic.clone()),
                    message: format!(
                        "Topic `{topic}` declares an empty message identity \
                         for schema `{schema}`."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(schema),
                        message:
                            "A mapped schema must declare the complete, non-empty identity of one logical message."
                                .to_string(),
                    }],
                }
            }

            ValidationError::MessageIdentityArityMismatch {
                topic,
                schema,
                expected,
                actual,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::MessageIdentityArityMismatch,
                    ),
                    severity: Severity::Error,
                    subject: Some(topic.clone()),
                    message: format!(
                        "Topic `{topic}` maps the message identity of \
                         `{schema}` with {actual} field(s), but other mapped \
                         schemas use {expected}."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(schema),
                        message:
                            "Identity tuple positions correspond across schemas, so every mapped tuple must have the same arity."
                                .to_string(),
                    }],
                }
            }

            ValidationError::EmptyRequestIdentity {
                input,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::EmptyRequestIdentity,
                    ),
                    severity: Severity::Error,
                    subject: Some(input.clone()),
                    message: format!(
                        "Request input `{input}` declares a keyed identity \
                         with no fields."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(input),
                        message:
                            "A keyed request identity must declare the complete, non-empty identity of one logical request."
                                .to_string(),
                    }],
                }
            }

            ValidationError::TransactionObjectOutsideDataModel {
                transaction,
                data_model,
                object,
            } => {
                let message = format!(
                    "Transaction `{transaction}` accesses object `{object}`, \
                     which does not belong to its declared data model `{data_model}`."
                );

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::TransactionObjectOutsideDataModel,
                    ),
                    severity: Severity::Error,
                    subject: Some(transaction),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(object),
                        message: format!(
                            "This object is outside data model `{data_model}`."
                        ),
                    }],
                }
            }

            ValidationError::TransactionMissingDataModel {
                transaction,
                object,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::TransactionMissingDataModel,
                    ),
                    severity: Severity::Error,
                    subject: Some(transaction.clone()),
                    message: format!(
                        "Transaction `{transaction}` accesses data object \
                         `{object}` but declares no data-model boundary."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(object),
                        message:
                            "Access to a persistent data object requires the transaction to declare its data model."
                                .to_string(),
                    }],
                }
            }

            ValidationError::TransactionReadOutsideTransaction {
                subject,
                read,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::TransactionReadOutsideTransaction,
                    ),
                    severity: Severity::Error,
                    subject: Some(subject.clone()),
                    message: format!(
                        "`{subject}` references transaction-read result `{read}` \
                         outside the transaction execution that produces it."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(read),
                        message:
                            "A transaction-read result is local to its transaction execution and never becomes a durable cross-transaction artifact."
                                .to_string(),
                    }],
                }
            }

            ValidationError::TransactionReadOutOfOrder {
                transaction,
                read,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::TransactionReadOutOfOrder,
                    ),
                    severity: Severity::Error,
                    subject: Some(transaction.clone()),
                    message: format!(
                        "Transaction `{transaction}` references transaction-read \
                         result `{read}` before the step that reads it."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(read),
                        message:
                            "A transaction-read result may only be referenced by steps that follow the read in transaction program order."
                                .to_string(),
                    }],
                }
            }

            ValidationError::TransactionReadFieldNotSelected {
                transaction,
                read,
                path,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::TransactionReadFieldNotSelected,
                    ),
                    severity: Severity::Error,
                    subject: Some(transaction),
                    message: format!(
                        "Transaction-read result `{read}` is referenced through \
                         field path `{path}`, which the read does not select."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(read),
                        message: format!(
                            "This read does not include `{path}` in its field selection."
                        ),
                    }],
                }
            }

            ValidationError::StateTransitionSubjectMismatch {
                transaction,
                machine,
                expected_object,
                actual_object,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::StateTransitionSubjectMismatch,
                    ),
                    severity: Severity::Error,
                    subject: Some(transaction),
                    message: format!(
                        "State-machine transition for `{machine}` selects \
                         data object `{actual_object}`, but the state machine \
                         governs `{expected_object}`."
                    ),
                    evidence: vec![
                        Evidence {
                            subject: Some(machine),
                            message: format!(
                                "This state machine governs data object \
                                 `{expected_object}`."
                            ),
                        },
                        Evidence {
                            subject: Some(actual_object),
                            message:
                                "This data object is selected as the subject of the transaction's state transition."
                                    .to_string(),
                        },
                    ],
                }
            }

            ValidationError::TransitionTransactionNotDeduplicated {
                transaction,
                machine,
                transition,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::TransitionTransactionNotDeduplicated,
                    ),
                    severity: Severity::Error,
                    subject: Some(transaction.clone()),
                    message: format!(
                        "Transaction `{transaction}` applies transition \
                         `{transition}` but declares no durable keyed commit \
                         deduplication."
                    ),
                    evidence: vec![
                        Evidence {
                            subject: Some(transition),
                            message: format!(
                                "This transition of `{machine}` changes the state \
                                 it is evaluated against, so the transaction \
                                 cannot be naturally replayed."
                            ),
                        },
                        Evidence {
                            subject: Some(transaction),
                            message:
                                "A transition-containing transaction must declare `deduplicated_by` so a later encounter can resolve the prior commit and recover its artifacts."
                                    .to_string(),
                        },
                    ],
                }
            }

            ValidationError::TransitionEffectValuesMismatch {
                transaction,
                transition,
                missing,
                unexpected,
            } => {
                let mut evidence = Vec::new();

                for effect in &missing {
                    evidence.push(Evidence {
                        subject: Some(effect.clone()),
                        message: format!(
                            "The transition declares side effect `{effect}`, \
                             but the step provides no value derivation for it."
                        ),
                    });
                }

                for effect in &unexpected {
                    evidence.push(Evidence {
                        subject: Some(effect.clone()),
                        message: format!(
                            "The step provides a value derivation for \
                             `{effect}`, which is not a side effect declared \
                             by transition `{transition}`."
                        ),
                    });
                }

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::TransitionEffectValuesMismatch,
                    ),
                    severity: Severity::Error,
                    subject: Some(transaction.clone()),
                    message: format!(
                        "Transaction `{transaction}` applies transition \
                         `{transition}` with `effect_values` that do not \
                         exactly match the transition's declared side effects."
                    ),
                    evidence,
                }
            }

            ValidationError::TransitionEffectIntentExplicitlyEstablished {
                transaction,
                intent,
                effect,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::TransitionEffectIntentExplicitlyEstablished,
                    ),
                    severity: Severity::Error,
                    subject: Some(transaction),
                    message: format!(
                        "Transaction establishes effect intent `{intent}`, but its \
                         effect `{effect}` is a transition side effect."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(effect),
                        message:
                            "A transition side effect is established implicitly by a successful transition, not by an explicit establishment step."
                                .to_string(),
                    }],
                }
            }

            ValidationError::AmbiguousTransitionEffectIntent {
                operation,
                effect,
                intents,
            } => {
                let evidence = intents
                    .iter()
                    .map(|intent| Evidence {
                        subject: Some(intent.clone()),
                        message: format!(
                            "`{intent}` claims the intent established by this \
                             transition side effect."
                        ),
                    })
                    .collect();

                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::AmbiguousTransitionEffectIntent,
                    ),
                    severity: Severity::Error,
                    subject: Some(operation.clone()),
                    message: format!(
                        "Operation `{operation}` declares more than one effect \
                         intent for transition side effect `{effect}`."
                    ),
                    evidence,
                }
            }

            ValidationError::UnestablishableTransitionEffectIntent {
                operation,
                intent,
                effect,
                transition,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::UnestablishableTransitionEffectIntent,
                    ),
                    severity: Severity::Error,
                    subject: Some(operation.clone()),
                    message: format!(
                        "Operation `{operation}` declares effect intent \
                         `{intent}` for transition side effect `{effect}`, but \
                         applies no transition that would establish it."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(transition.clone()),
                        message: format!(
                            "No transaction in `{operation}` applies transition \
                             `{transition}`, which owns this side effect."
                        ),
                    }],
                }
            }

            ValidationError::EmptyObjectIdentity {
                object,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::EmptyObjectIdentity,
                    ),
                    severity: Severity::Error,
                    subject: Some(object.clone()),
                    message: format!(
                        "Data object `{object}` declares an empty identity."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(object),
                        message:
                            "Every data object must declare the complete, non-empty logical identity of one instance."
                                .to_string(),
                    }],
                }
            }

            ValidationError::RecoverabilityRequiresFlow {
                operation,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::RecoverabilityRequiresFlow,
                    ),
                    severity: Severity::Error,
                    subject: Some(operation.clone()),
                    message: format!(
                        "Operation `{operation}` declares a recoverability \
                         requirement but declares no invocation flow."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(operation),
                        message:
                            "Recoverability obliges an invocation to reach the terminal step of a declared flow, so at least one flow must exist."
                                .to_string(),
                    }],
                }
            }

            ValidationError::ResponseInvocationResultSchemaMismatch {
                response,
                response_schema,
                result,
                result_schema,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::ResponseInvocationResultSchemaMismatch,
                    ),
                    severity: Severity::Error,
                    subject: Some(response.clone()),
                    message: format!(
                        "Response `{response}` uses schema `{response_schema}`, \
                         but its invocation-result source `{result}` uses \
                         schema `{result_schema}`."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(result),
                        message: format!(
                            "This invocation result is declared with schema \
                             `{result_schema}`."
                        ),
                    }],
                }
            }

            ValidationError::InvalidInputKind {
                subject,
                input,
                expected,
                actual,
            } => {
                Diagnostic {
                    code: DiagnosticCode::Validation(
                        ValidationCode::InvalidInputKind,
                    ),
                    severity: Severity::Error,
                    subject: Some(subject),
                    message: format!(
                        "`{input}` is referenced as a {expected}, \
                         but it is a {actual}."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(input),
                        message: format!(
                            "Expected {expected}, found {actual}."
                        ),
                    }],
                }
            }
        }
    }
}
