pub mod error;
pub mod id_declaration;
pub mod reference;

use std::collections::{BTreeMap, BTreeSet};

use crate::spec::*;

pub use error::*;
pub use id_declaration::*;
pub use reference::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Request,
    Subscription,
}

impl std::fmt::Display for InputKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request => f.write_str("request input"),
            Self::Subscription => f.write_str("subscription input"),
        }
    }
}

/// The transaction execution a value reference is evaluated in.
///
/// Transaction-read results are only meaningful inside the transaction
/// execution that produces them, and only after the read itself.
#[derive(Debug, Clone, Copy)]
struct TransactionScope<'a> {
    id: &'a Id,
    transaction: &'a Transaction,

    /// Index of the step whose values are being validated.
    step: usize,
}

impl<'a> TransactionScope<'a> {
    fn read(&self, result: &Id) -> Option<(usize, &'a Read)> {
        self.transaction
            .steps
            .iter()
            .enumerate()
            .find_map(|(position, step)| match step {
                TransactionStep::Read(read) if &read.result == result => Some((position, read)),

                _ => None,
            })
    }
}

/// The invocation context a value reference is evaluated in.
///
/// A value reference may only name sources that the invocations
/// evaluating it can actually observe.
#[derive(Debug, Clone, Copy)]
enum ValueScope<'a> {
    /// Evaluated by invocations of one operation.
    Operation(&'a Id),

    /// Declared on a state-machine transition, and therefore evaluated
    /// by invocations of whichever operation applies that transition.
    Transition(&'a Id),
}

/// Everything a value reference is validated against.
#[derive(Debug, Clone, Copy)]
struct ValueContext<'a> {
    scope: ValueScope<'a>,

    /// Set when the reference appears inside a transaction body.
    transaction: Option<TransactionScope<'a>>,
}

impl<'a> ValueContext<'a> {
    fn operation(operation: &'a Id) -> Self {
        Self {
            scope: ValueScope::Operation(operation),
            transaction: None,
        }
    }

    fn transition(transition: &'a Id) -> Self {
        Self {
            scope: ValueScope::Transition(transition),
            transaction: None,
        }
    }

    fn in_transaction(self, id: &'a Id, transaction: &'a Transaction, step: usize) -> Self {
        Self {
            transaction: Some(TransactionScope {
                id,
                transaction,
                step,
            }),
            ..self
        }
    }
}

struct ReferenceIndex<'a> {
    entries: BTreeMap<Id, ReferenceInfo<'a>>,

    /// Operations that apply each declared transition.
    ///
    /// A transition side effect is established by whichever operation
    /// applies the transition, so this determines what a value
    /// reference declared on that transition may observe.
    transition_appliers: BTreeMap<&'a Id, BTreeSet<&'a Id>>,

    /// The effect each result binding observes: the executed effect,
    /// or the effect of the executed intent.
    result_bindings: BTreeMap<&'a Id, &'a Id>,
}

#[derive(Debug, Clone, Copy)]
struct ReferenceInfo<'a> {
    kind: ReferenceKind,
    owner: Option<&'a Id>,
}

impl<'a> ReferenceIndex<'a> {
    fn build(model: &'a Model) -> Self {
        let mut entries = BTreeMap::new();

        visit_declarations(model, |id, kind, owner| {
            entries.insert(id.clone(), ReferenceInfo { kind, owner });
        });

        let mut transition_appliers: BTreeMap<&Id, BTreeSet<&Id>> = BTreeMap::new();

        for (operation_id, operation) in &model.operations {
            for transaction in operation.transactions.values() {
                for step in &transaction.steps {
                    let TransactionStep::Transition(step) = step else {
                        continue;
                    };

                    transition_appliers
                        .entry(&step.transition)
                        .or_default()
                        .insert(operation_id);
                }
            }
        }

        let mut result_bindings: BTreeMap<&Id, &Id> = BTreeMap::new();

        for operation in model.operations.values() {
            for (_, step) in operation.program.steps_with_locations() {
                match step {
                    OperationStep::ExecuteEffect(step) => {
                        if let Some(result) = &step.result {
                            result_bindings.insert(result, &step.effect);
                        }
                    }

                    OperationStep::ExecuteEffectIntent(step) => {
                        if let (Some(result), Some(intent)) =
                            (&step.result, operation.effect_intents.get(&step.intent))
                        {
                            result_bindings.insert(result, &intent.effect);
                        }
                    }

                    _ => {}
                }
            }
        }

        Self {
            entries,
            transition_appliers,
            result_bindings,
        }
    }

    fn get(&self, id: &Id) -> Option<ReferenceInfo<'a>> {
        self.entries.get(id).copied()
    }

    /// The effect a result binding observes.
    fn binding_effect(&self, result: &Id) -> Option<&'a Id> {
        self.result_bindings.get(result).copied()
    }

    fn applies_transition(&self, operation: &Id, transition: &Id) -> bool {
        self.transition_appliers
            .get(transition)
            .is_some_and(|operations| operations.iter().any(|applier| *applier == operation))
    }

    /// Whether invocations evaluating a reference in `scope` belong to
    /// `operation`.
    fn scope_admits_operation(&self, scope: ValueScope<'_>, operation: &Id) -> bool {
        match scope {
            ValueScope::Operation(id) => id == operation,

            ValueScope::Transition(transition) => self.applies_transition(operation, transition),
        }
    }

    /// Whether invocations evaluating a reference in `scope` can observe
    /// the payload of an effect owned by `transition`.
    fn scope_admits_transition(&self, scope: ValueScope<'_>, transition: &Id) -> bool {
        match scope {
            ValueScope::Operation(operation) => self.applies_transition(operation, transition),

            ValueScope::Transition(id) => id == transition,
        }
    }
}

pub fn validate(model: &Model) -> Vec<ValidationError> {
    let errors = validate_global_id_uniqueness(model);

    if !errors.is_empty() {
        return errors;
    }

    let index = ReferenceIndex::build(model);

    let errors = validate_references(model, &index);

    if !errors.is_empty() {
        return errors;
    }

    let errors = validate_fragment_cycles(model);

    if !errors.is_empty() {
        return errors;
    }

    let mut errors = Vec::new();

    errors.extend(validate_data_models(model));

    errors.extend(validate_topics(model));

    errors.extend(validate_request_identity_shape(model));

    errors.extend(validate_state_machines(model));

    errors.extend(validate_transactions(model, &index));

    errors.extend(validate_effect_intents(model, &index));

    errors.extend(validate_result_bindings(model, &index));

    errors.extend(validate_programs(model, &index));

    errors.extend(validate_field_paths(model, &index));

    errors
}

pub fn validate_global_id_uniqueness(model: &Model) -> Vec<ValidationError> {
    let mut seen: BTreeMap<&Id, IdDeclaration> = BTreeMap::new();

    let mut errors = Vec::new();

    visit_declarations(model, |id, kind, owner| {
        let declaration = IdDeclaration {
            kind,
            owner: owner.cloned(),
        };

        if let Some(first) = seen.get(id) {
            errors.push(ValidationError::DuplicateId {
                id: id.clone(),
                first: first.clone(),
                second: declaration,
            });

            return;
        }

        seen.insert(id, declaration);
    });

    errors
}

fn validate_references(model: &Model, index: &ReferenceIndex<'_>) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    validate_schema_references(model, index, &mut errors);

    validate_data_model_references(model, index, &mut errors);

    validate_topic_references(model, index, &mut errors);

    validate_state_machine_references(model, index, &mut errors);

    validate_operation_references(model, index, &mut errors);

    errors
}

fn validate_fragment_cycles(model: &Model) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let mut complete = BTreeSet::<Id>::new();

    for start in model.schemas.keys() {
        if complete.contains(start) {
            continue;
        }

        let mut path = Vec::<Id>::new();
        let mut positions = BTreeMap::<Id, usize>::new();

        let mut current = start.clone();

        loop {
            if complete.contains(&current) {
                break;
            }

            if let Some(position) = positions.get(&current).copied() {
                let mut cycle = path[position..].to_vec();

                cycle.push(current.clone());

                errors.push(ValidationError::FragmentCycle { cycle });

                break;
            }

            let Some(Schema::Fragment(fragment)) = model.schemas.get(&current) else {
                break;
            };

            positions.insert(current.clone(), path.len());

            path.push(current.clone());
            current = fragment.source.clone();
        }

        complete.extend(path);
    }

    errors
}

fn validate_data_models(model: &Model) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for data_model in model.data_models.values() {
        for (object_id, object) in &data_model.objects {
            if !matches!(
                model.schemas.get(&object.schema),
                Some(Schema::Canonical(_))
            ) {
                errors.push(ValidationError::DataObjectSchemaNotCanonical {
                    object: object_id.clone(),
                    schema: object.schema.clone(),
                });
            }

            // Insert uniqueness and selector precision both rest on a
            // complete declared identity.
            if object.identity.is_empty() {
                errors.push(ValidationError::EmptyObjectIdentity {
                    object: object_id.clone(),
                });
            }
        }
    }

    errors
}

fn validate_topics(model: &Model) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    validate_subscription_topic_membership(model, &mut errors);

    validate_publication_topic_membership(model, &mut errors);

    validate_topic_ordering_shape(model, &mut errors);

    validate_message_identity_shape(model, &mut errors);

    errors
}

fn validate_message_identity_shape(model: &Model, errors: &mut Vec<ValidationError>) {
    for (topic_id, topic) in &model.topics {
        let MessageIdentity::Keyed { mapping } = &topic.message_identity else {
            continue;
        };

        // The mapping may cover a subset of the carried schemas;
        // identity is meaningful knowledge per schema, unlike the
        // ordering key.
        for schema in mapping.keys() {
            if !topic.messages.contains(schema) {
                errors.push(ValidationError::MessageIdentitySchemaNotOnTopic {
                    topic: topic_id.clone(),
                    schema: schema.clone(),
                });
            }
        }

        // Identity tuple positions correspond across schemas, so every
        // mapped tuple shares one arity. Empty tuples are reported on
        // their own and excluded from the arity baseline.
        let expected = mapping.values().map(Vec::len).find(|len| *len > 0);

        for (schema, identity) in mapping {
            if identity.is_empty() {
                errors.push(ValidationError::EmptyMessageIdentity {
                    topic: topic_id.clone(),
                    schema: schema.clone(),
                });

                continue;
            }

            if let Some(expected) = expected
                && identity.len() != expected
            {
                errors.push(ValidationError::MessageIdentityArityMismatch {
                    topic: topic_id.clone(),
                    schema: schema.clone(),
                    expected,
                    actual: identity.len(),
                });
            }
        }
    }
}

fn validate_request_identity_shape(model: &Model) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for operation in model.operations.values() {
        for (input_id, input) in &operation.inputs {
            let Input::Request(request) = input else {
                continue;
            };

            let RequestIdentity::Keyed { fields } = &request.identity else {
                continue;
            };

            if fields.is_empty() {
                errors.push(ValidationError::EmptyRequestIdentity {
                    input: input_id.clone(),
                });
            }
        }
    }

    errors
}

fn validate_state_machines(model: &Model) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for operation in model.operations.values() {
        for (transaction_id, transaction) in &operation.transactions {
            for step in &transaction.steps {
                let TransactionStep::Transition(step) = step else {
                    continue;
                };

                let machine = model
                    .state_machines
                    .get(&step.machine)
                    .expect("references already validated");

                let expected_object = match &machine.subject {
                    StateMachineSubject::Object { object, .. } => object,
                };

                if &step.subject.object != expected_object {
                    errors.push(ValidationError::StateTransitionSubjectMismatch {
                        transaction: transaction_id.clone(),
                        machine: step.machine.clone(),
                        expected_object: expected_object.clone(),
                        actual_object: step.subject.object.clone(),
                    });
                }
            }
        }
    }

    errors
}

fn validate_transactions(model: &Model, index: &ReferenceIndex<'_>) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for operation in model.operations.values() {
        for (transaction_id, transaction) in &operation.transactions {
            for step in &transaction.steps {
                match step {
                    TransactionStep::Read(read) => {
                        validate_transaction_object(
                            index,
                            transaction_id,
                            transaction,
                            &read.target.object,
                            &mut errors,
                        );
                    }

                    TransactionStep::Write(write) => {
                        validate_transaction_object(
                            index,
                            transaction_id,
                            transaction,
                            &write.target.object,
                            &mut errors,
                        );
                    }

                    TransactionStep::Insert(insert) => {
                        validate_transaction_object(
                            index,
                            transaction_id,
                            transaction,
                            &insert.object,
                            &mut errors,
                        );
                    }

                    TransactionStep::Delete(delete) => {
                        validate_transaction_object(
                            index,
                            transaction_id,
                            transaction,
                            &delete.target.object,
                            &mut errors,
                        );
                    }

                    TransactionStep::Lock(lock) => {
                        validate_transaction_object(
                            index,
                            transaction_id,
                            transaction,
                            &lock.target.object,
                            &mut errors,
                        );
                    }

                    TransactionStep::Transition(transition) => {
                        validate_transaction_object(
                            index,
                            transaction_id,
                            transaction,
                            &transition.subject.object,
                            &mut errors,
                        );

                        validate_transition_effect_values(
                            model,
                            transaction_id,
                            transition,
                            &mut errors,
                        );
                    }

                    TransactionStep::EstablishEffectIntent(step) => {
                        validate_established_intent(
                            index,
                            transaction_id,
                            operation,
                            &step.intent,
                            &mut errors,
                        );
                    }

                    TransactionStep::EstablishTransactionOutput(_) => {}
                }
            }
        }
    }

    errors
}

/// A transition side effect is established implicitly by a successful
/// transition, so it must not also be established explicitly.
fn validate_established_intent(
    index: &ReferenceIndex<'_>,
    transaction_id: &Id,
    operation: &Operation,
    intent_id: &Id,
    errors: &mut Vec<ValidationError>,
) {
    let intent = operation
        .effect_intents
        .get(intent_id)
        .expect("references already validated");

    let info = index
        .get(&intent.effect)
        .expect("references already validated");

    let Some(owner) = info.owner else {
        return;
    };

    let owner_info = index.get(owner).expect("effect owner exists");

    if owner_info.kind == ReferenceKind::Transition {
        errors.push(
            ValidationError::TransitionEffectIntentExplicitlyEstablished {
                transaction: transaction_id.clone(),
                intent: intent_id.clone(),
                effect: intent.effect.clone(),
            },
        );
    }
}

/// Applying a transition constructs one effect instance per declared
/// side effect, so the step must supply exactly one value derivation
/// for each of them — no more, no fewer.
fn validate_transition_effect_values(
    model: &Model,
    transaction_id: &Id,
    transition: &StateTransition,
    errors: &mut Vec<ValidationError>,
) {
    let machine = model
        .state_machines
        .get(&transition.machine)
        .expect("references already validated");

    let declaration = machine
        .transitions
        .get(&transition.transition)
        .expect("references already validated");

    let declared: BTreeSet<&Id> = declaration.side_effects.keys().collect();
    let provided: BTreeSet<&Id> = transition.effect_values.keys().collect();

    if declared == provided {
        return;
    }

    errors.push(ValidationError::TransitionEffectValuesMismatch {
        transaction: transaction_id.clone(),
        transition: transition.transition.clone(),
        missing: declared
            .difference(&provided)
            .map(|effect| (*effect).clone())
            .collect(),
        unexpected: provided
            .difference(&declared)
            .map(|effect| (*effect).clone())
            .collect(),
    });
}

/// A transition establishes exactly one logical intent for each of its
/// side effects, so an operation's handle on that artifact must be
/// unambiguous and must actually be establishable.
fn validate_effect_intents(model: &Model, index: &ReferenceIndex<'_>) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (operation_id, operation) in &model.operations {
        let mut by_transition_effect: BTreeMap<&Id, Vec<&Id>> = BTreeMap::new();

        for (intent_id, intent) in &operation.effect_intents {
            let info = index
                .get(&intent.effect)
                .expect("references already validated");

            let Some(owner) = info.owner else {
                continue;
            };

            let owner_info = index.get(owner).expect("effect owner exists");

            if owner_info.kind != ReferenceKind::Transition {
                continue;
            }

            by_transition_effect
                .entry(&intent.effect)
                .or_default()
                .push(intent_id);

            if !index.applies_transition(operation_id, owner) {
                errors.push(ValidationError::UnestablishableTransitionEffectIntent {
                    operation: operation_id.clone(),
                    intent: intent_id.clone(),
                    effect: intent.effect.clone(),
                    transition: owner.clone(),
                });
            }
        }

        for (effect, intents) in by_transition_effect {
            if intents.len() > 1 {
                errors.push(ValidationError::AmbiguousTransitionEffectIntent {
                    operation: operation_id.clone(),
                    effect: effect.clone(),
                    intents: intents.into_iter().cloned().collect(),
                });
            }
        }
    }

    errors
}

// Validate Field Paths

fn validate_field_paths(model: &Model, index: &ReferenceIndex<'_>) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    // Schema fragments.
    for (schema_id, schema) in &model.schemas {
        let Schema::Fragment(fragment) = schema else {
            continue;
        };

        for path in fragment.mapping.values() {
            validate_schema_path(model, schema_id, &fragment.source, path, &mut errors);
        }
    }

    // Data-object identities.
    for data_model in model.data_models.values() {
        for (object_id, object) in &data_model.objects {
            for path in &object.identity {
                validate_schema_path(model, object_id, &object.schema, path, &mut errors);
            }
        }
    }

    // Topic ordering fields.
    for (topic_id, topic) in &model.topics {
        let TopicOrdering::Keyed(key) = &topic.ordering else {
            continue;
        };

        for (schema, path) in &key.mapping {
            validate_schema_path(model, topic_id, schema, path, &mut errors);
        }
    }

    // Topic message-identity fields.
    for (topic_id, topic) in &model.topics {
        let MessageIdentity::Keyed { mapping } = &topic.message_identity else {
            continue;
        };

        for (schema, identity) in mapping {
            for path in identity {
                validate_schema_path(model, topic_id, schema, path, &mut errors);
            }
        }
    }

    // State-machine state fields.
    for (machine_id, machine) in &model.state_machines {
        match &machine.subject {
            StateMachineSubject::Object { object, state } => {
                validate_object_path(model, index, machine_id, object, state, &mut errors);
            }
        }
    }

    // Operations.
    for (operation_id, operation) in &model.operations {
        for (input_id, input) in &operation.inputs {
            let Input::Request(request) = input else {
                continue;
            };

            let RequestIdentity::Keyed { fields } = &request.identity else {
                continue;
            };

            for path in fields {
                validate_schema_path(model, input_id, &request.schema, path, &mut errors);
            }
        }

        for requirement in &operation.requirements.serialization {
            validate_value_ref_path(
                model,
                index,
                operation_id,
                ValueContext::operation(operation_id),
                &requirement.key,
                &mut errors,
            );
        }

        for requirement in &operation.requirements.ordering {
            validate_value_ref_path(
                model,
                index,
                operation_id,
                ValueContext::operation(operation_id),
                &requirement.key,
                &mut errors,
            );
        }

        for requirement in &operation.requirements.idempotency {
            validate_idempotency_key_paths(
                model,
                index,
                operation_id,
                ValueContext::operation(operation_id),
                &requirement.key,
                &mut errors,
            );
        }

        for requirement in &operation.requirements.recoverability {
            validate_idempotency_key_paths(
                model,
                index,
                operation_id,
                ValueContext::operation(operation_id),
                &requirement.key,
                &mut errors,
            );
        }

        for (effect_id, effect) in &operation.effects {
            validate_effect_paths(
                model,
                index,
                effect_id,
                ValueContext::operation(operation_id),
                effect,
                &mut errors,
            );
        }

        for (transaction_id, transaction) in &operation.transactions {
            validate_transaction_paths(
                model,
                index,
                operation_id,
                transaction_id,
                transaction,
                &mut errors,
            );
        }

        validate_program_paths(model, index, operation_id, &operation.program, &mut errors);
    }

    // Transition-owned effects.
    for machine in model.state_machines.values() {
        for (transition_id, transition) in &machine.transitions {
            for (effect_id, effect) in &transition.side_effects {
                validate_transition_effect_paths(
                    model,
                    index,
                    effect_id,
                    ValueContext::transition(transition_id),
                    effect,
                    &mut errors,
                );
            }
        }
    }

    errors
}

fn validate_schema_path(
    model: &Model,
    subject: &Id,
    schema: &Id,
    path: &FieldPath,
    errors: &mut Vec<ValidationError>,
) {
    if !schema_path_resolves(model, schema, path) {
        errors.push(ValidationError::InvalidFieldPath {
            subject: subject.clone(),
            schema: schema.clone(),
            path: path.clone(),
        });
    }
}

fn validate_object_path(
    model: &Model,
    index: &ReferenceIndex<'_>,
    subject: &Id,
    object: &Id,
    path: &FieldPath,
    errors: &mut Vec<ValidationError>,
) {
    let schema = object_schema(model, index, object);

    validate_schema_path(model, subject, schema, path, errors);
}

fn validate_value_ref_path(
    model: &Model,
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    value: &ValueRef,
    errors: &mut Vec<ValidationError>,
) {
    match &value.source {
        ValueSource::Input(input_id) => {
            let input = find_input(model, index, input_id).expect("references already validated");

            match input {
                Input::Request(request) => {
                    validate_schema_path(model, subject, &request.schema, &value.path, errors);
                }

                Input::Subscription(subscription) => {
                    let topic = model
                        .topics
                        .get(&subscription.topic)
                        .expect("references already validated");

                    match &subscription.messages {
                        MessageSelector::All => {
                            for schema in &topic.messages {
                                validate_schema_path(model, subject, schema, &value.path, errors);
                            }
                        }

                        MessageSelector::Only(messages) => {
                            for schema in messages {
                                validate_schema_path(model, subject, schema, &value.path, errors);
                            }
                        }
                    }
                }
            }
        }

        ValueSource::Effect(effect_id) => {
            let Some(schema) = effect_schema(model, index, effect_id) else {
                errors.push(ValidationError::ValueSourceHasNoSchema {
                    subject: subject.clone(),
                    source: effect_id.clone(),
                });

                return;
            };

            validate_schema_path(model, subject, schema, &value.path, errors);
        }

        ValueSource::TransactionOutput(output_id) => {
            let info = index.get(output_id).expect("references already validated");

            let operation_id = info.owner.expect("transaction output has an owner");

            let output = model
                .operations
                .get(operation_id)
                .expect("owner operation exists")
                .transaction_outputs
                .get(output_id)
                .expect("transaction output exists");

            validate_schema_path(model, subject, &output.schema, &value.path, errors);
        }

        ValueSource::EffectResultOk(result_id) | ValueSource::EffectResultErr(result_id) => {
            let variant = match &value.source {
                ValueSource::EffectResultOk(_) => ResultVariant::Ok,
                _ => ResultVariant::Err,
            };

            // A binding on a resultless effect is reported by the
            // result-binding pass; there is no schema to resolve here.
            let Some(schema) = index
                .binding_effect(result_id)
                .and_then(|effect| effect_result_type(model, index, effect))
                .map(|result| result.schema(variant).clone())
            else {
                return;
            };

            validate_schema_path(model, subject, &schema, &value.path, errors);
        }

        ValueSource::StateMachineSubject(machine_id) => {
            let machine = model
                .state_machines
                .get(machine_id)
                .expect("references already validated");

            let object = match &machine.subject {
                StateMachineSubject::Object { object, .. } => object,
            };

            validate_object_path(model, index, subject, object, &value.path, errors);
        }

        ValueSource::TransactionRead(result_id) => {
            // Structural defects are reported by the reference pass.
            let Some(scope) = context.transaction else {
                return;
            };

            let Some((_, read)) = scope.read(result_id) else {
                return;
            };

            if let FieldSelection::Only(fields) = &read.fields
                && !fields
                    .iter()
                    .any(|field| field_selection_covers(field, &value.path))
            {
                errors.push(ValidationError::TransactionReadFieldNotSelected {
                    transaction: scope.id.clone(),
                    read: result_id.clone(),
                    path: value.path.clone(),
                });

                return;
            }

            validate_object_path(
                model,
                index,
                subject,
                &read.target.object,
                &value.path,
                errors,
            );
        }
    }
}

/// A read of a field also observes the values nested beneath it.
fn field_selection_covers(selected: &FieldPath, path: &FieldPath) -> bool {
    path.0.starts_with(&selected.0)
}

fn validate_derivation_paths(
    model: &Model,
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    derivation: &Derivation,
    errors: &mut Vec<ValidationError>,
) {
    let Derivation::Deterministic { from } = derivation else {
        return;
    };

    for value in from {
        validate_value_ref_path(model, index, subject, context, value, errors);
    }
}

fn validate_idempotency_key_paths(
    model: &Model,
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    key: &IdempotencyKey,
    errors: &mut Vec<ValidationError>,
) {
    for component in &key.components {
        validate_value_ref_path(model, index, subject, context, component, errors);
    }
}

fn validate_propagation_paths(
    model: &Model,
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    propagations: &[IdempotencyKeyPropagation],
    errors: &mut Vec<ValidationError>,
) {
    for propagation in propagations {
        validate_idempotency_key_paths(model, index, subject, context, &propagation.source, errors);

        validate_idempotency_key_paths(model, index, subject, context, &propagation.target, errors);
    }
}

fn validate_effect_paths(
    model: &Model,
    index: &ReferenceIndex<'_>,
    effect_id: &Id,
    context: ValueContext<'_>,
    effect: &Effect,
    errors: &mut Vec<ValidationError>,
) {
    match effect {
        Effect::Publication(effect) => {
            validate_propagation_paths(
                model,
                index,
                effect_id,
                context,
                &effect.idempotency_key_propagation,
                errors,
            );
        }

        Effect::Request(effect) => {
            validate_propagation_paths(
                model,
                index,
                effect_id,
                context,
                &effect.idempotency_key_propagation,
                errors,
            );
        }

        Effect::External(effect) => {
            if let IdempotencyGuarantee::DeduplicatedBy { key } = &effect.idempotency {
                validate_idempotency_key_paths(model, index, effect_id, context, key, errors);
            }
        }
    }
}

fn validate_transition_effect_paths(
    model: &Model,
    index: &ReferenceIndex<'_>,
    effect_id: &Id,
    context: ValueContext<'_>,
    effect: &TransitionSideEffect,
    errors: &mut Vec<ValidationError>,
) {
    match effect {
        TransitionSideEffect::Publication(effect) => {
            validate_propagation_paths(
                model,
                index,
                effect_id,
                context,
                &effect.idempotency_key_propagation,
                errors,
            );
        }

        TransitionSideEffect::Request(effect) => {
            validate_propagation_paths(
                model,
                index,
                effect_id,
                context,
                &effect.idempotency_key_propagation,
                errors,
            );
        }
    }
}

fn validate_transaction_paths(
    model: &Model,
    index: &ReferenceIndex<'_>,
    operation_id: &Id,
    transaction_id: &Id,
    transaction: &Transaction,
    errors: &mut Vec<ValidationError>,
) {
    let operation = ValueContext::operation(operation_id);

    // The commit key is evaluated for the invocation before the body
    // executes, so it may not observe transaction state.
    if let IdempotencyGuarantee::DeduplicatedBy { key } = &transaction.idempotency {
        validate_idempotency_key_paths(model, index, transaction_id, operation, key, errors);
    }

    for (step_index, step) in transaction.steps.iter().enumerate() {
        let context = operation.in_transaction(transaction_id, transaction, step_index);

        match step {
            TransactionStep::Read(read) => {
                validate_selector_paths(
                    model,
                    index,
                    transaction_id,
                    context,
                    &read.target,
                    errors,
                );

                if let FieldSelection::Only(fields) = &read.fields {
                    for field in fields {
                        validate_object_path(
                            model,
                            index,
                            transaction_id,
                            &read.target.object,
                            field,
                            errors,
                        );
                    }
                }
            }

            TransactionStep::Write(write) => {
                validate_selector_paths(
                    model,
                    index,
                    transaction_id,
                    context,
                    &write.target,
                    errors,
                );

                for field in &write.fields {
                    validate_object_path(
                        model,
                        index,
                        transaction_id,
                        &write.target.object,
                        field,
                        errors,
                    );
                }

                validate_derivation_paths(
                    model,
                    index,
                    transaction_id,
                    context,
                    &write.values,
                    errors,
                );
            }

            TransactionStep::Insert(insert) => {
                validate_derivation_paths(
                    model,
                    index,
                    transaction_id,
                    context,
                    &insert.values,
                    errors,
                );
            }

            TransactionStep::Delete(delete) => {
                validate_selector_paths(
                    model,
                    index,
                    transaction_id,
                    context,
                    &delete.target,
                    errors,
                );
            }

            TransactionStep::Lock(lock) => {
                validate_selector_paths(
                    model,
                    index,
                    transaction_id,
                    context,
                    &lock.target,
                    errors,
                );

                if let LockOrder::By(terms) = &lock.order {
                    for term in terms {
                        validate_object_path(
                            model,
                            index,
                            transaction_id,
                            &lock.target.object,
                            &term.field,
                            errors,
                        );
                    }
                }
            }

            TransactionStep::Transition(transition) => {
                validate_selector_paths(
                    model,
                    index,
                    transaction_id,
                    context,
                    &transition.subject,
                    errors,
                );

                for values in transition.effect_values.values() {
                    validate_derivation_paths(
                        model,
                        index,
                        transaction_id,
                        context,
                        values,
                        errors,
                    );
                }
            }

            TransactionStep::EstablishEffectIntent(step) => {
                validate_derivation_paths(
                    model,
                    index,
                    transaction_id,
                    context,
                    &step.values,
                    errors,
                );
            }

            TransactionStep::EstablishTransactionOutput(step) => {
                validate_derivation_paths(
                    model,
                    index,
                    transaction_id,
                    context,
                    &step.values,
                    errors,
                );
            }
        }
    }
}

fn validate_selector_paths(
    model: &Model,
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    selector: &ObjectSelector,
    errors: &mut Vec<ValidationError>,
) {
    validate_predicate_paths(
        model,
        index,
        subject,
        context,
        &selector.object,
        &selector.predicate,
        errors,
    );
}

fn validate_predicate_paths(
    model: &Model,
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    object: &Id,
    predicate: &SelectorPredicate,
    errors: &mut Vec<ValidationError>,
) {
    match predicate {
        SelectorPredicate::All => {}

        SelectorPredicate::Eq { field, value } => {
            validate_object_path(model, index, subject, object, field, errors);

            if let SelectorValue::Value(value) = value {
                validate_value_ref_path(model, index, subject, context, value, errors);
            }
        }

        SelectorPredicate::And { predicates } => {
            for predicate in predicates {
                validate_predicate_paths(model, index, subject, context, object, predicate, errors);
            }
        }
    }
}

// End Validate Field Paths

fn validate_transaction_object(
    index: &ReferenceIndex<'_>,
    transaction_id: &Id,
    transaction: &Transaction,
    object: &Id,
    errors: &mut Vec<ValidationError>,
) {
    let info = index.get(object).expect("references already validated");

    let Some(data_model) = &transaction.data_model else {
        errors.push(ValidationError::TransactionMissingDataModel {
            transaction: transaction_id.clone(),
            object: object.clone(),
        });

        return;
    };

    if info.owner != Some(data_model) {
        errors.push(ValidationError::TransactionObjectOutsideDataModel {
            transaction: transaction_id.clone(),
            data_model: data_model.clone(),
            object: object.clone(),
        });
    }
}

fn validate_subscription_topic_membership(model: &Model, errors: &mut Vec<ValidationError>) {
    for operation in model.operations.values() {
        for (input_id, input) in &operation.inputs {
            let Input::Subscription(subscription) = input else {
                continue;
            };

            let topic = model
                .topics
                .get(&subscription.topic)
                .expect("references already validated");

            let MessageSelector::Only(messages) = &subscription.messages else {
                continue;
            };

            for schema in messages {
                if !topic.messages.contains(schema) {
                    errors.push(ValidationError::SubscriptionMessageNotOnTopic {
                        input: input_id.clone(),
                        topic: subscription.topic.clone(),
                        schema: schema.clone(),
                    });
                }
            }
        }
    }
}

fn validate_publication_topic_membership(model: &Model, errors: &mut Vec<ValidationError>) {
    for operation in model.operations.values() {
        for (effect_id, effect) in &operation.effects {
            if let Effect::Publication(publication) = effect {
                validate_publication_membership(model, effect_id, publication, errors);
            }
        }
    }

    for machine in model.state_machines.values() {
        for transition in machine.transitions.values() {
            for (effect_id, effect) in &transition.side_effects {
                if let TransitionSideEffect::Publication(publication) = effect {
                    validate_publication_membership(model, effect_id, publication, errors);
                }
            }
        }
    }
}

fn validate_topic_ordering_shape(model: &Model, errors: &mut Vec<ValidationError>) {
    for (topic_id, topic) in &model.topics {
        let TopicOrdering::Keyed(key) = &topic.ordering else {
            continue;
        };

        for schema in key.mapping.keys() {
            if !topic.messages.contains(schema) {
                errors.push(ValidationError::TopicKeySchemaNotOnTopic {
                    topic: topic_id.clone(),
                    schema: schema.clone(),
                });
            }
        }

        for schema in &topic.messages {
            if !key.mapping.contains_key(schema) {
                errors.push(ValidationError::TopicKeyMissingSchema {
                    topic: topic_id.clone(),
                    schema: schema.clone(),
                });
            }
        }
    }
}

fn validate_publication_membership(
    model: &Model,
    effect_id: &Id,
    effect: &PublicationEffect,
    errors: &mut Vec<ValidationError>,
) {
    let topic = model
        .topics
        .get(&effect.topic)
        .expect("references already validated");

    if !topic.messages.contains(&effect.schema) {
        errors.push(ValidationError::PublicationEffectMessageNotOnTopic {
            effect: effect_id.clone(),
            topic: effect.topic.clone(),
            schema: effect.schema.clone(),
        });
    }
}

fn validate_schema_references(
    model: &Model,
    index: &ReferenceIndex<'_>,
    errors: &mut Vec<ValidationError>,
) {
    for (schema_id, schema) in &model.schemas {
        match schema {
            Schema::Canonical(schema) => {
                for field in schema.fields.values() {
                    validate_type_ref_references(schema_id, &field.ty, index, errors);
                }
            }

            Schema::Fragment(fragment) => {
                expect_reference(
                    index,
                    schema_id,
                    &fragment.source,
                    ReferenceKind::Schema,
                    errors,
                );
            }
        }
    }
}

fn validate_data_model_references(
    model: &Model,
    index: &ReferenceIndex<'_>,
    errors: &mut Vec<ValidationError>,
) {
    for data_model in model.data_models.values() {
        for (object_id, object) in &data_model.objects {
            expect_reference(
                index,
                object_id,
                &object.schema,
                ReferenceKind::Schema,
                errors,
            );
        }
    }
}

fn validate_topic_references(
    model: &Model,
    index: &ReferenceIndex<'_>,
    errors: &mut Vec<ValidationError>,
) {
    for (topic_id, topic) in &model.topics {
        for schema in &topic.messages {
            expect_reference(index, topic_id, schema, ReferenceKind::Schema, errors);
        }

        if let TopicOrdering::Keyed(key) = &topic.ordering {
            for schema in key.mapping.keys() {
                expect_reference(index, topic_id, schema, ReferenceKind::Schema, errors);
            }
        }

        if let MessageIdentity::Keyed { mapping } = &topic.message_identity {
            for schema in mapping.keys() {
                expect_reference(index, topic_id, schema, ReferenceKind::Schema, errors);
            }
        }
    }
}

fn validate_state_machine_references(
    model: &Model,
    index: &ReferenceIndex<'_>,
    errors: &mut Vec<ValidationError>,
) {
    for (machine_id, machine) in &model.state_machines {
        match &machine.subject {
            StateMachineSubject::Object { object, .. } => {
                expect_reference(index, machine_id, object, ReferenceKind::DataObject, errors);
            }
        }

        expect_owned_reference(
            index,
            machine_id,
            &machine.initial,
            ReferenceKind::State,
            machine_id,
            errors,
        );

        for (transition_id, transition) in &machine.transitions {
            for state in &transition.from {
                expect_owned_reference(
                    index,
                    transition_id,
                    state,
                    ReferenceKind::State,
                    machine_id,
                    errors,
                );
            }

            expect_owned_reference(
                index,
                transition_id,
                &transition.to,
                ReferenceKind::State,
                machine_id,
                errors,
            );

            for (effect_id, effect) in &transition.side_effects {
                validate_transition_effect_references(
                    model,
                    index,
                    effect_id,
                    ValueContext::transition(transition_id),
                    effect,
                    errors,
                );
            }
        }
    }
}

fn validate_operation_references(
    model: &Model,
    index: &ReferenceIndex<'_>,
    errors: &mut Vec<ValidationError>,
) {
    for (operation_id, operation) in &model.operations {
        expect_reference(
            index,
            operation_id,
            &operation.service,
            ReferenceKind::Service,
            errors,
        );

        for (input_id, input) in &operation.inputs {
            validate_input_references(index, input_id, input, errors);
        }

        for (effect_id, effect) in &operation.effects {
            validate_effect_references(
                model,
                index,
                effect_id,
                ValueContext::operation(operation_id),
                effect,
                errors,
            );
        }

        for (intent_id, intent) in &operation.effect_intents {
            // Effect may be transition-owned.
            expect_reference(
                index,
                intent_id,
                &intent.effect,
                ReferenceKind::Effect,
                errors,
            );
        }

        for (output_id, output) in &operation.transaction_outputs {
            expect_reference(
                index,
                output_id,
                &output.schema,
                ReferenceKind::Schema,
                errors,
            );
        }

        for (transaction_id, transaction) in &operation.transactions {
            if let Some(data_model) = &transaction.data_model {
                expect_reference(
                    index,
                    transaction_id,
                    data_model,
                    ReferenceKind::DataModel,
                    errors,
                );
            }

            if let IdempotencyGuarantee::DeduplicatedBy { key } = &transaction.idempotency {
                validate_idempotency_key_references(
                    index,
                    transaction_id,
                    ValueContext::operation(operation_id),
                    key,
                    errors,
                );
            }

            validate_transaction_references(
                index,
                operation_id,
                transaction_id,
                transaction,
                errors,
            );
        }

        validate_program_references(model, index, operation_id, &operation.program, errors);

        for requirement in &operation.requirements.serialization {
            validate_value_ref_reference(
                index,
                operation_id,
                ValueContext::operation(operation_id),
                &requirement.key,
                errors,
            );
        }

        for requirement in &operation.requirements.ordering {
            validate_value_ref_reference(
                index,
                operation_id,
                ValueContext::operation(operation_id),
                &requirement.key,
                errors,
            );
        }

        for requirement in &operation.requirements.idempotency {
            validate_idempotency_key_references(
                index,
                operation_id,
                ValueContext::operation(operation_id),
                &requirement.key,
                errors,
            );
        }

        for requirement in &operation.requirements.recoverability {
            validate_idempotency_key_references(
                index,
                operation_id,
                ValueContext::operation(operation_id),
                &requirement.key,
                errors,
            );
        }
    }
}

fn validate_effect_references(
    model: &Model,
    index: &ReferenceIndex<'_>,
    effect_id: &Id,
    context: ValueContext<'_>,
    effect: &Effect,
    errors: &mut Vec<ValidationError>,
) {
    match effect {
        Effect::Publication(publication) => {
            validate_publication_references(index, effect_id, context, publication, errors);
        }

        Effect::Request(request) => {
            validate_request_effect_references(model, index, effect_id, context, request, errors);
        }

        Effect::External(external) => {
            if let IdempotencyGuarantee::DeduplicatedBy { key } = &external.idempotency {
                validate_idempotency_key_references(index, effect_id, context, key, errors);
            }

            if let Some(result) = &external.result {
                validate_result_type_references(index, effect_id, result, errors);
            }
        }
    }
}

fn validate_result_type_references(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    result: &ResultType,
    errors: &mut Vec<ValidationError>,
) {
    expect_reference(index, subject, &result.ok, ReferenceKind::Schema, errors);

    expect_reference(index, subject, &result.err, ReferenceKind::Schema, errors);
}

fn validate_input_references(
    index: &ReferenceIndex<'_>,
    input_id: &Id,
    input: &Input,
    errors: &mut Vec<ValidationError>,
) {
    match input {
        Input::Request(request) => {
            expect_reference(
                index,
                input_id,
                &request.schema,
                ReferenceKind::Schema,
                errors,
            );

            validate_result_type_references(index, input_id, &request.result, errors);
        }

        Input::Subscription(subscription) => {
            expect_reference(
                index,
                input_id,
                &subscription.topic,
                ReferenceKind::Topic,
                errors,
            );

            if let MessageSelector::Only(messages) = &subscription.messages {
                for schema in messages {
                    expect_reference(index, input_id, schema, ReferenceKind::Schema, errors);
                }
            }
        }
    }
}

fn validate_transaction_references(
    index: &ReferenceIndex<'_>,
    operation_id: &Id,
    transaction_id: &Id,
    transaction: &Transaction,
    errors: &mut Vec<ValidationError>,
) {
    for (step_index, step) in transaction.steps.iter().enumerate() {
        let context = ValueContext::operation(operation_id).in_transaction(
            transaction_id,
            transaction,
            step_index,
        );

        match step {
            TransactionStep::Read(read) => {
                validate_selector_references(index, transaction_id, context, &read.target, errors);
            }

            TransactionStep::Write(write) => {
                validate_selector_references(index, transaction_id, context, &write.target, errors);

                validate_derivation_references(
                    index,
                    transaction_id,
                    context,
                    &write.values,
                    errors,
                );
            }

            TransactionStep::Insert(insert) => {
                expect_reference(
                    index,
                    transaction_id,
                    &insert.object,
                    ReferenceKind::DataObject,
                    errors,
                );

                validate_derivation_references(
                    index,
                    transaction_id,
                    context,
                    &insert.values,
                    errors,
                );
            }

            TransactionStep::Delete(delete) => {
                validate_selector_references(
                    index,
                    transaction_id,
                    context,
                    &delete.target,
                    errors,
                );
            }

            TransactionStep::Lock(lock) => {
                validate_selector_references(index, transaction_id, context, &lock.target, errors);
            }

            TransactionStep::Transition(transition) => {
                let machine_valid = expect_reference(
                    index,
                    transaction_id,
                    &transition.machine,
                    ReferenceKind::StateMachine,
                    errors,
                );

                let transition_valid = expect_reference(
                    index,
                    transaction_id,
                    &transition.transition,
                    ReferenceKind::Transition,
                    errors,
                );

                if machine_valid && transition_valid {
                    expect_owned_by(
                        index,
                        transaction_id,
                        &transition.transition,
                        &transition.machine,
                        errors,
                    );
                }

                validate_selector_references(
                    index,
                    transaction_id,
                    context,
                    &transition.subject,
                    errors,
                );

                // Side-effect instances are constructed when this step
                // applies the transition, so their derivations are
                // evaluated in the enclosing transaction context.
                for values in transition.effect_values.values() {
                    validate_derivation_references(index, transaction_id, context, values, errors);
                }
            }

            TransactionStep::EstablishEffectIntent(step) => {
                expect_owned_reference(
                    index,
                    transaction_id,
                    &step.intent,
                    ReferenceKind::EffectIntent,
                    operation_id,
                    errors,
                );

                validate_derivation_references(
                    index,
                    transaction_id,
                    context,
                    &step.values,
                    errors,
                );
            }

            TransactionStep::EstablishTransactionOutput(step) => {
                expect_owned_reference(
                    index,
                    transaction_id,
                    &step.output,
                    ReferenceKind::TransactionOutput,
                    operation_id,
                    errors,
                );

                validate_derivation_references(
                    index,
                    transaction_id,
                    context,
                    &step.values,
                    errors,
                );
            }
        }
    }
}

/// References made by the operation program, in the operation value
/// context: a transition side effect is established as an intent and
/// executed through `execute_effect_intent`, so a direct execution
/// must name an operation-owned effect, and a `return` must name an
/// operation-owned request input.
fn validate_program_references(
    model: &Model,
    index: &ReferenceIndex<'_>,
    operation_id: &Id,
    program: &OperationBlock,
    errors: &mut Vec<ValidationError>,
) {
    let context = ValueContext::operation(operation_id);

    for (_, step) in program.steps_with_locations() {
        match step {
            OperationStep::Transaction(step) => {
                expect_owned_reference(
                    index,
                    operation_id,
                    &step.transaction,
                    ReferenceKind::Transaction,
                    operation_id,
                    errors,
                );
            }

            OperationStep::ExecuteEffect(step) => {
                expect_owned_reference(
                    index,
                    operation_id,
                    &step.effect,
                    ReferenceKind::Effect,
                    operation_id,
                    errors,
                );

                // The effect instance is constructed at program level,
                // so its derivation is evaluated with no transaction in
                // scope.
                validate_derivation_references(index, operation_id, context, &step.values, errors);
            }

            OperationStep::ExecuteEffectIntent(step) => {
                expect_owned_reference(
                    index,
                    operation_id,
                    &step.intent,
                    ReferenceKind::EffectIntent,
                    operation_id,
                    errors,
                );
            }

            OperationStep::MatchResult(step) => {
                expect_owned_reference(
                    index,
                    operation_id,
                    &step.result,
                    ReferenceKind::EffectResult,
                    operation_id,
                    errors,
                );
            }

            OperationStep::Branch(step) => {
                for root in step.condition.roots() {
                    validate_value_ref_reference(index, operation_id, context, root, errors);
                }
            }

            OperationStep::Return(step) => {
                if expect_owned_reference(
                    index,
                    operation_id,
                    &step.request,
                    ReferenceKind::Input,
                    operation_id,
                    errors,
                ) {
                    validate_request_input_kind(model, index, operation_id, &step.request, errors);
                }

                validate_derivation_references(
                    index,
                    operation_id,
                    context,
                    step.outcome.values(),
                    errors,
                );
            }

            OperationStep::Complete => {}
        }
    }
}

fn validate_selector_references(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    selector: &ObjectSelector,
    errors: &mut Vec<ValidationError>,
) {
    expect_reference(
        index,
        subject,
        &selector.object,
        ReferenceKind::DataObject,
        errors,
    );

    validate_predicate_references(index, subject, context, &selector.predicate, errors);
}

fn validate_predicate_references(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    predicate: &SelectorPredicate,
    errors: &mut Vec<ValidationError>,
) {
    match predicate {
        SelectorPredicate::All => {}

        SelectorPredicate::Eq { value, .. } => {
            if let SelectorValue::Value(value) = value {
                validate_value_ref_reference(index, subject, context, value, errors);
            }
        }

        SelectorPredicate::And { predicates } => {
            for predicate in predicates {
                validate_predicate_references(index, subject, context, predicate, errors);
            }
        }
    }
}

fn validate_derivation_references(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    derivation: &Derivation,
    errors: &mut Vec<ValidationError>,
) {
    let Derivation::Deterministic { from } = derivation else {
        return;
    };

    for value in from {
        validate_value_ref_reference(index, subject, context, value, errors);
    }
}

fn validate_transition_effect_references(
    model: &Model,
    index: &ReferenceIndex<'_>,
    effect_id: &Id,
    context: ValueContext<'_>,
    effect: &TransitionSideEffect,
    errors: &mut Vec<ValidationError>,
) {
    match effect {
        TransitionSideEffect::Publication(publication) => {
            validate_publication_references(index, effect_id, context, publication, errors);
        }

        TransitionSideEffect::Request(request) => {
            validate_request_effect_references(model, index, effect_id, context, request, errors);
        }
    }
}

fn validate_request_effect_references(
    model: &Model,
    index: &ReferenceIndex<'_>,
    effect_id: &Id,
    context: ValueContext<'_>,
    effect: &RequestEffect,
    errors: &mut Vec<ValidationError>,
) {
    expect_reference(
        index,
        effect_id,
        &effect.schema,
        ReferenceKind::Schema,
        errors,
    );

    let operation_valid = expect_reference(
        index,
        effect_id,
        &effect.target.operation,
        ReferenceKind::Operation,
        errors,
    );

    let input_valid = expect_reference(
        index,
        effect_id,
        &effect.target.input,
        ReferenceKind::Input,
        errors,
    );

    if operation_valid && input_valid {
        expect_owned_by(
            index,
            effect_id,
            &effect.target.input,
            &effect.target.operation,
            errors,
        );
    }

    if input_valid {
        validate_request_input_kind(model, index, effect_id, &effect.target.input, errors);
    }

    for propagation in &effect.idempotency_key_propagation {
        validate_idempotency_key_references(index, effect_id, context, &propagation.source, errors);

        validate_idempotency_key_references(index, effect_id, context, &propagation.target, errors);
    }
}

fn validate_publication_references(
    index: &ReferenceIndex<'_>,
    effect_id: &Id,
    context: ValueContext<'_>,
    effect: &PublicationEffect,
    errors: &mut Vec<ValidationError>,
) {
    expect_reference(
        index,
        effect_id,
        &effect.topic,
        ReferenceKind::Topic,
        errors,
    );

    expect_reference(
        index,
        effect_id,
        &effect.schema,
        ReferenceKind::Schema,
        errors,
    );

    for propagation in &effect.idempotency_key_propagation {
        validate_idempotency_key_references(index, effect_id, context, &propagation.source, errors);

        validate_idempotency_key_references(index, effect_id, context, &propagation.target, errors);
    }
}

fn validate_idempotency_key_references(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    key: &IdempotencyKey,
    errors: &mut Vec<ValidationError>,
) {
    for component in &key.components {
        validate_value_ref_reference(index, subject, context, component, errors);
    }
}

fn validate_value_ref_reference(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    value: &ValueRef,
    errors: &mut Vec<ValidationError>,
) {
    match &value.source {
        ValueSource::Input(input) => {
            if expect_reference(index, subject, input, ReferenceKind::Input, errors) {
                expect_in_scope(index, subject, context, input, errors);
            }
        }

        ValueSource::Effect(effect) => {
            if expect_reference(index, subject, effect, ReferenceKind::Effect, errors) {
                expect_in_scope(index, subject, context, effect, errors);
            }
        }

        ValueSource::TransactionOutput(output) => {
            if expect_reference(
                index,
                subject,
                output,
                ReferenceKind::TransactionOutput,
                errors,
            ) {
                expect_in_scope(index, subject, context, output, errors);
            }
        }

        ValueSource::EffectResultOk(result) | ValueSource::EffectResultErr(result) => {
            if expect_reference(index, subject, result, ReferenceKind::EffectResult, errors) {
                expect_in_scope(index, subject, context, result, errors);
            }
        }

        // State machines are global: any operation may address the
        // objects they govern.
        ValueSource::StateMachineSubject(machine) => {
            expect_reference(index, subject, machine, ReferenceKind::StateMachine, errors);
        }

        ValueSource::TransactionRead(read) => {
            if !expect_reference(index, subject, read, ReferenceKind::TransactionRead, errors) {
                return;
            }

            let Some(scope) = context.transaction else {
                errors.push(ValidationError::TransactionReadOutsideTransaction {
                    subject: subject.clone(),
                    read: read.clone(),
                });

                return;
            };

            if !expect_owned_by(index, subject, read, scope.id, errors) {
                return;
            }

            let Some((position, _)) = scope.read(read) else {
                return;
            };

            if position >= scope.step {
                errors.push(ValidationError::TransactionReadOutOfOrder {
                    transaction: scope.id.clone(),
                    read: read.clone(),
                });
            }
        }
    }
}

fn validate_type_ref_references(
    subject: &Id,
    ty: &TypeRef,
    index: &ReferenceIndex<'_>,
    errors: &mut Vec<ValidationError>,
) {
    match ty {
        TypeRef::Scalar(_) => {}

        TypeRef::Schema(schema) => {
            expect_reference(index, subject, schema, ReferenceKind::Schema, errors);
        }

        TypeRef::List(inner) => {
            validate_type_ref_references(subject, inner, index, errors);
        }
    }
}

fn validate_request_input_kind(
    model: &Model,
    index: &ReferenceIndex<'_>,
    subject: &Id,
    input_id: &Id,
    errors: &mut Vec<ValidationError>,
) {
    let Some(input) = find_input(model, index, input_id) else {
        return;
    };

    let actual = input_kind(input);

    if actual != InputKind::Request {
        errors.push(ValidationError::InvalidInputKind {
            subject: subject.clone(),
            input: input_id.clone(),
            expected: InputKind::Request,
            actual,
        });
    }
}

fn visit_declarations<'a>(
    model: &'a Model,
    mut visit: impl FnMut(&'a Id, ReferenceKind, Option<&'a Id>),
) {
    for id in model.services.keys() {
        visit(id, ReferenceKind::Service, None);
    }

    for id in model.schemas.keys() {
        visit(id, ReferenceKind::Schema, None);
    }

    for (data_model_id, data_model) in &model.data_models {
        visit(data_model_id, ReferenceKind::DataModel, None);

        for object_id in data_model.objects.keys() {
            visit(object_id, ReferenceKind::DataObject, Some(data_model_id));
        }
    }

    for id in model.topics.keys() {
        visit(id, ReferenceKind::Topic, None);
    }

    for (machine_id, machine) in &model.state_machines {
        visit(machine_id, ReferenceKind::StateMachine, None);

        for state_id in &machine.states {
            visit(state_id, ReferenceKind::State, Some(machine_id));
        }

        for (transition_id, transition) in &machine.transitions {
            visit(transition_id, ReferenceKind::Transition, Some(machine_id));

            for effect_id in transition.side_effects.keys() {
                visit(effect_id, ReferenceKind::Effect, Some(transition_id));
            }
        }
    }

    for (operation_id, operation) in &model.operations {
        visit(operation_id, ReferenceKind::Operation, None);

        for input_id in operation.inputs.keys() {
            visit(input_id, ReferenceKind::Input, Some(operation_id));
        }

        for effect_id in operation.effects.keys() {
            visit(effect_id, ReferenceKind::Effect, Some(operation_id));
        }

        for intent_id in operation.effect_intents.keys() {
            visit(intent_id, ReferenceKind::EffectIntent, Some(operation_id));
        }

        for output_id in operation.transaction_outputs.keys() {
            visit(
                output_id,
                ReferenceKind::TransactionOutput,
                Some(operation_id),
            );
        }

        for (transaction_id, transaction) in &operation.transactions {
            visit(
                transaction_id,
                ReferenceKind::Transaction,
                Some(operation_id),
            );

            for step in &transaction.steps {
                let TransactionStep::Read(read) = step else {
                    continue;
                };

                visit(
                    &read.result,
                    ReferenceKind::TransactionRead,
                    Some(transaction_id),
                );
            }
        }

        // A result binding is declared by the program step that binds it.
        for (_, step) in operation.program.steps_with_locations() {
            if let Some(result) = step_result_binding(step) {
                visit(result, ReferenceKind::EffectResult, Some(operation_id));
            }
        }
    }
}

/// The result binding an effect-executing step declares, if any.
fn step_result_binding(step: &OperationStep) -> Option<&Id> {
    match step {
        OperationStep::ExecuteEffect(step) => step.result.as_ref(),
        OperationStep::ExecuteEffectIntent(step) => step.result.as_ref(),
        _ => None,
    }
}

fn expect_owned_reference(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    reference: &Id,
    expected_kind: ReferenceKind,
    expected_owner: &Id,
    errors: &mut Vec<ValidationError>,
) -> bool {
    if !expect_reference(index, subject, reference, expected_kind, errors) {
        return false;
    }

    expect_owned_by(index, subject, reference, expected_owner, errors)
}

fn expect_reference(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    reference: &Id,
    expected: ReferenceKind,
    errors: &mut Vec<ValidationError>,
) -> bool {
    let Some(info) = index.get(reference) else {
        errors.push(ValidationError::UnknownReference {
            subject: subject.clone(),
            reference: reference.clone(),
            expected,
        });

        return false;
    };

    if info.kind != expected {
        errors.push(ValidationError::InvalidReferenceKind {
            subject: subject.clone(),
            reference: reference.clone(),
            expected,
            actual: info.kind,
        });

        return false;
    }

    true
}

/// A value reference may only name a source that the invocations
/// evaluating it can observe.
///
/// An input, a transaction output, or a result binding belongs to
/// exactly one operation. An effect belongs either to an operation or
/// to a transition, and a transition-owned effect is observable by any
/// operation that applies that transition.
fn expect_in_scope(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    context: ValueContext<'_>,
    reference: &Id,
    errors: &mut Vec<ValidationError>,
) -> bool {
    let Some(info) = index.get(reference) else {
        return false;
    };

    let Some(owner) = info.owner else {
        return true;
    };

    let owner_info = index.get(owner).expect("declaration owners exist");

    let admitted = match owner_info.kind {
        ReferenceKind::Transition => index.scope_admits_transition(context.scope, owner),

        _ => index.scope_admits_operation(context.scope, owner),
    };

    if !admitted {
        errors.push(ValidationError::ValueSourceOutOfScope {
            subject: subject.clone(),
            source: reference.clone(),
            owner: owner.clone(),
        });
    }

    admitted
}

fn expect_owned_by(
    index: &ReferenceIndex<'_>,
    subject: &Id,
    reference: &Id,
    expected_owner: &Id,
    errors: &mut Vec<ValidationError>,
) -> bool {
    let Some(info) = index.get(reference) else {
        return false;
    };

    if info.owner != Some(expected_owner) {
        errors.push(ValidationError::InvalidReferenceOwner {
            subject: subject.clone(),
            reference: reference.clone(),
            expected_owner: expected_owner.clone(),
            actual_owner: info.owner.cloned(),
        });

        return false;
    }

    true
}

fn input_kind(input: &Input) -> InputKind {
    match input {
        Input::Request(_) => InputKind::Request,
        Input::Subscription(_) => InputKind::Subscription,
    }
}

fn find_input<'a>(model: &'a Model, index: &ReferenceIndex<'a>, input: &Id) -> Option<&'a Input> {
    let info = index.get(input)?;
    let owner = info.owner?;

    model.operations.get(owner)?.inputs.get(input)
}

fn schema_path_resolves(model: &Model, schema: &Id, path: &FieldPath) -> bool {
    if path.0.is_empty() {
        return false;
    }

    resolve_schema_components(model, schema, &path.0)
}

fn resolve_schema_components(model: &Model, schema_id: &Id, components: &[String]) -> bool {
    if components.is_empty() {
        return true;
    }

    let schema = model
        .schemas
        .get(schema_id)
        .expect("references already validated");

    match schema {
        Schema::Canonical(schema) => {
            let Some(field) = schema.fields.get(&components[0]) else {
                return false;
            };

            if components.len() == 1 {
                return true;
            }

            resolve_type_components(model, &field.ty, &components[1..])
        }

        Schema::Fragment(fragment) => {
            let Some(mapped) = fragment.mapping.get(&components[0]) else {
                return false;
            };

            let mut source_path = mapped.0.clone();

            source_path.extend_from_slice(&components[1..]);

            resolve_schema_components(model, &fragment.source, &source_path)
        }
    }
}

fn resolve_type_components(model: &Model, ty: &TypeRef, components: &[String]) -> bool {
    if components.is_empty() {
        return true;
    }

    match ty {
        TypeRef::Schema(schema) => resolve_schema_components(model, schema, components),

        // V1 does not define traversal through collections.
        TypeRef::List(_) | TypeRef::Scalar(_) => false,
    }
}

fn object_schema<'a>(model: &'a Model, index: &ReferenceIndex<'_>, object: &Id) -> &'a Id {
    let info = index.get(object).expect("references already validated");

    let data_model_id = info.owner.expect("data objects have data-model owners");

    let data_model = model
        .data_models
        .get(data_model_id)
        .expect("data-model owner exists");

    &data_model
        .objects
        .get(object)
        .expect("data object exists")
        .schema
}

fn effect_schema<'a>(
    model: &'a Model,
    index: &ReferenceIndex<'_>,
    effect_id: &Id,
) -> Option<&'a Id> {
    let info = index.get(effect_id).expect("references already validated");

    let owner = info.owner.expect("effects have owners");

    let owner_info = index.get(owner).expect("effect owner exists");

    match owner_info.kind {
        ReferenceKind::Operation => {
            let effect = model.operations.get(owner)?.effects.get(effect_id)?;

            match effect {
                Effect::Publication(effect) => Some(&effect.schema),

                Effect::Request(effect) => Some(&effect.schema),

                Effect::External(_) => None,
            }
        }

        ReferenceKind::Transition => {
            for machine in model.state_machines.values() {
                let Some(transition) = machine.transitions.get(owner) else {
                    continue;
                };

                let effect = transition.side_effects.get(effect_id)?;

                return match effect {
                    TransitionSideEffect::Publication(effect) => Some(&effect.schema),

                    TransitionSideEffect::Request(effect) => Some(&effect.schema),
                };
            }

            None
        }

        _ => None,
    }
}

// Result contracts

/// The `Result<Ok, Err>` contract an effect's execution yields: a
/// request inherits its target input's, an external effect declares its
/// own, a publication has none.
fn effect_result_type<'a>(
    model: &'a Model,
    index: &ReferenceIndex<'_>,
    effect_id: &Id,
) -> Option<&'a ResultType> {
    let info = index.get(effect_id)?;

    let owner = info.owner?;

    let owner_info = index.get(owner)?;

    let request_result = |request: &RequestEffect| -> Option<&'a ResultType> {
        match model
            .operations
            .get(&request.target.operation)?
            .inputs
            .get(&request.target.input)?
        {
            Input::Request(input) => Some(&input.result),
            Input::Subscription(_) => None,
        }
    };

    match owner_info.kind {
        ReferenceKind::Operation => match model.operations.get(owner)?.effects.get(effect_id)? {
            Effect::Publication(_) => None,
            Effect::Request(request) => request_result(request),
            Effect::External(external) => external.result.as_ref(),
        },

        ReferenceKind::Transition => {
            for machine in model.state_machines.values() {
                let Some(transition) = machine.transitions.get(owner) else {
                    continue;
                };

                return match transition.side_effects.get(effect_id)? {
                    TransitionSideEffect::Publication(_) => None,
                    TransitionSideEffect::Request(request) => request_result(request),
                };
            }

            None
        }

        _ => None,
    }
}

/// A result-bearing effect may be executed without binding its result;
/// an effect without a synchronous result must not declare a binding.
fn validate_result_bindings(model: &Model, index: &ReferenceIndex<'_>) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (operation_id, operation) in &model.operations {
        for (location, step) in operation.program.steps_with_locations() {
            let Some(result) = step_result_binding(step) else {
                continue;
            };

            let Some(effect) = index.binding_effect(result) else {
                continue;
            };

            if effect_result_type(model, index, effect).is_none() {
                errors.push(ValidationError::EffectHasNoResult {
                    operation: operation_id.clone(),
                    location,
                    effect: effect.clone(),
                    result: result.clone(),
                });
            }
        }
    }

    errors
}

// Program structure and definite availability

/// What is definitely available at one program point: the transaction
/// artifacts every path reaching it has established or recovered, the
/// result bindings every path has bound, and the result variants the
/// enclosing match arms have selected.
#[derive(Debug, Clone, Default)]
struct Availability {
    artifacts: BTreeSet<Id>,
    bound: BTreeSet<Id>,
    ok: BTreeSet<Id>,
    err: BTreeSet<Id>,
}

impl Availability {
    /// The state at a join: what both predecessors guarantee.
    fn meet(&self, other: &Self) -> Self {
        let both = |first: &BTreeSet<Id>, second: &BTreeSet<Id>| {
            first.intersection(second).cloned().collect()
        };

        Self {
            artifacts: both(&self.artifacts, &other.artifacts),
            bound: both(&self.bound, &other.bound),
            ok: both(&self.ok, &other.ok),
            err: both(&self.err, &other.err),
        }
    }
}

/// The state a block leaves behind: what falls through its end, or
/// nothing when every path through it terminates — a terminated
/// predecessor imposes no constraint on a join.
type Fallthrough = Option<Availability>;

fn join(first: Fallthrough, second: Fallthrough) -> Fallthrough {
    match (first, second) {
        (Some(first), Some(second)) => Some(first.meet(&second)),
        (Some(state), None) | (None, Some(state)) => Some(state),
        (None, None) => None,
    }
}

/// The forward definite-availability analysis over one operation
/// program.
///
/// Artifacts: a transaction adds what it establishes or recovers; a
/// join keeps what every predecessor guarantees. Result bindings: an
/// effect-executing step adds the binding it declares. Variants: a
/// match arm selects its variant for its own extent only, so neither
/// payload survives the join after the match.
struct ProgramValidator<'a> {
    model: &'a Model,
    operation_id: &'a Id,
    operation: &'a Operation,
    errors: Vec<ValidationError>,

    /// One diagnostic per (step, declaration), however many references
    /// the step makes to it.
    reported: BTreeSet<(StepLocation, Id)>,
}

impl<'a> ProgramValidator<'a> {
    fn block(
        &mut self,
        block: &OperationBlock,
        parent: &StepLocation,
        arm: Option<Arm>,
        mut state: Availability,
    ) -> Fallthrough {
        for (index, step) in block.steps.iter().enumerate() {
            let location = OperationBlock::location(parent, arm, index);

            match self.step(step, &location, state) {
                Some(next) => state = next,

                None => {
                    // Whatever follows a terminal in its block is dead;
                    // the first such step is reported and the rest are
                    // not analyzed against a state nothing reaches.
                    if index + 1 < block.steps.len() {
                        self.errors.push(ValidationError::UnreachableProgramStep {
                            operation: self.operation_id.clone(),
                            location: OperationBlock::location(parent, arm, index + 1),
                        });
                    }

                    return None;
                }
            }
        }

        Some(state)
    }

    fn step(
        &mut self,
        step: &OperationStep,
        location: &StepLocation,
        mut state: Availability,
    ) -> Fallthrough {
        match step {
            OperationStep::Transaction(step) => {
                let consumer = ProgramUse::Transaction {
                    transaction: step.transaction.clone(),
                };

                if let Some(body) = self.operation.transactions.get(&step.transaction) {
                    // The commit key is evaluated for the invocation
                    // before the body executes.
                    if let IdempotencyGuarantee::DeduplicatedBy { key } = &body.idempotency {
                        for root in &key.components {
                            self.require(&state, root, location, &consumer);
                        }
                    }

                    // References within the body to an output the body
                    // established at an earlier step are satisfied by
                    // that step, whatever the program guarantees at
                    // entry.
                    let mut established_here: BTreeSet<&Id> = BTreeSet::new();

                    for inner in &body.steps {
                        for root in inner.roots() {
                            if let ValueSource::TransactionOutput(output) = &root.source
                                && established_here.contains(output)
                            {
                                continue;
                            }

                            self.require(&state, root, location, &consumer);
                        }

                        if let TransactionStep::EstablishTransactionOutput(establish) = inner {
                            established_here.insert(&establish.output);
                        }
                    }

                    for artifact in established_by(self.model, self.operation, body) {
                        state.artifacts.insert(artifact.clone());
                    }
                }

                Some(state)
            }

            OperationStep::ExecuteEffect(step) => {
                let consumer = ProgramUse::Effect {
                    effect: step.effect.clone(),
                };

                for root in step.values.roots() {
                    self.require(&state, root, location, &consumer);
                }

                for root in declaration_roots(self.model, self.operation, &step.effect) {
                    self.require(&state, root, location, &consumer);
                }

                if let Some(result) = &step.result {
                    state.bound.insert(result.clone());
                }

                Some(state)
            }

            OperationStep::ExecuteEffectIntent(step) => {
                if !state.artifacts.contains(&step.intent)
                    && self
                        .reported
                        .insert((location.clone(), step.intent.clone()))
                {
                    self.errors
                        .push(ValidationError::TransactionArtifactNotAvailable {
                            operation: self.operation_id.clone(),
                            location: location.clone(),
                            artifact: step.intent.clone(),
                            consumer: ProgramUse::EffectIntent {
                                intent: step.intent.clone(),
                            },
                        });
                }

                // The declaration of the underlying effect is evaluated
                // wherever the effect executes, an intent site no less
                // than a direct one.
                if let Some(intent) = self.operation.effect_intents.get(&step.intent) {
                    let consumer = ProgramUse::EffectIntent {
                        intent: step.intent.clone(),
                    };

                    for root in declaration_roots(self.model, self.operation, &intent.effect) {
                        self.require(&state, root, location, &consumer);
                    }
                }

                if let Some(result) = &step.result {
                    state.bound.insert(result.clone());
                }

                Some(state)
            }

            OperationStep::MatchResult(step) => {
                if !state.bound.contains(&step.result) {
                    self.errors.push(ValidationError::EffectResultNotBound {
                        operation: self.operation_id.clone(),
                        location: location.clone(),
                        result: step.result.clone(),
                        consumer: ProgramUse::Match,
                    });
                }

                // A redundant nested match on a binding an enclosing arm
                // already selected must not strip that outer selection
                // at its join.
                let outer_ok = state.ok.contains(&step.result);
                let outer_err = state.err.contains(&step.result);

                let mut ok = state.clone();

                ok.ok.insert(step.result.clone());

                let mut err = state;

                err.err.insert(step.result.clone());

                let ok = self.block(&step.ok, location, Some(Arm::Ok), ok);
                let err = self.block(&step.err, location, Some(Arm::Err), err);

                // Variant payloads are arm-local: neither survives the
                // join, even when the other arm always terminates.
                join(ok, err).map(|mut state| {
                    if !outer_ok {
                        state.ok.remove(&step.result);
                    }

                    if !outer_err {
                        state.err.remove(&step.result);
                    }

                    state
                })
            }

            OperationStep::Branch(step) => {
                for root in step.condition.roots() {
                    self.require(&state, root, location, &ProgramUse::Condition);
                }

                let then = self.block(&step.then, location, Some(Arm::Then), state.clone());

                let otherwise = match &step.otherwise {
                    Some(block) => self.block(block, location, Some(Arm::Otherwise), state),
                    None => Some(state),
                };

                join(then, otherwise)
            }

            OperationStep::Return(step) => {
                let consumer = ProgramUse::Return {
                    request: step.request.clone(),
                };

                for root in step.outcome.values().roots() {
                    self.require(&state, root, location, &consumer);
                }

                None
            }

            OperationStep::Complete => None,
        }
    }

    /// Reports a reference to a declaration the state does not
    /// guarantee at this point.
    fn require(
        &mut self,
        state: &Availability,
        root: &ValueRef,
        location: &StepLocation,
        consumer: &ProgramUse,
    ) {
        let error = match &root.source {
            ValueSource::TransactionOutput(output) if !state.artifacts.contains(output) => {
                ValidationError::TransactionArtifactNotAvailable {
                    operation: self.operation_id.clone(),
                    location: location.clone(),
                    artifact: output.clone(),
                    consumer: consumer.clone(),
                }
            }

            ValueSource::EffectResultOk(result) | ValueSource::EffectResultErr(result) => {
                let (variant, selected) = match &root.source {
                    ValueSource::EffectResultOk(_) => (ResultVariant::Ok, &state.ok),
                    _ => (ResultVariant::Err, &state.err),
                };

                if selected.contains(result) {
                    return;
                }

                if state.bound.contains(result) {
                    ValidationError::EffectResultVariantOutOfScope {
                        operation: self.operation_id.clone(),
                        location: location.clone(),
                        result: result.clone(),
                        variant,
                        consumer: consumer.clone(),
                    }
                } else {
                    ValidationError::EffectResultNotBound {
                        operation: self.operation_id.clone(),
                        location: location.clone(),
                        result: result.clone(),
                        consumer: consumer.clone(),
                    }
                }
            }

            _ => return,
        };

        if self
            .reported
            .insert((location.clone(), root.source.id().clone()))
        {
            self.errors.push(error);
        }
    }
}

/// Every value reference an effect's declaration evaluates when the
/// effect executes — an external deduplication key, the source and
/// target of each propagation — for an operation-owned effect or a
/// transition side effect.
fn declaration_roots<'a>(
    model: &'a Model,
    operation: &'a Operation,
    effect_id: &Id,
) -> Vec<&'a ValueRef> {
    if let Some(effect) = operation.effects.get(effect_id) {
        return effect.roots();
    }

    for machine in model.state_machines.values() {
        for transition in machine.transitions.values() {
            if let Some(side_effect) = transition.side_effects.get(effect_id) {
                let propagations = match side_effect {
                    TransitionSideEffect::Publication(effect) => {
                        &effect.idempotency_key_propagation
                    }
                    TransitionSideEffect::Request(effect) => &effect.idempotency_key_propagation,
                };

                let mut roots = Vec::new();

                for propagation in propagations {
                    roots.extend(propagation.source.components.iter());
                    roots.extend(propagation.target.components.iter());
                }

                return roots;
            }
        }
    }

    Vec::new()
}

/// The transaction artifacts a successful execution of the transaction
/// establishes, or a re-encounter of its keyed commit recovers: its
/// explicitly established outputs and intents, and the operation's
/// intents for the side effects of every transition it applies.
fn established_by<'a>(
    model: &'a Model,
    operation: &'a Operation,
    body: &'a Transaction,
) -> Vec<&'a Id> {
    let mut artifacts = Vec::new();

    for step in &body.steps {
        match step {
            TransactionStep::EstablishTransactionOutput(establish) => {
                artifacts.push(&establish.output);
            }

            TransactionStep::EstablishEffectIntent(establish) => {
                artifacts.push(&establish.intent);
            }

            TransactionStep::Transition(transition) => {
                let Some(declaration) = model
                    .state_machines
                    .get(&transition.machine)
                    .and_then(|machine| machine.transitions.get(&transition.transition))
                else {
                    continue;
                };

                for (intent_id, intent) in &operation.effect_intents {
                    if declaration.side_effects.contains_key(&intent.effect) {
                        artifacts.push(intent_id);
                    }
                }
            }

            _ => {}
        }
    }

    artifacts
}

/// Structural and dataflow validation of every operation program: each
/// reachable path ends at a terminal, no step follows one, and every
/// artifact, binding, and variant payload is consumed only where
/// control flow definitely provides it.
fn validate_programs(model: &Model, _index: &ReferenceIndex<'_>) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (operation_id, operation) in &model.operations {
        let mut validator = ProgramValidator {
            model,
            operation_id,
            operation,
            errors: Vec::new(),
            reported: BTreeSet::new(),
        };

        let fallthrough = validator.block(
            &operation.program,
            &StepLocation::root(),
            None,
            Availability::default(),
        );

        errors.extend(validator.errors);

        if fallthrough.is_some() {
            errors.push(ValidationError::ProgramNotTerminated {
                operation: operation_id.clone(),
            });
        }
    }

    errors
}

/// Field paths of the references the program makes at operation level:
/// effect instance derivations, branch conditions, and return outcomes.
fn validate_program_paths(
    model: &Model,
    index: &ReferenceIndex<'_>,
    operation_id: &Id,
    program: &OperationBlock,
    errors: &mut Vec<ValidationError>,
) {
    let context = ValueContext::operation(operation_id);

    for (_, step) in program.steps_with_locations() {
        match step {
            OperationStep::ExecuteEffect(step) => {
                validate_derivation_paths(
                    model,
                    index,
                    operation_id,
                    context,
                    &step.values,
                    errors,
                );
            }

            OperationStep::Branch(step) => {
                for root in step.condition.roots() {
                    validate_value_ref_path(model, index, operation_id, context, root, errors);
                }
            }

            OperationStep::Return(step) => {
                validate_derivation_paths(
                    model,
                    index,
                    operation_id,
                    context,
                    step.outcome.values(),
                    errors,
                );
            }

            _ => {}
        }
    }
}
