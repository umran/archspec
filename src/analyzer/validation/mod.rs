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

        Self {
            entries,
            transition_appliers,
        }
    }

    fn get(&self, id: &Id) -> Option<ReferenceInfo<'a>> {
        self.entries.get(id).copied()
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

    errors.extend(validate_state_machines(model));

    errors.extend(validate_transactions(model, &index));

    errors.extend(validate_responses(model));

    errors.extend(validate_operation_requirements(model));

    errors.extend(validate_effect_intents(model, &index));

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
            let deduplicated = matches!(
                transaction.idempotency,
                IdempotencyGuarantee::DeduplicatedBy { .. }
            );

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

                        // A committed transition changes the state it was
                        // evaluated against, so V1 cannot replay the
                        // transaction naturally to reproduce its outcome
                        // or its artifacts. The durable keyed commit is
                        // the recovery boundary.
                        if !deduplicated {
                            errors.push(ValidationError::TransitionTransactionNotDeduplicated {
                                transaction: transaction_id.clone(),
                                machine: transition.machine.clone(),
                                transition: transition.transition.clone(),
                            });
                        }

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

                    TransactionStep::EstablishInvocationResult(_) => {}
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

fn validate_operation_requirements(model: &Model) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for (operation_id, operation) in &model.operations {
        // Terminal execution is defined relative to a declared flow.
        if !operation.requirements.recoverability.is_empty() && operation.flows.is_empty() {
            errors.push(ValidationError::RecoverabilityRequiresFlow {
                operation: operation_id.clone(),
            });
        }
    }

    errors
}

fn validate_responses(model: &Model) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for operation in model.operations.values() {
        for (response_id, response) in &operation.responses {
            let ResponseSource::InvocationResult { result: result_id } = &response.source else {
                continue;
            };

            let result = operation
                .invocation_results
                .get(result_id)
                .expect("references already validated");

            if response.schema != result.schema {
                errors.push(ValidationError::ResponseInvocationResultSchemaMismatch {
                    response: response_id.clone(),
                    response_schema: response.schema.clone(),
                    result: result_id.clone(),
                    result_schema: result.schema.clone(),
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

        for (flow_id, flow) in &operation.flows {
            for step in &flow.steps {
                let FlowStep::ExecuteEffect { values, .. } = step else {
                    continue;
                };

                validate_derivation_paths(
                    model,
                    index,
                    flow_id,
                    ValueContext::operation(operation_id),
                    values,
                    &mut errors,
                );
            }
        }
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

        ValueSource::InvocationResult(result_id) => {
            let info = index.get(result_id).expect("references already validated");

            let operation_id = info.owner.expect("invocation result has an owner");

            let result = model
                .operations
                .get(operation_id)
                .expect("owner operation exists")
                .invocation_results
                .get(result_id)
                .expect("invocation result exists");

            validate_schema_path(model, subject, &result.schema, &value.path, errors);
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

            TransactionStep::EstablishInvocationResult(step) => {
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

        for (result_id, result) in &operation.invocation_results {
            expect_reference(
                index,
                result_id,
                &result.schema,
                ReferenceKind::Schema,
                errors,
            );
        }

        for (response_id, response) in &operation.responses {
            if expect_owned_reference(
                index,
                response_id,
                &response.request,
                ReferenceKind::Input,
                operation_id,
                errors,
            ) {
                validate_request_input_kind(model, index, response_id, &response.request, errors);
            }

            expect_reference(
                index,
                response_id,
                &response.schema,
                ReferenceKind::Schema,
                errors,
            );

            match &response.source {
                ResponseSource::Unspecified => {}

                ResponseSource::InvocationResult { result } => {
                    expect_owned_reference(
                        index,
                        response_id,
                        result,
                        ReferenceKind::InvocationResult,
                        operation_id,
                        errors,
                    );
                }
            }
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

        for (flow_id, flow) in &operation.flows {
            validate_flow_references(index, operation_id, flow_id, flow, errors);
        }

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
        }
    }
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

            TransactionStep::EstablishInvocationResult(step) => {
                expect_owned_reference(
                    index,
                    transaction_id,
                    &step.result,
                    ReferenceKind::InvocationResult,
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

fn validate_flow_references(
    index: &ReferenceIndex<'_>,
    operation_id: &Id,
    flow_id: &Id,
    flow: &InvocationFlow,
    errors: &mut Vec<ValidationError>,
) {
    for step in &flow.steps {
        match step {
            FlowStep::Transaction { transaction } => {
                expect_owned_reference(
                    index,
                    flow_id,
                    transaction,
                    ReferenceKind::Transaction,
                    operation_id,
                    errors,
                );
            }

            FlowStep::ExecuteEffect { effect, values } => {
                // A transition side effect is established as an intent
                // and executed through ExecuteEffectIntent, so a direct
                // execution must name an operation-owned effect.
                expect_owned_reference(
                    index,
                    flow_id,
                    effect,
                    ReferenceKind::Effect,
                    operation_id,
                    errors,
                );

                // The effect instance is constructed at flow level, so
                // its derivation is evaluated in the operation value
                // context with no transaction in scope.
                validate_derivation_references(
                    index,
                    flow_id,
                    ValueContext::operation(operation_id),
                    values,
                    errors,
                );
            }

            FlowStep::ExecuteEffectIntent { intent } => {
                expect_owned_reference(
                    index,
                    flow_id,
                    intent,
                    ReferenceKind::EffectIntent,
                    operation_id,
                    errors,
                );
            }
        }
    }

    if let Some(response) = &flow.response {
        expect_owned_reference(
            index,
            flow_id,
            response,
            ReferenceKind::Response,
            operation_id,
            errors,
        );
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

        ValueSource::InvocationResult(result) => {
            if expect_reference(
                index,
                subject,
                result,
                ReferenceKind::InvocationResult,
                errors,
            ) {
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

        for result_id in operation.invocation_results.keys() {
            visit(
                result_id,
                ReferenceKind::InvocationResult,
                Some(operation_id),
            );
        }

        for response_id in operation.responses.keys() {
            visit(response_id, ReferenceKind::Response, Some(operation_id));
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

        for flow_id in operation.flows.keys() {
            visit(flow_id, ReferenceKind::InvocationFlow, Some(operation_id));
        }
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
/// An input or invocation result belongs to exactly one operation. An
/// effect belongs either to an operation or to a transition, and a
/// transition-owned effect is observable by any operation that applies
/// that transition.
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
