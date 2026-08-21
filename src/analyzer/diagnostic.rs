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
    Verification(VerificationCode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationCode {
    /// A declared serialization requirement is not established by the
    /// declared facts. Epistemic, not a violation (§1.2).
    SerializationUnproven,

    /// A declared response-replay obligation is not established by the
    /// declared facts. Epistemic, not a violation (§1.2).
    ResponseReplayUnproven,

    /// A declared recoverability requirement is not established by the
    /// declared facts. Epistemic, not a violation (§1.2).
    RecoverabilityUnproven,

    /// A declared idempotency requirement is not established by the
    /// declared facts. Epistemic, not a violation (§1.2).
    IdempotencyUnproven,
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

    MessageIdentitySchemaNotOnTopic,
    EmptyMessageIdentity,
    MessageIdentityArityMismatch,
    EmptyRequestIdentity,

    TransactionObjectOutsideDataModel,
    TransactionMissingDataModel,

    TransactionReadOutsideTransaction,
    TransactionReadOutOfOrder,
    TransactionReadFieldNotSelected,

    StateTransitionSubjectMismatch,
    TransitionTransactionNotDeduplicated,
    TransitionEffectValuesMismatch,
    TransitionEffectIntentExplicitlyEstablished,
    AmbiguousTransitionEffectIntent,
    UnestablishableTransitionEffectIntent,

    EmptyObjectIdentity,

    RecoverabilityRequiresFlow,

    ResponseInvocationResultSchemaMismatch,

    InvalidInputKind,
}
