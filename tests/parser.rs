use std::{
    fs,
    path::{Path, PathBuf},
};

use archspec::{
    parser::yaml,
    spec::{
        CompletionRequirement, Derivation, Effect, Id, IdempotencyGuarantee, Input,
        LaneConcurrency, Schema, SchemaCompleteness, ServiceKind, TopicOrdering, TransactionStep,
        TransitionSideEffect, ValueSource,
    },
};

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn read_fixture(name: &str) -> String {
    let path = fixture_path(name);

    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read fixture `{}`: {error}", path.display()))
}

#[test]
fn parses_minimal_model() {
    let source = read_fixture("minimal.yaml");

    let model = yaml::parse(&source).expect("minimal fixture should parse");

    assert_eq!(model.revision.0, 1);

    assert_eq!(model.services.len(), 1);
    assert_eq!(model.schemas.len(), 1);
    assert_eq!(model.topics.len(), 1);

    assert!(model.data_models.is_empty());
    assert!(model.state_machines.is_empty());
    assert!(model.operations.is_empty());

    let checkout = model
        .services
        .get(&Id("checkout".into()))
        .expect("checkout service should exist");

    assert_eq!(checkout.kind, ServiceKind::Backend);

    let order_created = model
        .schemas
        .get(&Id("OrderCreated".into()))
        .expect("OrderCreated schema should exist");

    let Schema::Canonical(order_created) = order_created else {
        panic!("OrderCreated should be a canonical schema");
    };

    assert_eq!(order_created.completeness, SchemaCompleteness::Complete);

    assert!(order_created.fields.contains_key("order_id"));
    assert!(order_created.fields.contains_key("quantity"));

    let order_events = model
        .topics
        .get(&Id("order_events".into()))
        .expect("order_events topic should exist");

    assert!(order_events.messages.contains(&Id("OrderCreated".into())));

    assert_eq!(order_events.ordering, TopicOrdering::Unordered);
}

#[test]
fn parses_keyed_topic_model() {
    let source = read_fixture("keyed_topic.yaml");

    let model = yaml::parse(&source).expect("keyed topic fixture should parse");

    assert_eq!(model.revision.0, 2);

    assert_eq!(model.services.len(), 2);
    assert_eq!(model.schemas.len(), 2);
    assert_eq!(model.topics.len(), 1);

    let checkout = model
        .services
        .get(&Id("checkout".into()))
        .expect("checkout service should exist");

    assert_eq!(checkout.kind, ServiceKind::Backend);

    let payments = model
        .services
        .get(&Id("payments".into()))
        .expect("payments service should exist");

    assert_eq!(payments.kind, ServiceKind::Worker);

    let order_event = model
        .schemas
        .get(&Id("OrderEvent".into()))
        .expect("OrderEvent schema should exist");

    let Schema::Canonical(order_event) = order_event else {
        panic!("OrderEvent should be canonical");
    };

    assert_eq!(order_event.completeness, SchemaCompleteness::Complete);

    assert!(order_event.fields.contains_key("order_id"));
    assert!(order_event.fields.contains_key("event_id"));
    assert!(order_event.fields.contains_key("event_type"));

    let order_identity = model
        .schemas
        .get(&Id("OrderIdentity".into()))
        .expect("OrderIdentity schema should exist");

    let Schema::Fragment(order_identity) = order_identity else {
        panic!("OrderIdentity should be a fragment");
    };

    assert_eq!(order_identity.source, Id("OrderEvent".into()));

    let order_id_mapping = order_identity
        .mapping
        .get("order_id")
        .expect("fragment should map order_id");

    assert_eq!(order_id_mapping.0, vec!["order_id".to_string()]);

    let topic = model
        .topics
        .get(&Id("order_events".into()))
        .expect("order_events topic should exist");

    assert_eq!(
        topic.messages,
        [Id("OrderEvent".into())].into_iter().collect()
    );

    let TopicOrdering::Keyed(key) = &topic.ordering else {
        panic!("order_events should use keyed ordering");
    };

    let order_event_key = key
        .mapping
        .get(&Id("OrderEvent".into()))
        .expect("OrderEvent should define its topic ordering key");

    assert_eq!(order_event_key.0, vec!["order_id".to_string()]);
}

#[test]
fn serializes_and_reparses_minimal_model() {
    let source = read_fixture("minimal.yaml");

    let original = yaml::parse(&source).expect("minimal fixture should parse");

    let serialized = yaml::serialize(&original).expect("model should serialize");

    let reparsed = yaml::parse(&serialized).expect("serialized model should parse");

    assert_eq!(original, reparsed);
}

#[test]
fn serializes_and_reparses_keyed_topic_model() {
    let source = read_fixture("keyed_topic.yaml");

    let original = yaml::parse(&source).expect("keyed topic fixture should parse");

    let serialized = yaml::serialize(&original).expect("model should serialize");

    let reparsed = yaml::parse(&serialized).expect("serialized model should parse");

    assert_eq!(original, reparsed);
}

#[test]
fn serialization_is_canonical() {
    let source = read_fixture("keyed_topic.yaml");

    let model = yaml::parse(&source).expect("fixture should parse");

    let first = yaml::serialize(&model).expect("model should serialize");

    let reparsed = yaml::parse(&first).expect("serialized model should parse");

    let second = yaml::serialize(&reparsed).expect("reparsed model should serialize");

    assert_eq!(
        first, second,
        "serializing a canonical model twice should produce identical YAML"
    );
}

#[test]
fn rejects_invalid_service_kind() {
    let source = read_fixture("invalid_service_kind.yaml");

    let error = yaml::parse(&source).expect_err("invalid service kind should fail to deserialize");

    let message = error.to_string();

    assert!(
        message.contains("definitely_not_a_service"),
        "error should mention the invalid value, got: {message}"
    );
}

#[test]
fn parses_flash_checkout_model() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    assert_eq!(model.revision.0, 1);

    assert_eq!(model.services.len(), 3);
    assert_eq!(model.schemas.len(), 13);
    assert_eq!(model.data_models.len(), 2);
    assert_eq!(model.topics.len(), 1);
    assert_eq!(model.state_machines.len(), 1);
    assert_eq!(model.operations.len(), 6);

    assert!(
        model
            .operations
            .contains_key(&Id("operation.create_order".into()))
    );

    assert!(
        model
            .operations
            .contains_key(&Id("operation.reserve_inventory".into()))
    );

    assert!(
        model
            .operations
            .contains_key(&Id("operation.charge_payment".into()))
    );

    assert!(
        model
            .operations
            .contains_key(&Id("operation.cancel_order".into()))
    );

    assert!(
        model
            .operations
            .contains_key(&Id("operation.apply_payment".into()))
    );

    assert!(
        model
            .operations
            .contains_key(&Id("operation.transfer_stock".into()))
    );
}

#[test]
fn flash_checkout_parses_nested_semantics() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    // Subscription + MessageSelector + DispatchSemantics +
    // LaneConcurrency.
    let reserve_inventory = model
        .operations
        .get(&Id("operation.reserve_inventory".into()))
        .expect("reserve_inventory should exist");

    let input = reserve_inventory
        .inputs
        .get(&Id("input.reserve_inventory.created".into()))
        .expect("reserve_inventory subscription should exist");

    let Input::Subscription(subscription) = input else {
        panic!("reserve_inventory input should be a subscription");
    };

    let LaneConcurrency::Bounded(concurrency) = subscription.dispatch.lane_concurrency else {
        panic!("subscription lane concurrency should be bounded");
    };

    assert_eq!(concurrency.get(), 1);

    // Effect + nested IdempotencyGuarantee.
    let charge_payment = model
        .operations
        .get(&Id("operation.charge_payment".into()))
        .expect("charge_payment should exist");

    let effect = charge_payment
        .effects
        .get(&Id("effect.charge_payment.card".into()))
        .expect("card charge effect should exist");

    let Effect::External(effect) = effect else {
        panic!("card charge should be an external effect");
    };

    assert_eq!(effect.idempotency, IdempotencyGuarantee::NotDeduplicated);

    // TransactionStep + SelectorPredicate + SelectorValue +
    // FieldSelection + LockOrder.
    let transfer_stock = model
        .operations
        .get(&Id("operation.transfer_stock".into()))
        .expect("transfer_stock should exist");

    let transaction = transfer_stock
        .transactions
        .get(&Id("tx.transfer_stock".into()))
        .expect("transfer_stock transaction should exist");

    assert_eq!(transaction.steps.len(), 5);

    assert!(matches!(&transaction.steps[0], TransactionStep::Lock(_)));

    assert!(matches!(&transaction.steps[1], TransactionStep::Lock(_)));

    assert!(matches!(&transaction.steps[2], TransactionStep::Read(_)));

    assert!(matches!(&transaction.steps[3], TransactionStep::Write(_)));

    assert!(matches!(&transaction.steps[4], TransactionStep::Write(_)));
}

#[test]
fn flash_checkout_round_trips_through_yaml() {
    let source = read_fixture("flash_checkout.yaml");

    let original = yaml::parse(&source).expect("flash checkout fixture should parse");

    let serialized = yaml::serialize(&original).expect("flash checkout model should serialize");

    let reparsed = yaml::parse(&serialized).expect("serialized flash checkout model should parse");

    assert_eq!(original, reparsed);
}

#[test]
fn flash_checkout_serialization_is_canonical() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let first = yaml::serialize(&model).expect("flash checkout model should serialize");

    let reparsed = yaml::parse(&first).expect("canonical YAML should parse");

    let second = yaml::serialize(&reparsed).expect("reparsed model should serialize");

    assert_eq!(first, second);
}

#[test]
fn flash_checkout_parses_keyed_transaction_idempotency() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let transaction = model
        .operations
        .get(&Id("operation.create_order".into()))
        .expect("create_order should exist")
        .transactions
        .get(&Id("tx.create_order.new".into()))
        .expect("create_order transaction should exist");

    let IdempotencyGuarantee::DeduplicatedBy { key } = &transaction.idempotency else {
        panic!("create_order transaction should declare keyed commit deduplication");
    };

    assert_eq!(key.components.len(), 1);

    assert_eq!(
        key.components[0].source,
        ValueSource::Input(Id("input.create_order.request".into()))
    );

    assert_eq!(
        key.components[0].path.0,
        vec!["idempotency_key".to_string()]
    );

    // The artifact carries no key of its own; durable identity comes
    // from the committing transaction.
    let result = model
        .operations
        .get(&Id("operation.create_order".into()))
        .unwrap()
        .invocation_results
        .get(&Id("result.create_order".into()))
        .expect("create_order result should exist");

    assert_eq!(result.schema, Id("schema.CreateOrderResponse".into()));
}

#[test]
fn flash_checkout_parses_transaction_read_provenance() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let transaction = model
        .operations
        .get(&Id("operation.reserve_inventory".into()))
        .expect("reserve_inventory should exist")
        .transactions
        .get(&Id("tx.reserve_inventory".into()))
        .expect("reserve_inventory transaction should exist");

    let TransactionStep::Read(read) = &transaction.steps[0] else {
        panic!("first step should be a read");
    };

    assert_eq!(read.result, Id("read.reserve_inventory.stock".into()));

    let TransactionStep::Write(write) = &transaction.steps[1] else {
        panic!("second step should be a write");
    };

    let Derivation::Deterministic { from } = &write.values else {
        panic!("write should declare deterministic value provenance");
    };

    assert_eq!(
        from[0].source,
        ValueSource::TransactionRead(Id("read.reserve_inventory.stock".into()))
    );

    // Provenance is declared even where V1 will not use it to prove
    // natural replayability.
    assert_eq!(from[0].path.0, vec!["reserved".to_string()]);
}

#[test]
fn flash_checkout_parses_transition_side_effect_intent() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let transition = model
        .state_machines
        .get(&Id("machine.order_lifecycle".into()))
        .expect("order lifecycle should exist")
        .transitions
        .get(&Id("transition.order.mark_paid".into()))
        .expect("mark_paid transition should exist");

    let effect = transition
        .side_effects
        .get(&Id("effect.order.paid".into()))
        .expect("mark_paid should declare a side effect");

    assert!(matches!(effect, TransitionSideEffect::Publication(_)));

    // The operation names that implicitly established intent so a flow
    // step can execute it.
    let apply_payment = model
        .operations
        .get(&Id("operation.apply_payment".into()))
        .expect("apply_payment should exist");

    let intent = apply_payment
        .effect_intents
        .get(&Id("intent.apply_payment.order_paid".into()))
        .expect("apply_payment should declare the transition intent");

    assert_eq!(intent.effect, Id("effect.order.paid".into()));

    // The transaction establishes no intent explicitly.
    let transaction = apply_payment
        .transactions
        .get(&Id("tx.apply_payment".into()))
        .expect("apply_payment transaction should exist");

    assert!(
        transaction
            .steps
            .iter()
            .all(|step| !matches!(step, TransactionStep::EstablishEffectIntent(_)))
    );
}

#[test]
fn flash_checkout_parses_unspecified_derivation() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let transaction = model
        .operations
        .get(&Id("operation.transfer_stock".into()))
        .expect("transfer_stock should exist")
        .transactions
        .get(&Id("tx.transfer_stock".into()))
        .expect("transfer_stock transaction should exist");

    assert_eq!(transaction.idempotency, IdempotencyGuarantee::Unspecified);

    let TransactionStep::Write(write) = &transaction.steps[4] else {
        panic!("fifth step should be the destination write");
    };

    assert_eq!(write.values, Derivation::Unspecified);
}

#[test]
fn flash_checkout_parses_recoverability_requirements() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    // Request-driven: no retry driver is modeled, so only resumability
    // is required.
    let create_order = model
        .operations
        .get(&Id("operation.create_order".into()))
        .expect("create_order should exist");

    let requirement = &create_order.requirements.recoverability[0];

    assert_eq!(requirement.completion, CompletionRequirement::Resumable);

    assert_eq!(
        requirement.key.components[0].source,
        ValueSource::Input(Id("input.create_order.request".into()))
    );

    // Subscription-driven with at-least-once delivery: completion is
    // required outright.
    let apply_payment = model
        .operations
        .get(&Id("operation.apply_payment".into()))
        .expect("apply_payment should exist");

    let requirement = &apply_payment.requirements.recoverability[0];

    assert_eq!(requirement.completion, CompletionRequirement::Guaranteed);

    assert_eq!(
        requirement.key.components[0].path.0,
        vec!["event_id".to_string()]
    );

    // Recoverability is independent of idempotency: both are declared
    // here, keyed by the same logical invocation identity.
    assert_eq!(apply_payment.requirements.idempotency.len(), 1);

    assert_eq!(
        apply_payment.requirements.idempotency[0].key,
        requirement.key
    );

    // An operation may require neither.
    let transfer_stock = model
        .operations
        .get(&Id("operation.transfer_stock".into()))
        .expect("transfer_stock should exist");

    assert!(transfer_stock.requirements.recoverability.is_empty());
}
