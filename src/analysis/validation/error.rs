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

    TransactionObjectOutsideDataModel {
        transaction: Id,
        data_model: Id,
        object: Id,
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
            ValidationError::DuplicateId { id, first, second } => {
                let first_subject = first.owner.clone().or_else(|| Some(id.clone()));

                let second_subject = second.owner.clone().or_else(|| Some(id.clone()));

                Diagnostic {
                    code: DiagnosticCode::Validation(ValidationCode::DuplicateId),
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
                let message = format!("`{subject}` references unknown {expected} `{reference}`.");

                Diagnostic {
                    code: DiagnosticCode::Validation(ValidationCode::UnknownReference),
                    severity: Severity::Error,
                    subject: Some(subject),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(reference),
                        message: format!("Expected this ID to resolve to a {expected}."),
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
                    code: DiagnosticCode::Validation(ValidationCode::InvalidReferenceKind),
                    severity: Severity::Error,
                    subject: Some(subject),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(reference),
                        message: format!("Expected {expected}, found {actual}."),
                    }],
                }
            }

            ValidationError::InvalidFieldPath {
                subject,
                schema,
                path,
            } => {
                let message =
                    format!("Field path `{path}` does not resolve against schema `{schema}`.");

                Diagnostic {
                    code: DiagnosticCode::Validation(ValidationCode::InvalidFieldPath),
                    severity: Severity::Error,
                    subject: Some(subject),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(schema),
                        message: format!("Could not resolve field path `{path}` from this schema."),
                    }],
                }
            }

            ValidationError::FragmentCycle { cycle } => {
                let rendered_cycle = cycle
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" -> ");

                let subject = cycle.first().cloned();

                Diagnostic {
                    code: DiagnosticCode::Validation(ValidationCode::FragmentCycle),
                    severity: Severity::Error,
                    subject,
                    message: format!(
                        "Schema fragment derivation contains a cycle: \
                         {rendered_cycle}."
                    ),
                    evidence: vec![],
                }
            }

            ValidationError::DataObjectSchemaNotCanonical { object, schema } => {
                let message = format!(
                    "Data object `{object}` references non-canonical \
                     schema `{schema}`."
                );

                Diagnostic {
                    code: DiagnosticCode::Validation(ValidationCode::DataObjectSchemaNotCanonical),
                    severity: Severity::Error,
                    subject: Some(object),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(schema),
                        message: "A data object's state must be described \
                                  by a canonical schema."
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
                    code: DiagnosticCode::Validation(ValidationCode::SubscriptionMessageNotOnTopic),
                    severity: Severity::Error,
                    subject: Some(input),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(topic),
                        message: format!("Topic does not declare schema `{schema}` as a message."),
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
                        message: format!("This object is outside data model `{data_model}`."),
                    }],
                }
            }

            ValidationError::InvalidInputKind {
                subject,
                input,
                expected,
                actual,
            } => Diagnostic {
                code: DiagnosticCode::Validation(ValidationCode::InvalidInputKind),
                severity: Severity::Error,
                subject: Some(subject),
                message: format!("`{input}` is referenced as a {expected}, but it is a {actual}."),
                evidence: vec![Evidence {
                    subject: Some(input),
                    message: format!("Expected {expected}, found {actual}."),
                }],
            },

            ValidationError::InvalidReferenceOwner {
                subject,
                reference,
                expected_owner,
                actual_owner,
            } => {
                let actual = match &actual_owner {
                    Some(owner) => {
                        format!("`{owner}`")
                    }

                    None => "no owner".to_string(),
                };

                Diagnostic {
                    code: DiagnosticCode::Validation(ValidationCode::InvalidReferenceOwner),
                    severity: Severity::Error,
                    subject: Some(subject),
                    message: format!(
                        "`{reference}` is referenced through `{expected_owner}`, \
             but is not owned by it."
                    ),
                    evidence: vec![Evidence {
                        subject: Some(reference),
                        message: format!("Expected owner `{expected_owner}`, found {actual}."),
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
                        message: format!("Topic does not declare schema `{schema}` as a message."),
                    }],
                }
            }
        }
    }
}
