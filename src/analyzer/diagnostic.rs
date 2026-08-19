use crate::spec::Id;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,

    /// Primary model entity to which this diagnostic applies.
    pub subject: Option<Id>,

    pub message: String,

    /// Additional model facts that explain the diagnostic.
    pub evidence: Vec<Evidence>,
}

impl Diagnostic {
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    pub subject: Option<Id>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    Validation(ValidationCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationCode {
    DuplicateId,
    UnknownReference,
    InvalidReferenceKind,
    InvalidReferenceOwner,

    InvalidFieldPath,
    ValueSourceHasNoSchema,
    ValueSourceOutOfScope,

    FragmentCycle,

    DataObjectSchemaNotCanonical,

    SubscriptionMessageNotOnTopic,
    PublicationEffectMessageNotOnTopic,

    TopicKeySchemaNotOnTopic,
    TopicKeyMissingSchema,

    TransactionObjectOutsideDataModel,
    TransactionMissingDataModel,

    TransactionReadOutsideTransaction,
    TransactionReadOutOfOrder,
    TransactionReadFieldNotSelected,

    StateTransitionSubjectMismatch,
    TransitionTransactionNotDeduplicated,
    TransitionEffectIntentExplicitlyEstablished,
    AmbiguousTransitionEffectIntent,
    UnestablishableTransitionEffectIntent,

    EmptyObjectIdentity,

    RecoverabilityRequiresFlow,

    ResponseInvocationResultSchemaMismatch,

    InvalidInputKind,
}
