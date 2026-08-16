use crate::analysis::{
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

    TransactionObjectOutsideDataModel {
        transaction: Id,
        data_model: Id,
        object: Id,
    },

    TransactionMissingDataModel {
        transaction: Id,
        object: Id,
    },

    StateTransitionSubjectMismatch {
        transaction: Id,
        machine: Id,
        expected_object: Id,
        actual_object: Id,
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
