use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use archspec::{
    analyzer::validation::{self, ReferenceKind, ValidationError},
    parser::yaml,
    spec::{
        FieldPath, FlowStep, Id, Input, MessageSelector, Model, Schema, SchemaFragment,
        TopicOrdering, TransactionStep, ValueSource,
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
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.apply_payment"))
        .unwrap();

    transaction.data_model = None;

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionMissingDataModel {
            transaction: id("tx.apply_payment"),
            object: id("object.order"),
        }]
    );
}

#[test]
fn rejects_transaction_access_outside_declared_data_model() {
    let mut model = load_flash_checkout();

    let transaction = model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.apply_payment"))
        .unwrap();

    transaction.data_model = Some(id("data.inventory"));

    let errors = validation::validate(&model);

    assert_eq!(
        errors,
        vec![ValidationError::TransactionObjectOutsideDataModel {
            transaction: id("tx.apply_payment"),
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

    let TransactionStep::Transition(transition) = &mut transaction.steps[0] else {
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
