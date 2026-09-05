use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use archspec::{
    analyzer::validation::{self, ProgramUse, ReferenceKind, ValidationError},
    parser::yaml,
    spec::{
        Arm, Branch, Condition, Derivation, EffectIntent, EstablishEffectIntent,
        EstablishTransactionOutput, ExecuteEffect, FieldPath, Id, IdempotencyGuarantee, Input,
        Literal, MessageIdentity, MessageSelector, Model, OperationBlock, OperationStep,
        RequestIdentity, ResultOutcome, ResultVariant, Return, RunTransaction, Schema,
        SchemaFragment, SelectorValue, StateTransition, StepHop, StepLocation, TopicOrdering,
        Transaction, TransactionIsolation, TransactionOutput, TransactionStep, ValueRef,
        ValueSource,
    },
};

fn id(value: &str) -> Id {
    Id(value.to_owned())
}

fn path(components: &[&str]) -> FieldPath {
    FieldPath(components.iter().map(|part| (*part).to_owned()).collect())
}

fn input_ref(input: &str, components: &[&str]) -> ValueRef {
    ValueRef {
        source: ValueSource::Input(id(input)),
        path: path(components),
    }
}

/// A step location from `(index, arm entered beneath it)` hops.
fn at(hops: &[(usize, Option<Arm>)]) -> StepLocation {
    StepLocation(
        hops.iter()
            .map(|(step, arm)| StepHop {
                step: *step,
                arm: *arm,
            })
            .collect(),
    )
}

fn program_mut<'a>(model: &'a mut Model, operation: &str) -> &'a mut OperationBlock {
    &mut model.operations.get_mut(&id(operation)).unwrap().program
}

fn run(transaction: &str) -> OperationStep {
    OperationStep::Transaction(RunTransaction {
        transaction: id(transaction),
    })
}

fn return_ok(request: &str, values: Derivation) -> OperationStep {
    OperationStep::Return(Return {
        request: id(request),
        outcome: ResultOutcome::Ok { values },
    })
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn load_flash_checkout() -> Model {
    let path = fixture_path("flash_checkout.yaml");

    let source = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read fixture `{}`: {error}", path.display()));

    yaml::parse(&source).expect("flash checkout fixture should parse")
}

#[test]
fn flash_checkout_is_valid() {
    let model = load_flash_checkout();

    let errors = validation::validate(&model);

    assert!(
        errors.is_empty(),
        "flash checkout should be valid:\n{errors:#?}"
    );
}

#[test]
fn rejects_duplicate_global_id() {
    let mut model = load_flash_checkout();

    let schema = model
        .schemas
        .get(&id("schema.CreateOrderRequest"))
        .unwrap()
        .clone();

    // Collides with an existing service ID.
    model.schemas.insert(id("service.checkout"), schema);

    let errors = validation::validate(&model);

    assert_eq!(errors.len(), 1);

    assert!(matches!(
        &errors[0],
        ValidationError::DuplicateId { id: duplicate, .. }
            if duplicate == &id("service.checkout")
    ));
}

#[test]
fn rejects_unknown_service_reference() {
    let mut model = load_flash_checkout();

    model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .service = id("service.missing");

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::UnknownReference {
            subject: id("operation.create_order"),
            reference: id("service.missing"),
            expected: ReferenceKind::Service,
        }]
    );
}

#[test]
fn rejects_reference_with_wrong_kind() {
    let mut model = load_flash_checkout();

    model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .service = id("schema.CreateOrderRequest");

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidReferenceKind {
            subject: id("operation.create_order"),
            reference: id("schema.CreateOrderRequest"),
            expected: ReferenceKind::Service,
            actual: ReferenceKind::Schema,
        }]
    );
}

#[test]
fn rejects_program_using_transaction_owned_by_another_operation() {
    let mut model = load_flash_checkout();

    program_mut(&mut model, "operation.reserve_inventory").steps[0] = run("tx.create_order.new");

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidReferenceOwner {
            subject: id("operation.reserve_inventory"),
            reference: id("tx.create_order.new"),
            expected_owner: id("operation.reserve_inventory"),
            actual_owner: Some(id("operation.create_order")),
        }]
    );
}

#[test]
fn rejects_schema_fragment_cycle() {
    let mut model = load_flash_checkout();

    model.schemas.insert(
        id("schema.FragmentA"),
        Schema::Fragment(SchemaFragment {
            source: id("schema.FragmentB"),
            mapping: BTreeMap::new(),
        }),
    );

    model.schemas.insert(
        id("schema.FragmentB"),
        Schema::Fragment(SchemaFragment {
            source: id("schema.FragmentA"),
            mapping: BTreeMap::new(),
        }),
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::FragmentCycle {
            cycle: vec![
                id("schema.FragmentA"),
                id("schema.FragmentB"),
                id("schema.FragmentA"),
            ],
        }]
    );
}

#[test]
fn rejects_subscription_message_not_carried_by_topic() {
    let mut model = load_flash_checkout();

    // Clone OrderCreated so all of the operation's existing field-path
    // requirements still resolve. The only defect is topic membership.
    let shadow_id = id("schema.OrderCreatedShadow");

    let shadow = model
        .schemas
        .get(&id("schema.OrderCreated"))
        .unwrap()
        .clone();

    model.schemas.insert(shadow_id.clone(), shadow);

    let operation = model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap();

    let input = operation
        .inputs
        .get_mut(&id("input.reserve_inventory.created"))
        .unwrap();

    let Input::Subscription(subscription) = input else {
        panic!("expected subscription input");
    };

    let MessageSelector::Only(schemas) = &mut subscription.messages else {
        panic!("expected selective message subscription");
    };

    schemas.insert(shadow_id.clone());

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::SubscriptionMessageNotOnTopic {
            input: id("input.reserve_inventory.created"),
            topic: id("topic.order_events"),
            schema: shadow_id,
        }]
    );
}

#[test]
fn rejects_publication_schema_not_carried_by_topic() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.cancel_order"))
        .unwrap();

    let effect = operation
        .effects
        .get_mut(&id("effect.cancel_order.publish_cancelled"))
        .unwrap();

    let archspec::spec::Effect::Publication(publication) = effect else {
        panic!("expected publication effect");
    };

    publication.schema = id("schema.CancelOrderRequest");

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::PublicationEffectMessageNotOnTopic {
            effect: id("effect.cancel_order.publish_cancelled"),
            topic: id("topic.order_events"),
            schema: id("schema.CancelOrderRequest"),
        }]
    );
}

#[test]
fn rejects_keyed_topic_missing_schema_mapping() {
    let mut model = load_flash_checkout();

    let topic = model.topics.get_mut(&id("topic.order_events")).unwrap();

    let TopicOrdering::Keyed(key) = &mut topic.ordering else {
        panic!("expected keyed topic");
    };

    key.mapping.remove(&id("schema.PaymentCaptured"));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TopicKeyMissingSchema {
            topic: id("topic.order_events"),
            schema: id("schema.PaymentCaptured"),
        }]
    );
}

#[test]
fn rejects_topic_key_for_schema_not_on_topic() {
    let mut model = load_flash_checkout();

    let topic = model.topics.get_mut(&id("topic.order_events")).unwrap();

    let TopicOrdering::Keyed(key) = &mut topic.ordering else {
        panic!("expected keyed topic");
    };

    key.mapping.insert(
        id("schema.CancelOrderRequest"),
        FieldPath(vec!["order_id".to_owned()]),
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TopicKeySchemaNotOnTopic {
            topic: id("topic.order_events"),
            schema: id("schema.CancelOrderRequest"),
        }]
    );
}

#[test]
fn rejects_transaction_access_without_data_model() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.cancel_order"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.cancel_order"))
        .unwrap();

    transaction.data_model = None;

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionMissingDataModel {
            transaction: id("tx.cancel_order"),
            object: id("object.order"),
        }]
    );
}

#[test]
fn rejects_transaction_access_outside_declared_data_model() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.cancel_order"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.cancel_order"))
        .unwrap();

    transaction.data_model = Some(id("data.inventory"));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionObjectOutsideDataModel {
            transaction: id("tx.cancel_order"),
            data_model: id("data.inventory"),
            object: id("object.order"),
        }]
    );
}

#[test]
fn rejects_state_transition_with_wrong_subject_object() {
    let mut model = load_flash_checkout();

    // Create another valid object in the SAME data model with the
    // SAME schema so that ownership and field-path validation still
    // succeed. The only defect is state-machine subject identity.
    let shadow_id = id("object.order_shadow");

    let shadow = model
        .data_models
        .get(&id("data.checkout"))
        .unwrap()
        .objects
        .get(&id("object.order"))
        .unwrap()
        .clone();

    model
        .data_models
        .get_mut(&id("data.checkout"))
        .unwrap()
        .objects
        .insert(shadow_id.clone(), shadow);

    let transaction = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.apply_payment"))
        .unwrap();

    let TransactionStep::Transition(transition) = &mut transaction.steps[1] else {
        panic!("expected transition step");
    };

    transition.subject.object = shadow_id.clone();

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::StateTransitionSubjectMismatch {
            transaction: id("tx.apply_payment"),
            machine: id("machine.order_lifecycle"),
            expected_object: id("object.order"),
            actual_object: shadow_id,
        }]
    );
}

#[test]
fn rejects_invalid_value_ref_field_path() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap();

    operation.requirements.ordering[0].key.path = FieldPath(vec!["does_not_exist".to_owned()]);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidFieldPath {
            subject: id("operation.apply_payment"),
            schema: id("schema.PaymentCaptured"),
            path: FieldPath(vec!["does_not_exist".to_owned()]),
        }]
    );
}

#[test]
fn rejects_field_reference_into_untyped_external_effect() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.charge_payment"))
        .unwrap();

    operation.requirements.serialization[0].key.source =
        ValueSource::Effect(id("effect.charge_payment.card"));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ValueSourceHasNoSchema {
            subject: id("operation.charge_payment"),
            source: id("effect.charge_payment.card"),
        }]
    );
}

#[test]
fn transition_transactions_accept_any_idempotency_guarantee() {
    // A transition blocks the natural-replay proof route,
    // but the missing proof fact is the solver's concern, never a
    // structural error. The keyed form is the fixture as declared.
    for idempotency in [
        IdempotencyGuarantee::Unspecified,
        IdempotencyGuarantee::NotDeduplicated,
    ] {
        let mut model = load_flash_checkout();

        model
            .operations
            .get_mut(&id("operation.apply_payment"))
            .unwrap()
            .transactions
            .get_mut(&id("tx.apply_payment"))
            .unwrap()
            .idempotency = idempotency;

        let errors = validation::validate(&model);

        assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
    }
}

#[test]
fn transition_effect_values_coverage_is_independent_of_the_guarantee() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.apply_payment"))
        .unwrap();

    transaction.idempotency = IdempotencyGuarantee::Unspecified;

    let TransactionStep::Transition(transition) = &mut transaction.steps[1] else {
        panic!("expected the mark_paid transition step");
    };

    transition.effect_values.clear();

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransitionEffectValuesMismatch {
            transaction: id("tx.apply_payment"),
            transition: id("transition.order.mark_paid"),
            missing: vec![id("effect.order.paid")],
            unexpected: vec![],
        }]
    );
}

#[test]
fn rejects_explicit_establishment_of_transition_effect_intent() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.apply_payment"))
        .unwrap();

    transaction
        .steps
        .push(TransactionStep::EstablishEffectIntent(
            EstablishEffectIntent {
                intent: id("intent.apply_payment.order_paid"),
                values: Derivation::Unspecified,
            },
        ));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![
            ValidationError::TransitionEffectIntentExplicitlyEstablished {
                transaction: id("tx.apply_payment"),
                intent: id("intent.apply_payment.order_paid"),
                effect: id("effect.order.paid"),
            }
        ]
    );
}

#[test]
fn rejects_direct_execution_of_transition_side_effect() {
    let mut model = load_flash_checkout();

    program_mut(&mut model, "operation.apply_payment").steps[1] =
        OperationStep::ExecuteEffect(ExecuteEffect {
            effect: id("effect.order.paid"),
            values: Derivation::Unspecified,
            result: None,
        });

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidReferenceOwner {
            subject: id("operation.apply_payment"),
            reference: id("effect.order.paid"),
            expected_owner: id("operation.apply_payment"),
            actual_owner: Some(id("transition.order.mark_paid")),
        }]
    );
}

#[test]
fn rejects_transaction_read_used_outside_a_transaction() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap();

    operation.requirements.serialization[0].key.source =
        ValueSource::TransactionRead(id("read.reserve_inventory.stock"));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionReadOutsideTransaction {
            subject: id("operation.reserve_inventory"),
            read: id("read.reserve_inventory.stock"),
        }]
    );
}

#[test]
fn rejects_transaction_read_from_another_transaction() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.transfer_stock"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.transfer_stock"))
        .unwrap();

    let TransactionStep::Write(write) = &mut transaction.steps[3] else {
        panic!("expected the source-warehouse write");
    };

    let Derivation::Deterministic { from } = &mut write.values else {
        panic!("expected a deterministic derivation");
    };

    from[0].source = ValueSource::TransactionRead(id("read.reserve_inventory.stock"));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidReferenceOwner {
            subject: id("tx.transfer_stock"),
            reference: id("read.reserve_inventory.stock"),
            expected_owner: id("tx.transfer_stock"),
            actual_owner: Some(id("tx.reserve_inventory")),
        }]
    );
}

#[test]
fn rejects_transaction_read_referenced_before_the_read() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.reserve_inventory"))
        .unwrap();

    // Move the mutation ahead of the read it derives its values from.
    transaction.steps.swap(0, 1);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionReadOutOfOrder {
            transaction: id("tx.reserve_inventory"),
            read: id("read.reserve_inventory.stock"),
        }]
    );
}

#[test]
fn rejects_reference_to_field_the_read_did_not_select() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.reserve_inventory"))
        .unwrap();

    let TransactionStep::Write(write) = &mut transaction.steps[1] else {
        panic!("expected the stock write");
    };

    let Derivation::Deterministic { from } = &mut write.values else {
        panic!("expected a deterministic derivation");
    };

    // `warehouse_id` resolves against the object schema, but the read
    // selects only `on_hand` and `reserved`.
    from[0].path = FieldPath(vec!["warehouse_id".to_owned()]);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionReadFieldNotSelected {
            transaction: id("tx.reserve_inventory"),
            read: id("read.reserve_inventory.stock"),
            path: FieldPath(vec!["warehouse_id".to_owned()]),
        }]
    );
}

#[test]
fn rejects_transaction_read_result_colliding_with_another_id() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.reserve_inventory"))
        .unwrap();

    let TransactionStep::Read(read) = &mut transaction.steps[0] else {
        panic!("expected the stock read");
    };

    read.result = id("object.stock");

    let errors = validation::validate(&model);

    assert_eq!(errors.len(), 1);

    assert!(matches!(
        &errors[0],
        ValidationError::DuplicateId { id: duplicate, .. }
            if duplicate == &id("object.stock")
    ));
}

#[test]
fn rejects_data_object_without_identity() {
    let mut model = load_flash_checkout();

    model
        .data_models
        .get_mut(&id("data.checkout"))
        .unwrap()
        .objects
        .get_mut(&id("object.order"))
        .unwrap()
        .identity
        .clear();

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::EmptyObjectIdentity {
            object: id("object.order"),
        }]
    );
}

#[test]
fn rejects_a_program_that_falls_through_without_a_terminal() {
    let mut model = load_flash_checkout();

    program_mut(&mut model, "operation.charge_payment")
        .steps
        .clear();

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ProgramNotTerminated {
            operation: id("operation.charge_payment"),
        }]
    );

    // A path that falls through one arm of a decision and then off the
    // end of the program is the same defect.
    let mut model = load_flash_checkout();

    let OperationStep::MatchResult(matched) =
        &mut program_mut(&mut model, "operation.charge_payment").steps[1]
    else {
        panic!("expected the card match");
    };

    matched.err.steps.pop();

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ProgramNotTerminated {
            operation: id("operation.charge_payment"),
        }]
    );
}

#[test]
fn rejects_invalid_recoverability_key_field_path() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap();

    operation.requirements.recoverability[0].key.components[0].path =
        FieldPath(vec!["does_not_exist".to_owned()]);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidFieldPath {
            subject: id("operation.apply_payment"),
            schema: id("schema.PaymentCaptured"),
            path: FieldPath(vec!["does_not_exist".to_owned()]),
        }]
    );
}

#[test]
fn rejects_unknown_reference_in_recoverability_key() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap();

    operation.requirements.recoverability[0].key.components[0].source =
        ValueSource::Input(id("input.missing"));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::UnknownReference {
            subject: id("operation.apply_payment"),
            reference: id("input.missing"),
            expected: ReferenceKind::Input,
        }]
    );
}

#[test]
fn rejects_transaction_read_in_recoverability_key() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap();

    operation.requirements.recoverability[0].key.components[0].source =
        ValueSource::TransactionRead(id("read.reserve_inventory.stock"));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionReadOutsideTransaction {
            subject: id("operation.reserve_inventory"),
            read: id("read.reserve_inventory.stock"),
        }]
    );
}

/// Retarget the first component of `tx.reserve_inventory`'s write
/// derivation, which is the fixture's provenance site.
fn set_reserve_write_source(model: &mut Model, source: ValueSource, path: &[&str]) {
    let transaction = model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.reserve_inventory"))
        .unwrap();

    let TransactionStep::Write(write) = &mut transaction.steps[1] else {
        panic!("expected the stock write");
    };

    let Derivation::Deterministic { from } = &mut write.values else {
        panic!("expected a deterministic derivation");
    };

    from[0] = ValueRef {
        source,
        path: FieldPath(path.iter().map(|part| (*part).to_owned()).collect()),
    };
}

#[test]
fn rejects_value_ref_to_another_operations_input() {
    let mut model = load_flash_checkout();

    set_reserve_write_source(
        &mut model,
        ValueSource::Input(id("input.create_order.request")),
        &["quantity"],
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ValueSourceOutOfScope {
            subject: id("tx.reserve_inventory"),
            source: id("input.create_order.request"),
            owner: id("operation.create_order"),
        }]
    );
}

#[test]
fn rejects_value_ref_to_another_operations_transaction_output() {
    let mut model = load_flash_checkout();

    set_reserve_write_source(
        &mut model,
        ValueSource::TransactionOutput(id("output.create_order")),
        &["order_id"],
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ValueSourceOutOfScope {
            subject: id("tx.reserve_inventory"),
            source: id("output.create_order"),
            owner: id("operation.create_order"),
        }]
    );
}

#[test]
fn rejects_value_ref_to_another_operations_effect_result() {
    let mut model = load_flash_checkout();

    set_reserve_write_source(
        &mut model,
        ValueSource::EffectResultOk(id("result.charge_payment.card")),
        &["authorization_id"],
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ValueSourceOutOfScope {
            subject: id("tx.reserve_inventory"),
            source: id("result.charge_payment.card"),
            owner: id("operation.charge_payment"),
        }]
    );
}

#[test]
fn rejects_value_ref_to_another_operations_effect() {
    let mut model = load_flash_checkout();

    set_reserve_write_source(
        &mut model,
        ValueSource::Effect(id("effect.create_order.publish_created")),
        &["order_id"],
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ValueSourceOutOfScope {
            subject: id("tx.reserve_inventory"),
            source: id("effect.create_order.publish_created"),
            owner: id("operation.create_order"),
        }]
    );
}

#[test]
fn rejects_value_ref_to_transition_effect_the_operation_does_not_apply() {
    let mut model = load_flash_checkout();

    // reserve_inventory applies no transition, so the mark_paid side
    // effect's payload is not observable by its invocations.
    set_reserve_write_source(
        &mut model,
        ValueSource::Effect(id("effect.order.paid")),
        &["order_id"],
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ValueSourceOutOfScope {
            subject: id("tx.reserve_inventory"),
            source: id("effect.order.paid"),
            owner: id("transition.order.mark_paid"),
        }]
    );
}

#[test]
fn accepts_value_ref_to_state_machine_subject_from_any_operation() {
    let mut model = load_flash_checkout();

    // State machines are global; reserve_inventory may address the
    // object one governs even though it applies no transition.
    set_reserve_write_source(
        &mut model,
        ValueSource::StateMachineSubject(id("machine.order_lifecycle")),
        &["status"],
    );

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
}

#[test]
fn rejects_foreign_input_in_a_transaction_commit_key() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.create_order.new"))
        .unwrap();

    let IdempotencyGuarantee::DeduplicatedBy { key } = &mut transaction.idempotency else {
        panic!("expected a keyed transaction");
    };

    key.components[0] = ValueRef {
        source: ValueSource::Input(id("input.apply_payment.captured")),
        path: FieldPath(vec!["event_id".to_owned()]),
    };

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ValueSourceOutOfScope {
            subject: id("tx.create_order.new"),
            source: id("input.apply_payment.captured"),
            owner: id("operation.apply_payment"),
        }]
    );
}

#[test]
fn rejects_ambiguous_intents_for_one_transition_side_effect() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap();

    operation.effect_intents.insert(
        id("intent.apply_payment.order_paid.duplicate"),
        EffectIntent {
            effect: id("effect.order.paid"),
        },
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::AmbiguousTransitionEffectIntent {
            operation: id("operation.apply_payment"),
            effect: id("effect.order.paid"),
            intents: vec![
                id("intent.apply_payment.order_paid"),
                id("intent.apply_payment.order_paid.duplicate"),
            ],
        }]
    );
}

#[test]
fn rejects_transition_effect_intent_the_operation_cannot_establish() {
    let mut model = load_flash_checkout();

    // cancel_order applies transition.order.cancel, never mark_paid,
    // so it can never establish that transition's side effect.
    model
        .operations
        .get_mut(&id("operation.cancel_order"))
        .unwrap()
        .effect_intents
        .insert(
            id("intent.cancel_order.order_paid"),
            EffectIntent {
                effect: id("effect.order.paid"),
            },
        );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::UnestablishableTransitionEffectIntent {
            operation: id("operation.cancel_order"),
            intent: id("intent.cancel_order.order_paid"),
            effect: id("effect.order.paid"),
            transition: id("transition.order.mark_paid"),
        }]
    );
}

/// Retarget the card-charge program step's derivation, which is the
/// fixture's direct-execution provenance site.
fn set_charge_card_values(model: &mut Model, values: Derivation) {
    let OperationStep::ExecuteEffect(step) =
        &mut program_mut(model, "operation.charge_payment").steps[0]
    else {
        panic!("expected the card-charge execute_effect step");
    };

    step.values = values;
}

#[test]
fn accepts_operation_scoped_deterministic_execute_effect_values() {
    let mut model = load_flash_checkout();

    set_charge_card_values(
        &mut model,
        Derivation::Deterministic {
            from: vec![ValueRef {
                source: ValueSource::Input(id("input.charge_payment.reserved")),
                path: FieldPath(vec!["amount".to_owned()]),
            }],
        },
    );

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
}

#[test]
fn accepts_unspecified_execute_effect_values() {
    let mut model = load_flash_checkout();

    set_charge_card_values(&mut model, Derivation::Unspecified);

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
}

#[test]
fn rejects_transaction_read_in_execute_effect_values() {
    let mut model = load_flash_checkout();

    // No transaction context exists at program level, so a transaction
    // read can never be a direct-execution provenance root.
    set_charge_card_values(
        &mut model,
        Derivation::Deterministic {
            from: vec![ValueRef {
                source: ValueSource::TransactionRead(id("read.reserve_inventory.stock")),
                path: FieldPath(vec!["on_hand".to_owned()]),
            }],
        },
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionReadOutsideTransaction {
            subject: id("operation.charge_payment"),
            read: id("read.reserve_inventory.stock"),
        }]
    );
}

#[test]
fn rejects_invalid_field_path_in_execute_effect_values() {
    let mut model = load_flash_checkout();

    set_charge_card_values(
        &mut model,
        Derivation::Deterministic {
            from: vec![ValueRef {
                source: ValueSource::Input(id("input.charge_payment.reserved")),
                path: FieldPath(vec!["does_not_exist".to_owned()]),
            }],
        },
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidFieldPath {
            subject: id("operation.charge_payment"),
            schema: id("schema.InventoryReserved"),
            path: FieldPath(vec!["does_not_exist".to_owned()]),
        }]
    );
}

/// The fixture's `tx.apply_payment` transition step, whose
/// `effect_values` map is the transition provenance site.
fn apply_payment_transition(model: &mut Model) -> &mut StateTransition {
    let transaction = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.apply_payment"))
        .unwrap();

    let TransactionStep::Transition(transition) = &mut transaction.steps[1] else {
        panic!("expected the mark_paid transition step");
    };

    transition
}

#[test]
fn accepts_transition_effect_derivation_from_preceding_read() {
    let mut model = load_flash_checkout();

    let transition = apply_payment_transition(&mut model);

    let Some(Derivation::Deterministic { from }) =
        transition.effect_values.get(&id("effect.order.paid"))
    else {
        panic!("expected a deterministic transition effect derivation");
    };

    assert_eq!(
        from[0].source,
        ValueSource::TransactionRead(id("read.apply_payment.order"))
    );

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
}

#[test]
fn accepts_empty_effect_values_for_transition_without_side_effects() {
    let model = load_flash_checkout();

    let transaction = model
        .operations
        .get(&id("operation.cancel_order"))
        .unwrap()
        .transactions
        .get(&id("tx.cancel_order"))
        .unwrap();

    let TransactionStep::Transition(transition) = &transaction.steps[0] else {
        panic!("expected the cancel transition step");
    };

    assert!(transition.effect_values.is_empty());

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
}

#[test]
fn rejects_missing_transition_effect_derivation() {
    let mut model = load_flash_checkout();

    apply_payment_transition(&mut model).effect_values.clear();

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransitionEffectValuesMismatch {
            transaction: id("tx.apply_payment"),
            transition: id("transition.order.mark_paid"),
            missing: vec![id("effect.order.paid")],
            unexpected: vec![],
        }]
    );
}

#[test]
fn rejects_extra_transition_effect_derivation() {
    let mut model = load_flash_checkout();

    apply_payment_transition(&mut model)
        .effect_values
        .insert(id("effect.order.unrelated"), Derivation::Unspecified);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransitionEffectValuesMismatch {
            transaction: id("tx.apply_payment"),
            transition: id("transition.order.mark_paid"),
            missing: vec![],
            unexpected: vec![id("effect.order.unrelated")],
        }]
    );
}

#[test]
fn rejects_transition_effect_derivation_owned_by_another_transition() {
    let mut model = load_flash_checkout();

    // transition.order.cancel declares no side effects, so mark_paid's
    // side effect has no instance here for a derivation to describe.
    let transaction = model
        .operations
        .get_mut(&id("operation.cancel_order"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.cancel_order"))
        .unwrap();

    let TransactionStep::Transition(transition) = &mut transaction.steps[0] else {
        panic!("expected the cancel transition step");
    };

    transition
        .effect_values
        .insert(id("effect.order.paid"), Derivation::Unspecified);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransitionEffectValuesMismatch {
            transaction: id("tx.cancel_order"),
            transition: id("transition.order.cancel"),
            missing: vec![],
            unexpected: vec![id("effect.order.paid")],
        }]
    );
}

#[test]
fn rejects_transition_effect_derivation_referencing_later_read() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.apply_payment"))
        .unwrap();

    // Move the transition ahead of the read its effect derivation
    // depends on.
    transaction.steps.swap(0, 1);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionReadOutOfOrder {
            transaction: id("tx.apply_payment"),
            read: id("read.apply_payment.order"),
        }]
    );
}

#[test]
fn rejects_transition_effect_derivation_field_not_selected() {
    let mut model = load_flash_checkout();

    let transition = apply_payment_transition(&mut model);

    let Some(Derivation::Deterministic { from }) =
        transition.effect_values.get_mut(&id("effect.order.paid"))
    else {
        panic!("expected a deterministic transition effect derivation");
    };

    // `status` resolves against the order schema, but the read selects
    // only `order_id`.
    from[0].path = FieldPath(vec!["status".to_owned()]);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionReadFieldNotSelected {
            transaction: id("tx.apply_payment"),
            read: id("read.apply_payment.order"),
            path: FieldPath(vec!["status".to_owned()]),
        }]
    );
}

#[test]
fn rejects_invalid_field_path_in_transition_effect_derivation() {
    let mut model = load_flash_checkout();

    let transition = apply_payment_transition(&mut model);

    let Some(Derivation::Deterministic { from }) =
        transition.effect_values.get_mut(&id("effect.order.paid"))
    else {
        panic!("expected a deterministic transition effect derivation");
    };

    from[1].path = FieldPath(vec!["does_not_exist".to_owned()]);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidFieldPath {
            subject: id("tx.apply_payment"),
            schema: id("schema.PaymentCaptured"),
            path: FieldPath(vec!["does_not_exist".to_owned()]),
        }]
    );
}

#[test]
fn rejects_empty_request_identity() {
    let mut model = load_flash_checkout();

    let Some(Input::Request(request)) = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .inputs
        .get_mut(&id("input.create_order.request"))
    else {
        panic!("create_order input should be a request");
    };

    request.identity = RequestIdentity::Keyed { fields: Vec::new() };

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::EmptyRequestIdentity {
            input: id("input.create_order.request"),
        }]
    );
}

#[test]
fn rejects_unresolvable_request_identity_field() {
    let mut model = load_flash_checkout();

    let Some(Input::Request(request)) = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .inputs
        .get_mut(&id("input.create_order.request"))
    else {
        panic!("create_order input should be a request");
    };

    request.identity = RequestIdentity::Keyed {
        fields: vec![FieldPath(vec!["does_not_exist".to_owned()])],
    };

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidFieldPath {
            subject: id("input.create_order.request"),
            schema: id("schema.CreateOrderRequest"),
            path: FieldPath(vec!["does_not_exist".to_owned()]),
        }]
    );
}

fn order_events_message_identity(
    model: &mut Model,
) -> &mut std::collections::BTreeMap<Id, Vec<FieldPath>> {
    let topic = model.topics.get_mut(&id("topic.order_events")).unwrap();

    let MessageIdentity::Keyed { mapping } = &mut topic.message_identity else {
        panic!("order_events should declare a keyed message identity");
    };

    mapping
}

#[test]
fn rejects_message_identity_for_uncarried_schema() {
    let mut model = load_flash_checkout();

    // A declared schema, but not one carried by the topic.
    order_events_message_identity(&mut model).insert(
        id("schema.CreateOrderRequest"),
        vec![FieldPath(vec!["idempotency_key".to_owned()])],
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::MessageIdentitySchemaNotOnTopic {
            topic: id("topic.order_events"),
            schema: id("schema.CreateOrderRequest"),
        }]
    );
}

#[test]
fn rejects_unknown_schema_in_message_identity() {
    let mut model = load_flash_checkout();

    order_events_message_identity(&mut model).insert(
        id("schema.missing"),
        vec![FieldPath(vec!["event_id".to_owned()])],
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::UnknownReference {
            subject: id("topic.order_events"),
            reference: id("schema.missing"),
            expected: ReferenceKind::Schema,
        }]
    );
}

#[test]
fn rejects_empty_message_identity() {
    let mut model = load_flash_checkout();

    order_events_message_identity(&mut model).insert(id("schema.OrderCreated"), Vec::new());

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::EmptyMessageIdentity {
            topic: id("topic.order_events"),
            schema: id("schema.OrderCreated"),
        }]
    );
}

#[test]
fn rejects_message_identity_arity_mismatch() {
    let mut model = load_flash_checkout();

    order_events_message_identity(&mut model).insert(
        id("schema.OrderCreated"),
        vec![
            FieldPath(vec!["event_id".to_owned()]),
            FieldPath(vec!["order_id".to_owned()]),
        ],
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::MessageIdentityArityMismatch {
            topic: id("topic.order_events"),
            schema: id("schema.OrderCreated"),
            expected: 1,
            actual: 2,
        }]
    );
}

#[test]
fn rejects_unresolvable_message_identity_field() {
    let mut model = load_flash_checkout();

    order_events_message_identity(&mut model).insert(
        id("schema.OrderCreated"),
        vec![FieldPath(vec!["does_not_exist".to_owned()])],
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidFieldPath {
            subject: id("topic.order_events"),
            schema: id("schema.OrderCreated"),
            path: FieldPath(vec!["does_not_exist".to_owned()]),
        }]
    );
}

// ---------------------------------------------------------------------------
// Request results, effect results, and the operation program
// ---------------------------------------------------------------------------

#[test]
fn rejects_request_result_schema_that_does_not_exist() {
    let mut model = load_flash_checkout();

    let Some(Input::Request(request)) = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .inputs
        .get_mut(&id("input.create_order.request"))
    else {
        panic!("create_order input should be a request");
    };

    request.result.err.schema = id("schema.missing");

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::UnknownReference {
            subject: id("input.create_order.request"),
            reference: id("schema.missing"),
            expected: ReferenceKind::Schema,
        }]
    );
}

#[test]
fn rejects_external_result_schema_that_does_not_exist() {
    let mut model = load_flash_checkout();

    let Some(archspec::spec::Effect::External(card)) = model
        .operations
        .get_mut(&id("operation.charge_payment"))
        .unwrap()
        .effects
        .get_mut(&id("effect.charge_payment.card"))
    else {
        panic!("card charge should be an external effect");
    };

    card.result.as_mut().unwrap().ok = id("schema.missing");

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::UnknownReference {
            subject: id("effect.charge_payment.card"),
            reference: id("schema.missing"),
            expected: ReferenceKind::Schema,
        }]
    );
}

#[test]
fn rejects_a_result_binding_on_a_publication() {
    let mut model = load_flash_checkout();

    let OperationStep::MatchResult(matched) =
        &mut program_mut(&mut model, "operation.charge_payment").steps[1]
    else {
        panic!("expected the card match");
    };

    let OperationStep::ExecuteEffect(captured) = &mut matched.ok.steps[0] else {
        panic!("expected the capture publication");
    };

    captured.result = Some(id("result.charge_payment.captured"));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::EffectHasNoResult {
            operation: id("operation.charge_payment"),
            location: at(&[(1, Some(Arm::Ok)), (0, None)]),
            effect: id("effect.charge_payment.publish_captured"),
            result: id("result.charge_payment.captured"),
        }]
    );
}

#[test]
fn rejects_a_result_binding_on_an_external_effect_without_a_contract() {
    let mut model = load_flash_checkout();

    let Some(archspec::spec::Effect::External(card)) = model
        .operations
        .get_mut(&id("operation.charge_payment"))
        .unwrap()
        .effects
        .get_mut(&id("effect.charge_payment.card"))
    else {
        panic!("card charge should be an external effect");
    };

    card.result = None;

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::EffectHasNoResult {
            operation: id("operation.charge_payment"),
            location: at(&[(0, None)]),
            effect: id("effect.charge_payment.card"),
            result: id("result.charge_payment.card"),
        }]
    );
}

#[test]
fn accepts_an_ignored_result() {
    let mut model = load_flash_checkout();

    // The provider's result may be ignored: the card charge executes
    // without binding it, and with nothing to match on the program
    // completes directly.
    let program = program_mut(&mut model, "operation.charge_payment");

    let OperationStep::ExecuteEffect(card) = &mut program.steps[0] else {
        panic!("expected the card charge");
    };

    card.result = None;

    program.steps[1] = OperationStep::Complete;

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
}

#[test]
fn rejects_a_match_on_a_result_no_step_declares() {
    let mut model = load_flash_checkout();

    let OperationStep::ExecuteEffect(card) =
        &mut program_mut(&mut model, "operation.charge_payment").steps[0]
    else {
        panic!("expected the card charge");
    };

    card.result = None;

    let errors = validation::validate(&model);

    // The match and the err arm's payload reference both name a binding
    // nothing declares.
    assert_eq!(
        errors,
        vec![
            ValidationError::UnknownReference {
                subject: id("operation.charge_payment"),
                reference: id("result.charge_payment.card"),
                expected: ReferenceKind::EffectResult,
            },
            ValidationError::UnknownReference {
                subject: id("operation.charge_payment"),
                reference: id("result.charge_payment.card"),
                expected: ReferenceKind::EffectResult,
            },
        ]
    );
}

#[test]
fn rejects_a_match_before_the_binding_step() {
    let mut model = load_flash_checkout();

    program_mut(&mut model, "operation.charge_payment")
        .steps
        .swap(0, 1);

    let errors = validation::validate(&model);

    // The match runs before the card charge binds its result, and the
    // charge itself follows a decision whose every arm terminates.
    assert_eq!(
        errors,
        vec![
            ValidationError::EffectResultNotBound {
                operation: id("operation.charge_payment"),
                location: at(&[(0, None)]),
                result: id("result.charge_payment.card"),
                consumer: ProgramUse::Match,
            },
            ValidationError::UnreachableProgramStep {
                operation: id("operation.charge_payment"),
                location: at(&[(1, None)]),
            },
        ]
    );
}

#[test]
fn rejects_a_variant_payload_outside_its_arm() {
    let mut model = load_flash_checkout();

    let OperationStep::MatchResult(matched) =
        &mut program_mut(&mut model, "operation.charge_payment").steps[1]
    else {
        panic!("expected the card match");
    };

    // The failure publication reads the err payload; moving it into the
    // ok arm puts that reference out of scope.
    let failed = matched.err.steps.remove(0);

    matched.ok.steps.insert(0, failed);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::EffectResultVariantOutOfScope {
            operation: id("operation.charge_payment"),
            location: at(&[(1, Some(Arm::Ok)), (0, None)]),
            result: id("result.charge_payment.card"),
            variant: ResultVariant::Err,
            consumer: ProgramUse::Effect {
                effect: id("effect.charge_payment.publish_failed"),
            },
        }]
    );
}

#[test]
fn rejects_a_variant_payload_after_the_join() {
    let mut model = load_flash_checkout();

    let program = program_mut(&mut model, "operation.charge_payment");

    let OperationStep::MatchResult(matched) = &mut program.steps[1] else {
        panic!("expected the card match");
    };

    // Both arms fall through; the failure publication follows the join.
    let failed = matched.err.steps.remove(0);

    matched.ok.steps.clear();
    matched.err.steps.clear();

    program.steps.push(failed);
    program.steps.push(OperationStep::Complete);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::EffectResultVariantOutOfScope {
            operation: id("operation.charge_payment"),
            location: at(&[(2, None)]),
            result: id("result.charge_payment.card"),
            variant: ResultVariant::Err,
            consumer: ProgramUse::Effect {
                effect: id("effect.charge_payment.publish_failed"),
            },
        }]
    );
}

#[test]
fn rejects_an_intent_executed_where_no_path_establishes_it() {
    let mut model = load_flash_checkout();

    model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.create_order.new"))
        .unwrap()
        .steps
        .retain(|step| !matches!(step, TransactionStep::EstablishEffectIntent(_)));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionArtifactNotAvailable {
            operation: id("operation.create_order"),
            location: at(&[(1, None)]),
            artifact: id("intent.create_order.publish_created"),
            consumer: ProgramUse::EffectIntent {
                intent: id("intent.create_order.publish_created"),
            },
        }]
    );
}

#[test]
fn rejects_an_output_consumed_where_no_path_establishes_it() {
    let mut model = load_flash_checkout();

    model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.create_order.new"))
        .unwrap()
        .steps
        .retain(|step| !matches!(step, TransactionStep::EstablishTransactionOutput(_)));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionArtifactNotAvailable {
            operation: id("operation.create_order"),
            location: at(&[(2, None)]),
            artifact: id("output.create_order"),
            consumer: ProgramUse::Return {
                request: id("input.create_order.request"),
            },
        }]
    );
}

/// Wraps the first step of create_order's program in a branch on the
/// request's `sku`, with the given arms.
fn branch_create_order(
    model: &mut Model,
    then: Vec<OperationStep>,
    otherwise: Option<Vec<OperationStep>>,
) {
    let program = program_mut(model, "operation.create_order");

    program.steps.remove(0);

    program.steps.insert(
        0,
        OperationStep::Branch(Branch {
            condition: Condition::Eq {
                value: input_ref("input.create_order.request", &["sku"]),
                equals: SelectorValue::Literal(Literal::String("bundle".into())),
            },
            then: OperationBlock { steps: then },
            otherwise: otherwise.map(|steps| OperationBlock { steps }),
        }),
    );
}

#[test]
fn accepts_an_artifact_established_on_every_arm() {
    let mut model = load_flash_checkout();

    branch_create_order(
        &mut model,
        vec![run("tx.create_order.new")],
        Some(vec![run("tx.create_order.new")]),
    );

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
}

#[test]
fn rejects_an_artifact_established_on_one_arm_only() {
    let mut model = load_flash_checkout();

    branch_create_order(&mut model, vec![run("tx.create_order.new")], None);

    let errors = validation::validate(&model);

    // Falling through the branch establishes nothing, so neither the
    // intent execution nor the returned payload is definitely supplied.
    assert_eq!(
        errors,
        vec![
            ValidationError::TransactionArtifactNotAvailable {
                operation: id("operation.create_order"),
                location: at(&[(1, None)]),
                artifact: id("intent.create_order.publish_created"),
                consumer: ProgramUse::EffectIntent {
                    intent: id("intent.create_order.publish_created"),
                },
            },
            ValidationError::TransactionArtifactNotAvailable {
                operation: id("operation.create_order"),
                location: at(&[(2, None)]),
                artifact: id("output.create_order"),
                consumer: ProgramUse::Return {
                    request: id("input.create_order.request"),
                },
            },
        ]
    );
}

#[test]
fn rejects_a_step_after_a_terminal() {
    let mut model = load_flash_checkout();

    program_mut(&mut model, "operation.create_order")
        .steps
        .push(OperationStep::Complete);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::UnreachableProgramStep {
            operation: id("operation.create_order"),
            location: at(&[(3, None)]),
        }]
    );
}

#[test]
fn rejects_a_return_on_a_subscription_input() {
    let mut model = load_flash_checkout();

    program_mut(&mut model, "operation.reserve_inventory").steps[2] =
        return_ok("input.reserve_inventory.created", Derivation::Unspecified);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidInputKind {
            subject: id("operation.reserve_inventory"),
            input: id("input.reserve_inventory.created"),
            expected: validation::InputKind::Request,
            actual: validation::InputKind::Subscription,
        }]
    );
}

#[test]
fn rejects_a_return_for_another_operations_request() {
    let mut model = load_flash_checkout();

    program_mut(&mut model, "operation.reserve_inventory").steps[2] =
        return_ok("input.create_order.request", Derivation::Unspecified);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidReferenceOwner {
            subject: id("operation.reserve_inventory"),
            reference: id("input.create_order.request"),
            expected_owner: id("operation.reserve_inventory"),
            actual_owner: Some(id("operation.create_order")),
        }]
    );
}

#[test]
fn rejects_a_variant_field_path_that_does_not_resolve() {
    let mut model = load_flash_checkout();

    let OperationStep::MatchResult(matched) =
        &mut program_mut(&mut model, "operation.charge_payment").steps[1]
    else {
        panic!("expected the card match");
    };

    let OperationStep::ExecuteEffect(failed) = &mut matched.err.steps[0] else {
        panic!("expected the failure publication");
    };

    let Derivation::Deterministic { from } = &mut failed.values else {
        panic!("expected a deterministic derivation");
    };

    from[2].path = path(&["does_not_exist"]);

    let errors = validation::validate(&model);

    // The err payload resolves against the provider's err schema.
    assert_eq!(
        errors,
        vec![ValidationError::InvalidFieldPath {
            subject: id("operation.charge_payment"),
            schema: id("schema.ChargeDeclined"),
            path: path(&["does_not_exist"]),
        }]
    );
}

#[test]
fn rejects_a_transaction_output_field_path_that_does_not_resolve() {
    let mut model = load_flash_checkout();

    let OperationStep::Return(returned) =
        &mut program_mut(&mut model, "operation.create_order").steps[2]
    else {
        panic!("expected the return");
    };

    let ResultOutcome::Ok {
        values: Derivation::Deterministic { from },
    } = &mut returned.outcome
    else {
        panic!("expected a deterministic ok outcome");
    };

    from[0].path = path(&["does_not_exist"]);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidFieldPath {
            subject: id("operation.create_order"),
            schema: id("schema.CreateOrderResponse"),
            path: path(&["does_not_exist"]),
        }]
    );
}

#[test]
fn rejects_duplicate_result_bindings() {
    let mut model = load_flash_checkout();

    let program = program_mut(&mut model, "operation.charge_payment");

    let first = program.steps[0].clone();

    program.steps.insert(0, first);

    let errors = validation::validate(&model);

    assert_eq!(errors.len(), 1);

    assert!(matches!(
        &errors[0],
        ValidationError::DuplicateId { id: duplicate, .. }
            if duplicate == &id("result.charge_payment.card")
    ));
}

#[test]
fn rejects_a_condition_root_out_of_scope() {
    let mut model = load_flash_checkout();

    program_mut(&mut model, "operation.create_order")
        .steps
        .insert(
            0,
            OperationStep::Branch(Branch {
                condition: Condition::Eq {
                    value: input_ref("input.cancel_order.request", &["order_id"]),
                    equals: SelectorValue::Literal(Literal::Int(1)),
                },
                then: OperationBlock::default(),
                otherwise: None,
            }),
        );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ValueSourceOutOfScope {
            subject: id("operation.create_order"),
            source: id("input.cancel_order.request"),
            owner: id("operation.cancel_order"),
        }]
    );
}

/// Adds a second output to create_order, established by a new keyed
/// transaction whose derivation reads the first output.
fn add_receipt_transaction(model: &mut Model) {
    let operation = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap();

    operation.transaction_outputs.insert(
        id("output.create_order.receipt"),
        TransactionOutput {
            schema: id("schema.CreateOrderResponse"),
        },
    );

    operation.transactions.insert(
        id("tx.create_order.receipt"),
        Transaction {
            data_model: None,
            isolation: TransactionIsolation::Unspecified,
            idempotency: IdempotencyGuarantee::Unspecified,
            steps: vec![TransactionStep::EstablishTransactionOutput(
                EstablishTransactionOutput {
                    output: id("output.create_order.receipt"),
                    values: Derivation::Deterministic {
                        from: vec![ValueRef {
                            source: ValueSource::TransactionOutput(id("output.create_order")),
                            path: path(&["order_id"]),
                        }],
                    },
                },
            )],
        },
    );
}

#[test]
fn accepts_an_output_consumed_by_a_later_transaction() {
    let mut model = load_flash_checkout();

    add_receipt_transaction(&mut model);

    program_mut(&mut model, "operation.create_order")
        .steps
        .insert(1, run("tx.create_order.receipt"));

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
}

#[test]
fn rejects_an_output_consumed_by_a_transaction_before_it_is_available() {
    let mut model = load_flash_checkout();

    add_receipt_transaction(&mut model);

    program_mut(&mut model, "operation.create_order")
        .steps
        .insert(0, run("tx.create_order.receipt"));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionArtifactNotAvailable {
            operation: id("operation.create_order"),
            location: at(&[(0, None)]),
            artifact: id("output.create_order"),
            consumer: ProgramUse::Transaction {
                transaction: id("tx.create_order.receipt"),
            },
        }]
    );
}

#[test]
fn a_same_transaction_output_reference_needs_only_step_order() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap();

    operation.transaction_outputs.insert(
        id("output.create_order.receipt"),
        TransactionOutput {
            schema: id("schema.CreateOrderResponse"),
        },
    );

    let establish = TransactionStep::EstablishTransactionOutput(EstablishTransactionOutput {
        output: id("output.create_order.receipt"),
        values: Derivation::Deterministic {
            from: vec![ValueRef {
                source: ValueSource::TransactionOutput(id("output.create_order")),
                path: path(&["order_id"]),
            }],
        },
    });

    // After the step that establishes the first output: satisfied by
    // atomicity, whatever the program guarantees at entry.
    let transaction = operation
        .transactions
        .get_mut(&id("tx.create_order.new"))
        .unwrap();

    transaction.steps.push(establish.clone());

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");

    // Before it: the reference reads an output no step has produced.
    let transaction = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.create_order.new"))
        .unwrap();

    transaction.steps.pop();
    transaction.steps.insert(0, establish);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionArtifactNotAvailable {
            operation: id("operation.create_order"),
            location: at(&[(0, None)]),
            artifact: id("output.create_order"),
            consumer: ProgramUse::Transaction {
                transaction: id("tx.create_order.new"),
            },
        }]
    );
}

#[test]
fn accepts_a_variant_payload_in_a_transaction_used_inside_its_arm() {
    let mut model = load_flash_checkout();

    // A transaction reading the provider's ok payload, executed only
    // inside the ok arm.
    let operation = model
        .operations
        .get_mut(&id("operation.charge_payment"))
        .unwrap();

    operation.transaction_outputs.insert(
        id("output.charge_payment.authorization"),
        TransactionOutput {
            schema: id("schema.ChargeAccepted"),
        },
    );

    operation.transactions.insert(
        id("tx.charge_payment.record"),
        Transaction {
            data_model: None,
            isolation: TransactionIsolation::Unspecified,
            idempotency: IdempotencyGuarantee::Unspecified,
            steps: vec![TransactionStep::EstablishTransactionOutput(
                EstablishTransactionOutput {
                    output: id("output.charge_payment.authorization"),
                    values: Derivation::Deterministic {
                        from: vec![ValueRef {
                            source: ValueSource::EffectResultOk(id("result.charge_payment.card")),
                            path: path(&["authorization_id"]),
                        }],
                    },
                },
            )],
        },
    );

    let OperationStep::MatchResult(matched) = &mut operation.program.steps[1] else {
        panic!("expected the card match");
    };

    matched.ok.steps.insert(0, run("tx.charge_payment.record"));

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");

    // Executed after the join, the same transaction reads a payload no
    // arm selects.
    let program = program_mut(&mut model, "operation.charge_payment");

    let OperationStep::MatchResult(matched) = &mut program.steps[1] else {
        panic!("expected the card match");
    };

    matched.ok.steps.remove(0);
    matched.ok.steps.pop();
    matched.err.steps.pop();

    program.steps.push(run("tx.charge_payment.record"));
    program.steps.push(OperationStep::Complete);

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::EffectResultVariantOutOfScope {
            operation: id("operation.charge_payment"),
            location: at(&[(2, None)]),
            result: id("result.charge_payment.card"),
            variant: ResultVariant::Ok,
            consumer: ProgramUse::Transaction {
                transaction: id("tx.charge_payment.record"),
            },
        }]
    );
}

#[test]
fn intent_execution_evaluates_the_effect_declarations_roots() {
    let mut model = load_flash_checkout();

    // The transition-owned effect gains a propagation rooted in an
    // output the operation declares but never establishes; the intent
    // execution is where the declaration is evaluated.
    let operation = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap();

    operation.transaction_outputs.insert(
        id("output.apply_payment.receipt"),
        TransactionOutput {
            schema: id("schema.OrderPaid"),
        },
    );

    let machine = model
        .state_machines
        .get_mut(&id("machine.order_lifecycle"))
        .unwrap();

    let transition = machine
        .transitions
        .get_mut(&id("transition.order.mark_paid"))
        .unwrap();

    let archspec::spec::TransitionSideEffect::Publication(paid) = transition
        .side_effects
        .get_mut(&id("effect.order.paid"))
        .unwrap()
    else {
        panic!("effect.order.paid should be a publication");
    };

    paid.idempotency_key_propagation
        .push(archspec::spec::IdempotencyKeyPropagation {
            source: archspec::spec::IdempotencyKey {
                components: vec![ValueRef {
                    source: ValueSource::TransactionOutput(id("output.apply_payment.receipt")),
                    path: path(&["order_id"]),
                }],
            },
            target: archspec::spec::IdempotencyKey {
                components: vec![ValueRef {
                    source: ValueSource::Effect(id("effect.order.paid")),
                    path: path(&["event_id"]),
                }],
            },
        });

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionArtifactNotAvailable {
            operation: id("operation.apply_payment"),
            location: at(&[(1, None)]),
            artifact: id("output.apply_payment.receipt"),
            consumer: ProgramUse::EffectIntent {
                intent: id("intent.apply_payment.order_paid"),
            },
        }]
    );
}

#[test]
fn a_nested_match_keeps_the_enclosing_arms_variant_selection() {
    let mut model = load_flash_checkout();

    // A redundant nested match on the same binding inside the ok arm;
    // the step after its join is still inside the outer ok arm, so the
    // ok payload stays in scope there.
    let OperationStep::MatchResult(matched) =
        &mut program_mut(&mut model, "operation.charge_payment").steps[1]
    else {
        panic!("expected the card match");
    };

    let OperationStep::ExecuteEffect(captured) = &mut matched.ok.steps[0] else {
        panic!("expected the capture publication");
    };

    let Derivation::Deterministic { from } = &mut captured.values else {
        panic!("expected a deterministic derivation");
    };

    from.push(ValueRef {
        source: ValueSource::EffectResultOk(id("result.charge_payment.card")),
        path: path(&["authorization_id"]),
    });

    matched.ok.steps.insert(
        0,
        OperationStep::MatchResult(archspec::spec::MatchResult {
            result: id("result.charge_payment.card"),
            ok: OperationBlock::default(),
            err: OperationBlock::default(),
        }),
    );

    let errors = validation::validate(&model);

    assert!(errors.is_empty(), "expected no errors, got:\n{errors:#?}");
}
