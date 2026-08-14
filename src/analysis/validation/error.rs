use std::fmt;

use crate::analysis::{Diagnostic, DiagnosticCode, Evidence, Severity, ValidationCode};
use crate::spec::{FieldPath, Id};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// The same globally unique ID was assigned to more than
    /// one addressable model entity.
    DuplicateId { id: Id },

    /// A reference does not resolve to any model entity.
    UnknownReference {
        subject: Id,
        reference: Id,
        expected: ReferenceKind,
    },

    /// The referenced ID exists, but identifies the wrong
    /// kind of model entity.
    InvalidReferenceKind {
        subject: Id,
        reference: Id,
        expected: ReferenceKind,
        actual: ReferenceKind,
    },

    /// A field path cannot be resolved against the schema
    /// against which it was declared.
    InvalidFieldPath {
        subject: Id,
        schema: Id,
        path: FieldPath,
    },

    /// Schema-fragment derivation contains a cycle.
    FragmentCycle { cycle: Vec<Id> },

    /// Persistent data objects must use canonical schemas.
    DataObjectSchemaNotCanonical { object: Id, schema: Id },

    /// A subscription selects a schema that the referenced
    /// topic does not carry.
    SubscriptionMessageNotOnTopic { input: Id, topic: Id, schema: Id },

    /// A publication emits a schema not declared as a message
    /// of its target topic.
    PublicationMessageNotOnTopic { output: Id, topic: Id, schema: Id },

    /// A globally valid input exists, but does not belong to
    /// the operation through which it was referenced.
    InputNotOwnedByOperation {
        subject: Id,
        operation: Id,
        input: Id,
    },

    /// A transaction may only operate on objects belonging to
    /// its declared data-model boundary.
    TransactionObjectOutsideDataModel {
        transaction: Id,
        data_model: Id,
        object: Id,
    },
}

impl From<ValidationError> for Diagnostic {
    fn from(error: ValidationError) -> Self {
        match error {
            ValidationError::DuplicateId { id } => {
                let message = format!("ID `{id}` is declared more than once.");

                Diagnostic {
                    code: DiagnosticCode::Validation(ValidationCode::DuplicateId),
                    severity: Severity::Error,
                    subject: Some(id),
                    message,
                    evidence: vec![],
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

            ValidationError::PublicationMessageNotOnTopic {
                output,
                topic,
                schema,
            } => {
                let message = format!(
                    "Publication output `{output}` publishes schema `{schema}` \
                     to topic `{topic}`, but the topic does not carry that schema."
                );

                Diagnostic {
                    code: DiagnosticCode::Validation(ValidationCode::PublicationMessageNotOnTopic),
                    severity: Severity::Error,
                    subject: Some(output),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(topic),
                        message: format!("Topic does not declare schema `{schema}` as a message."),
                    }],
                }
            }

            ValidationError::InputNotOwnedByOperation {
                subject,
                operation,
                input,
            } => {
                let message = format!(
                    "`{subject}` references input `{input}` through operation \
                     `{operation}`, but that input belongs to a different operation."
                );

                Diagnostic {
                    code: DiagnosticCode::Validation(ValidationCode::InputNotOwnedByOperation),
                    severity: Severity::Error,
                    subject: Some(subject),
                    message,
                    evidence: vec![Evidence {
                        subject: Some(input),
                        message: format!("This input is not owned by operation `{operation}`."),
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    Service,
    Schema,
    DataModel,
    DataObject,
    Topic,
    Operation,
    Input,
    Output,
    SideEffect,
    Transaction,
}

impl fmt::Display for ReferenceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Service => "service",
            Self::Schema => "schema",
            Self::DataModel => "data model",
            Self::DataObject => "data object",
            Self::Topic => "topic",
            Self::Operation => "operation",
            Self::Input => "input",
            Self::Output => "output",
            Self::SideEffect => "side effect",
            Self::Transaction => "transaction",
        };

        f.write_str(name)
    }
}
