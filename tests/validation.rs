use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use archspec::{
    analyzer::validation::{self, ReferenceKind, ValidationError},
    parser::yaml,
    spec::{
        CompletionRequirement, Derivation, EffectIntent, EstablishEffectIntent, FieldPath,
        FlowStep, Id, IdempotencyGuarantee, IdempotencyKey, Input, MessageSelector, Model,
        RecoverabilityRequirement, Schema, SchemaFragment, StateTransition, TopicOrdering,
        TransactionStep, ValueRef, ValueSource,
    },
};

fn id(value: &str) -> Id {
    Id(value.to_owned())
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
fn rejects_flow_using_transaction_owned_by_another_operation() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap();

    let flow = operation
        .flows
        .get_mut(&id("flow.reserve_inventory"))
        .unwrap();

    flow.steps[0] = FlowStep::Transaction {
        transaction: id("tx.create_order.new"),
    };

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidReferenceOwner {
            subject: id("flow.reserve_inventory"),
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
fn rejects_response_and_invocation_result_schema_mismatch() {
    let mut model = load_flash_checkout();

    let response = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .responses
        .get_mut(&id("response.create_order"))
        .unwrap();

    response.schema = id("schema.CancelOrderResponse");

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ResponseInvocationResultSchemaMismatch {
            response: id("response.create_order"),
            response_schema: id("schema.CancelOrderResponse"),
            result: id("result.create_order"),
            result_schema: id("schema.CreateOrderResponse"),
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
fn rejects_transition_transaction_without_keyed_idempotency() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.apply_payment"))
        .unwrap();

    transaction.idempotency = IdempotencyGuarantee::Unspecified;

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransitionTransactionNotDeduplicated {
            transaction: id("tx.apply_payment"),
            machine: id("machine.order_lifecycle"),
            transition: id("transition.order.mark_paid"),
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

    let flow = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .flows
        .get_mut(&id("flow.apply_payment"))
        .unwrap();

    flow.steps[1] = FlowStep::ExecuteEffect {
        effect: id("effect.order.paid"),
        values: Derivation::Unspecified,
    };

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::InvalidReferenceOwner {
            subject: id("flow.apply_payment"),
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
fn rejects_recoverability_requirement_with_no_flow() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.charge_payment"))
        .unwrap();

    operation
        .requirements
        .recoverability
        .push(RecoverabilityRequirement {
            key: IdempotencyKey {
                components: vec![ValueRef {
                    source: ValueSource::Input(id("input.charge_payment.reserved")),
                    path: FieldPath(vec!["event_id".to_owned()]),
                }],
            },
            completion: CompletionRequirement::Guaranteed,
        });

    operation.flows.clear();

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::RecoverabilityRequiresFlow {
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
fn rejects_value_ref_to_another_operations_invocation_result() {
    let mut model = load_flash_checkout();

    set_reserve_write_source(
        &mut model,
        ValueSource::InvocationResult(id("result.create_order")),
        &["order_id"],
    );

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::ValueSourceOutOfScope {
            subject: id("tx.reserve_inventory"),
            source: id("result.create_order"),
            owner: id("operation.create_order"),
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

/// Retarget the card-charge flow step's derivation, which is the
/// fixture's direct-execution provenance site.
fn set_charge_card_values(model: &mut Model, values: Derivation) {
    let flow = model
        .operations
        .get_mut(&id("operation.charge_payment"))
        .unwrap()
        .flows
        .get_mut(&id("flow.charge_payment"))
        .unwrap();

    let FlowStep::ExecuteEffect {
        values: step_values,
        ..
    } = &mut flow.steps[0]
    else {
        panic!("expected the card-charge execute_effect step");
    };

    *step_values = values;
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

    // No transaction context exists at flow level, so a transaction
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
            subject: id("flow.charge_payment"),
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
            subject: id("flow.charge_payment"),
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
