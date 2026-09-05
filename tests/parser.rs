use std::{
    fs,
    path::{Path, PathBuf},
};

use conseqa::{
    parser::yaml,
    spec::{
        CompletionRequirement, Condition, Derivation, Effect, ErrorDisposition, ErrorResultType,
        Field, FieldPath, Id, IdempotencyGuarantee, Input, LaneConcurrency, Literal,
        MessageIdentity, OperationStep, RequestIdentity, ResultOutcome, ResultVariant, ScalarType,
        Schema, SchemaCompleteness, SelectorValue, ServiceKind, TopicOrdering, TransactionStep,
        TransitionSideEffect, TypeRef, ValueSource,
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

    // The ordering key and the message identity are separate
    // declarations: order_id sequences events for an order, event_id
    // identifies one logical message.
    let MessageIdentity::Keyed { mapping } = &topic.message_identity else {
        panic!("order_events should declare a keyed message identity");
    };

    let order_event_identity = mapping
        .get(&Id("OrderEvent".into()))
        .expect("OrderEvent should define its message identity");

    assert_eq!(order_event_identity.len(), 1);
    assert_eq!(order_event_identity[0].0, vec!["event_id".to_string()]);
}

#[test]
fn flash_checkout_parses_stimulus_identities() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let input = model
        .operations
        .get(&Id("operation.create_order".into()))
        .expect("create_order should exist")
        .inputs
        .get(&Id("input.create_order.request".into()))
        .expect("create_order request should exist");

    let Input::Request(request) = input else {
        panic!("create_order input should be a request");
    };

    let RequestIdentity::Keyed { fields } = &request.identity else {
        panic!("create_order request should declare a keyed identity");
    };

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].0, vec!["idempotency_key".to_string()]);

    let topic = model
        .topics
        .get(&Id("topic.order_events".into()))
        .expect("order_events topic should exist");

    let MessageIdentity::Keyed { mapping } = &topic.message_identity else {
        panic!("order_events should declare a keyed message identity");
    };

    // Every carried schema is identified by its event_id.
    assert_eq!(mapping.len(), 6);

    for identity in mapping.values() {
        assert_eq!(identity.len(), 1);
        assert_eq!(identity[0].0, vec!["event_id".to_string()]);
    }
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
    assert_eq!(model.schemas.len(), 17);
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
    let output = model
        .operations
        .get(&Id("operation.create_order".into()))
        .unwrap()
        .transaction_outputs
        .get(&Id("output.create_order".into()))
        .expect("create_order output should exist");

    assert_eq!(output.schema, Id("schema.CreateOrderResponse".into()));
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

    // The operation names that implicitly established intent so a
    // program step can execute it.
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

#[test]
fn flash_checkout_parses_execute_effect_values_and_result_bindings() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let program = &model
        .operations
        .get(&Id("operation.charge_payment".into()))
        .expect("charge_payment should exist")
        .program;

    // Unknown provenance is declared explicitly, never omitted; the
    // card charge binds the provider's result.
    let OperationStep::ExecuteEffect(card) = &program.steps[0] else {
        panic!("first step should execute the card charge");
    };

    assert_eq!(card.effect, Id("effect.charge_payment.card".into()));
    assert_eq!(card.values, Derivation::Unspecified);
    assert_eq!(card.result, Some(Id("result.charge_payment.card".into())));

    let OperationStep::MatchResult(matched) = &program.steps[1] else {
        panic!("second step should match the card result");
    };

    assert_eq!(matched.result, Id("result.charge_payment.card".into()));

    let OperationStep::ExecuteEffect(captured) = &matched.ok.steps[0] else {
        panic!("the ok arm should publish the capture");
    };

    assert_eq!(
        captured.effect,
        Id("effect.charge_payment.publish_captured".into())
    );

    assert_eq!(captured.result, None);

    let Derivation::Deterministic { from } = &captured.values else {
        panic!("publication values should declare deterministic provenance");
    };

    assert_eq!(from.len(), 3);

    assert_eq!(
        from[0].source,
        ValueSource::Input(Id("input.charge_payment.reserved".into()))
    );

    assert_eq!(from[0].path.0, vec!["event_id".to_string()]);

    // The err arm reads the provider's err payload.
    let OperationStep::ExecuteEffect(failed) = &matched.err.steps[0] else {
        panic!("the err arm should publish the failure");
    };

    let Derivation::Deterministic { from } = &failed.values else {
        panic!("failure values should declare deterministic provenance");
    };

    assert_eq!(
        from[2].source,
        ValueSource::EffectResultErr(Id("result.charge_payment.card".into()))
    );

    assert_eq!(from[2].path.0, vec!["reason".to_string()]);

    assert!(matches!(matched.ok.steps[1], OperationStep::Complete));
    assert!(matches!(matched.err.steps[1], OperationStep::Complete));
}

#[test]
fn flash_checkout_parses_request_results_and_return_terminals() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let create_order = model
        .operations
        .get(&Id("operation.create_order".into()))
        .expect("create_order should exist");

    let Some(Input::Request(request)) = create_order
        .inputs
        .get(&Id("input.create_order.request".into()))
    else {
        panic!("create_order input should be a request");
    };

    assert_eq!(request.result.ok, Id("schema.CreateOrderResponse".into()));
    assert_eq!(
        request.result.err.schema,
        Id("schema.RequestRejected".into())
    );

    // The bare-schema shorthand declares nothing about disposition.
    assert_eq!(
        request.result.err.disposition,
        ErrorDisposition::Unspecified
    );
    assert_eq!(
        request.result.schema(ResultVariant::Err),
        &Id("schema.RequestRejected".into())
    );

    let OperationStep::Return(returned) = &create_order.program.steps[2] else {
        panic!("the program should end by returning the request's result");
    };

    assert_eq!(returned.request, Id("input.create_order.request".into()));
    assert_eq!(returned.outcome.variant(), ResultVariant::Ok);

    let ResultOutcome::Ok { values } = &returned.outcome else {
        panic!("create_order returns ok");
    };

    let Derivation::Deterministic { from } = values else {
        panic!("the returned payload declares provenance");
    };

    assert_eq!(
        from[0].source,
        ValueSource::TransactionOutput(Id("output.create_order".into()))
    );

    // The output is established by the transaction; the return only
    // reads it.
    let transaction = create_order
        .transactions
        .get(&Id("tx.create_order.new".into()))
        .expect("create_order transaction should exist");

    assert!(matches!(
        &transaction.steps[2],
        TransactionStep::EstablishTransactionOutput(establish)
            if establish.output == Id("output.create_order".into())
    ));
}

#[test]
fn external_effects_declare_their_result_contract() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let Some(Effect::External(card)) = model
        .operations
        .get(&Id("operation.charge_payment".into()))
        .unwrap()
        .effects
        .get(&Id("effect.charge_payment.card".into()))
    else {
        panic!("card charge should be an external effect");
    };

    let result = card.result.as_ref().expect("the provider returns a result");

    assert_eq!(result.ok, Id("schema.ChargeAccepted".into()));
    assert_eq!(result.err.schema, Id("schema.ChargeDeclined".into()));
    assert_eq!(result.err.disposition, ErrorDisposition::Unspecified);

    // A boundary modeling no synchronous result says so.
    let source = read_fixture("video_streaming.yaml");

    let model = yaml::parse(&source).expect("video streaming fixture should parse");

    let Some(Effect::External(push)) = model
        .operations
        .get(&Id("operation.notify_published".into()))
        .unwrap()
        .effects
        .get(&Id("effect.notify_published.push".into()))
    else {
        panic!("push should be an external effect");
    };

    assert_eq!(push.result, None);

    // A declared disposition parses as part of the contract.
    let Some(Effect::External(engine)) = model
        .operations
        .get(&Id("operation.transcode_video".into()))
        .unwrap()
        .effects
        .get(&Id("effect.transcode_video.engine".into()))
    else {
        panic!("the engine should be an external effect");
    };

    let result = engine.result.as_ref().expect("the engine returns a result");

    assert_eq!(result.err.schema, Id("schema.RenderFailed".into()));
    assert_eq!(result.err.disposition, ErrorDisposition::Terminal);
}

fn parse_error_contract(declaration: &str) -> ErrorResultType {
    serde_yaml::from_str(declaration)
        .unwrap_or_else(|error| panic!("`{declaration}` should parse: {error}"))
}

fn error_contract_error(declaration: &str) -> String {
    serde_yaml::from_str::<ErrorResultType>(declaration)
        .expect_err(&format!("`{declaration}` should not parse"))
        .to_string()
}

#[test]
fn an_error_contract_declares_schema_and_disposition() {
    // Every disposition parses in the canonical map form.
    for (text, disposition) in [
        ("unspecified", ErrorDisposition::Unspecified),
        ("terminal", ErrorDisposition::Terminal),
        ("retryable", ErrorDisposition::Retryable),
    ] {
        let contract = parse_error_contract(&format!(
            "schema: schema.ProviderError\ndisposition: {text}"
        ));

        assert_eq!(contract.schema, Id("schema.ProviderError".into()));
        assert_eq!(contract.disposition, disposition);
    }

    // The bare-schema shorthand declares nothing: `unspecified` is
    // epistemic, and no shorthand may silently strengthen it.
    assert_eq!(
        parse_error_contract("schema.ProviderError"),
        ErrorResultType {
            schema: Id("schema.ProviderError".into()),
            disposition: ErrorDisposition::Unspecified,
        }
    );

    // So does omitting the disposition in the map form.
    assert_eq!(
        parse_error_contract("schema: schema.ProviderError").disposition,
        ErrorDisposition::Unspecified
    );

    // A disposition outside the declared three is rejected.
    let message = error_contract_error(
        "schema: schema.ProviderError\ndisposition: definitely_not_a_disposition",
    );

    assert!(
        message.contains("definitely_not_a_disposition"),
        "error should mention the invalid value, got: {message}"
    );

    // The map form requires the error schema.
    error_contract_error("disposition: terminal");
}

#[test]
fn error_dispositions_serialize_into_the_canonical_form() {
    let source = read_fixture("video_streaming.yaml");

    let model = yaml::parse(&source).expect("video streaming fixture should parse");

    let serialized = yaml::serialize(&model).expect("model should serialize");

    // Canonical serialization always emits the disposition — the
    // declared terminal one, and `unspecified` for every contract the
    // shorthand left undeclared.
    assert!(
        serialized.contains("disposition: terminal"),
        "serialized model should carry the engine's terminal disposition"
    );

    assert!(
        serialized.contains("disposition: unspecified"),
        "serialized model should make undeclared dispositions explicit"
    );

    let reparsed = yaml::parse(&serialized).expect("serialized model should parse");

    assert_eq!(model, reparsed);
}

#[test]
fn a_result_binding_may_be_omitted() {
    let step: OperationStep = serde_yaml::from_str(
        "kind: execute_effect
effect: effect.x
values:
  kind: unspecified",
    )
    .expect("a step without a binding should parse");

    let OperationStep::ExecuteEffect(step) = step else {
        panic!("expected an execute_effect step");
    };

    assert_eq!(step.result, None);

    let step: OperationStep = serde_yaml::from_str(
        "kind: execute_effect_intent
intent: intent.x",
    )
    .expect("an intent execution without a binding should parse");

    assert!(matches!(
        step,
        OperationStep::ExecuteEffectIntent(step) if step.result.is_none()
    ));
}

#[test]
fn a_branch_condition_accepts_the_selector_value_surface() {
    let condition: Condition = serde_yaml::from_str(
        "kind: eq
value:
  source: input:input.checkout
  path: region
equals: CA",
    )
    .expect("a literal comparison should parse");

    let Condition::Eq { value, equals } = &condition else {
        panic!("expected an equality");
    };

    assert_eq!(
        value.source,
        ValueSource::Input(Id("input.checkout".into()))
    );
    assert_eq!(
        equals,
        &SelectorValue::Literal(Literal::String("CA".into()))
    );
    assert!(condition.is_deterministic());
    assert_eq!(condition.roots().len(), 1);

    let condition: Condition = serde_yaml::from_str(
        "kind: not
condition:
  kind: and
  conditions:
    - kind: eq
      value:
        source: input:input.checkout
        path: region
      equals:
        source: transaction_output:output.routing
        path: region
    - kind: unspecified",
    )
    .expect("a nested condition should parse");

    // Both sides of a reference comparison are roots; `unspecified`
    // anywhere makes the whole decision non-deterministic.
    assert_eq!(condition.roots().len(), 2);
    assert!(!condition.is_deterministic());
}

#[test]
fn flash_checkout_parses_transition_effect_values() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let transaction = model
        .operations
        .get(&Id("operation.apply_payment".into()))
        .expect("apply_payment should exist")
        .transactions
        .get(&Id("tx.apply_payment".into()))
        .expect("apply_payment transaction should exist");

    let TransactionStep::Transition(transition) = &transaction.steps[1] else {
        panic!("second step should be the mark_paid transition");
    };

    assert_eq!(transition.effect_values.len(), 1);

    let values = transition
        .effect_values
        .get(&Id("effect.order.paid".into()))
        .expect("the mark_paid side effect should have a derivation");

    let Derivation::Deterministic { from } = values else {
        panic!("transition effect values should declare deterministic provenance");
    };

    // The derivation is evaluated in the transaction context, so it may
    // reference the preceding read.
    assert_eq!(
        from[0].source,
        ValueSource::TransactionRead(Id("read.apply_payment.order".into()))
    );

    assert_eq!(from[0].path.0, vec!["order_id".to_string()]);

    // A transition without side effects declares an explicit empty map.
    let transaction = model
        .operations
        .get(&Id("operation.cancel_order".into()))
        .expect("cancel_order should exist")
        .transactions
        .get(&Id("tx.cancel_order".into()))
        .expect("cancel_order transaction should exist");

    let TransactionStep::Transition(transition) = &transaction.steps[0] else {
        panic!("first step should be the cancel transition");
    };

    assert!(transition.effect_values.is_empty());
}

/// A one-schema model whose sole schema declares `fields`, so field
/// surface syntax can be exercised without a fixture.
fn field_source(fields: &str) -> String {
    let mut source = String::from(
        "revision: 1
services: {}
schemas:
  Subject:
    kind: canonical
    completeness: complete
    fields:
",
    );

    for line in fields.lines() {
        source.push_str("      ");
        source.push_str(line);
        source.push('\n');
    }

    source.push_str(
        "data_models: {}
topics: {}
state_machines: {}
operations: {}
",
    );

    source
}

fn parse_field(declaration: &str) -> Field {
    let source = field_source(declaration);

    let model = yaml::parse(&source)
        .unwrap_or_else(|error| panic!("`{declaration}` should parse: {error}"));

    let Some(Schema::Canonical(subject)) = model.schemas.get(&Id("Subject".into())) else {
        panic!("Subject should be a canonical schema");
    };

    subject
        .fields
        .values()
        .next()
        .cloned()
        .expect("the schema should declare a field")
}

fn field_error(declaration: &str) -> String {
    let source = field_source(declaration);

    yaml::parse(&source)
        .expect_err(&format!("`{declaration}` should not parse"))
        .to_string()
}

#[test]
fn a_shorthand_field_means_what_the_canonical_form_means() {
    let shorthand = parse_field("order_id: uuid");

    let canonical = parse_field(
        "order_id:
  ty:
    kind: scalar
    value: uuid
  optional: false",
    );

    assert_eq!(shorthand, canonical);

    assert_eq!(shorthand.ty, TypeRef::Scalar(ScalarType::Uuid));
    assert!(!shorthand.optional);
}

#[test]
fn a_trailing_question_mark_marks_a_field_optional() {
    let field = parse_field("note: string?");

    assert_eq!(field.ty, TypeRef::Scalar(ScalarType::String));
    assert!(field.optional);
}

#[test]
fn a_shorthand_name_that_is_not_a_scalar_is_a_schema_reference() {
    let field = parse_field("customer: schema.Customer");

    assert_eq!(field.ty, TypeRef::Schema(Id("schema.Customer".into())));
    assert!(!field.optional);
}

#[test]
fn a_schema_named_for_a_scalar_stays_reachable_through_the_canonical_form() {
    // `uuid` reads as the scalar in shorthand, so the canonical form
    // is the escape hatch rather than a special case in the grammar.
    let field = parse_field(
        "subject:
  ty:
    kind: schema
    value: uuid
  optional: false",
    );

    assert_eq!(field.ty, TypeRef::Schema(Id("uuid".into())));
}

#[test]
fn a_one_element_sequence_is_a_list_type() {
    let field = parse_field("tags: [string]");

    assert_eq!(
        field.ty,
        TypeRef::List(Box::new(TypeRef::Scalar(ScalarType::String)))
    );

    assert!(!field.optional);

    // Nesting works because the element is itself a type.
    let field = parse_field("rows: [[string]]");

    assert_eq!(
        field.ty,
        TypeRef::List(Box::new(TypeRef::List(Box::new(TypeRef::Scalar(
            ScalarType::String
        )))))
    );

    // An optional list needs the whole declaration quoted, because the
    // marker belongs to the field rather than to the element type.
    let field = parse_field("tags: \"[string]?\"");

    assert_eq!(
        field.ty,
        TypeRef::List(Box::new(TypeRef::Scalar(ScalarType::String)))
    );

    assert!(field.optional);
}

#[test]
fn shorthand_types_are_accepted_inside_the_canonical_form() {
    let field = parse_field(
        "note:
  ty: string
  optional: true",
    );

    assert_eq!(field.ty, TypeRef::Scalar(ScalarType::String));
    assert!(field.optional);

    let field = parse_field(
        "tags:
  ty: [schema.Tag]
  optional: true",
    );

    assert_eq!(
        field.ty,
        TypeRef::List(Box::new(TypeRef::Schema(Id("schema.Tag".into()))))
    );

    assert!(field.optional);
}

#[test]
fn a_type_may_not_carry_the_optional_marker() {
    // Optionality is a claim about the field, not about the type, so
    // the marker is rejected wherever a type alone is expected.
    let message = field_error(
        "note:
  ty: string?
  optional: true",
    );

    assert!(
        message.contains("`?` marks a field optional"),
        "error should explain where the marker belongs, got: {message}"
    );

    let message = field_error("tags: [string?]");

    assert!(
        message.contains("`?` marks a field optional"),
        "error should reject an optional element type, got: {message}"
    );
}

#[test]
fn a_list_shorthand_holds_exactly_one_element_type() {
    let message = field_error("tags: []");

    assert!(
        message.contains("exactly one element type"),
        "error should reject an empty list shorthand, got: {message}"
    );

    let message = field_error("tags: [string, int]");

    assert!(
        message.contains("exactly one element type"),
        "error should reject a multi-element list shorthand, got: {message}"
    );
}

#[test]
fn an_unterminated_list_shorthand_is_rejected() {
    let message = field_error("tags: \"[string\"");

    assert!(
        message.contains("unterminated"),
        "error should name the unterminated bracket, got: {message}"
    );
}

#[test]
fn a_canonical_schema_may_omit_its_description() {
    let source = field_source("order_id: uuid");

    let model = yaml::parse(&source).expect("a schema without a description should parse");

    let Some(Schema::Canonical(subject)) = model.schemas.get(&Id("Subject".into())) else {
        panic!("Subject should be a canonical schema");
    };

    assert_eq!(subject.description, None);
}

#[test]
fn shorthand_fields_serialize_into_the_canonical_form() {
    let source = field_source("note: string?");

    let model = yaml::parse(&source).expect("shorthand model should parse");

    let serialized = yaml::serialize(&model).expect("model should serialize");

    // Serialization is the wire format tooling reads, so it stays
    // explicit even when the source was written in shorthand.
    assert!(
        serialized.contains("kind: scalar"),
        "serialized model should carry the tagged type, got:\n{serialized}"
    );

    let reparsed = yaml::parse(&serialized).expect("serialized model should parse");

    assert_eq!(model, reparsed);
}

fn parse_path(declaration: &str) -> FieldPath {
    serde_yaml::from_str(declaration)
        .unwrap_or_else(|error| panic!("`{declaration}` should parse: {error}"))
}

fn path_error(declaration: &str) -> String {
    serde_yaml::from_str::<FieldPath>(declaration)
        .expect_err(&format!("`{declaration}` should not parse"))
        .to_string()
}

fn parse_source(declaration: &str) -> ValueSource {
    serde_yaml::from_str(declaration)
        .unwrap_or_else(|error| panic!("`{declaration}` should parse: {error}"))
}

fn source_error(declaration: &str) -> String {
    serde_yaml::from_str::<ValueSource>(declaration)
        .expect_err(&format!("`{declaration}` should not parse"))
        .to_string()
}

#[test]
fn a_dotted_path_means_what_the_component_sequence_means() {
    assert_eq!(parse_path("customer.id"), parse_path("[customer, id]"));

    assert_eq!(
        parse_path("customer.id").0,
        vec!["customer".to_string(), "id".to_string()]
    );

    assert_eq!(parse_path("order_id").0, vec!["order_id".to_string()]);
}

#[test]
fn a_dotted_path_has_no_empty_components() {
    for declaration in ["customer..id", ".id", "customer.", "\"\""] {
        let message = path_error(declaration);

        assert!(
            message.contains("field path"),
            "error should name the offending path, got: {message}"
        );
    }
}

#[test]
fn a_path_naming_nothing_still_reaches_validation() {
    // Whether a path resolves is validation's question, so an empty
    // sequence parses here and fails there, exactly as before.
    assert_eq!(parse_path("[]").0, Vec::<String>::new());
}

#[test]
fn a_value_source_shorthand_means_what_the_tagged_map_means() {
    let shorthand = parse_source("input:input.create_order.request");

    let canonical = parse_source(
        "kind: input
id: input.create_order.request",
    );

    assert_eq!(shorthand, canonical);

    assert_eq!(
        shorthand,
        ValueSource::Input(Id("input.create_order.request".into()))
    );
}

#[test]
fn every_value_source_kind_has_a_shorthand() {
    assert_eq!(parse_source("input:x"), ValueSource::Input(Id("x".into())));

    assert_eq!(
        parse_source("effect:x"),
        ValueSource::Effect(Id("x".into()))
    );

    assert_eq!(
        parse_source("transaction_output:x"),
        ValueSource::TransactionOutput(Id("x".into()))
    );

    assert_eq!(
        parse_source("state_machine_subject:x"),
        ValueSource::StateMachineSubject(Id("x".into()))
    );

    assert_eq!(
        parse_source("transaction_read:x"),
        ValueSource::TransactionRead(Id("x".into()))
    );

    assert_eq!(
        parse_source("effect_result_ok:x"),
        ValueSource::EffectResultOk(Id("x".into()))
    );

    assert_eq!(
        parse_source("effect_result_err:x"),
        ValueSource::EffectResultErr(Id("x".into()))
    );

    // The retired kind is refused rather than read as anything else.
    let message = source_error("invocation_result:x");

    assert!(
        message.contains("is not a value source kind"),
        "error should reject the retired kind, got: {message}"
    );
}

#[test]
fn a_value_source_always_names_its_kind() {
    // The kind is never inferred from the id: the seven variants index
    // six namespaces (the two result-payload kinds share the binding
    // namespace), and an id may be declared in more than one.
    let message = source_error("input.create_order.request");

    assert!(
        message.contains("names its kind"),
        "error should ask for the kind, got: {message}"
    );

    let message = source_error("topic:topic.order_events");

    assert!(
        message.contains("is not a value source kind"),
        "error should reject an unknown kind, got: {message}"
    );

    let message = source_error("\"input:\"");

    assert!(
        message.contains("expected an id"),
        "error should ask for the id, got: {message}"
    );
}

#[test]
fn shorthand_paths_and_sources_serialize_into_the_canonical_form() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let serialized = yaml::serialize(&model).expect("model should serialize");

    // Tooling reads the serialized model, so it keeps the component
    // sequence and the tagged source.
    assert!(
        serialized.contains("kind: input"),
        "serialized model should carry tagged value sources"
    );

    assert!(
        !serialized.contains("path: idempotency_key"),
        "serialized model should carry path components as a sequence, not a dotted name"
    );

    assert!(
        !serialized.contains("source: input:"),
        "serialized model should carry the tagged source, not the shorthand"
    );

    let reparsed = yaml::parse(&serialized).expect("serialized model should parse");

    assert_eq!(model, reparsed);
}

fn parse_selector_value(declaration: &str) -> SelectorValue {
    serde_yaml::from_str(declaration)
        .unwrap_or_else(|error| panic!("`{declaration}` should parse: {error}"))
}

fn selector_value_error(declaration: &str) -> String {
    serde_yaml::from_str::<SelectorValue>(declaration)
        .expect_err(&format!("`{declaration}` should not parse"))
        .to_string()
}

#[test]
fn a_selector_value_map_is_a_value_reference() {
    let shorthand = parse_selector_value(
        "source: input:input.create_order.request
path: idempotency_key",
    );

    let canonical = parse_selector_value(
        "kind: value
value:
  source:
    kind: input
    id: input.create_order.request
  path:
    - idempotency_key",
    );

    assert_eq!(shorthand, canonical);

    let SelectorValue::Value(reference) = shorthand else {
        panic!("a map with `source` should be a value reference");
    };

    assert_eq!(
        reference.source,
        ValueSource::Input(Id("input.create_order.request".into()))
    );

    assert_eq!(reference.path.0, vec!["idempotency_key".to_string()]);
}

#[test]
fn a_selector_value_scalar_is_a_literal() {
    assert_eq!(
        parse_selector_value("pending"),
        SelectorValue::Literal(Literal::String("pending".into()))
    );

    assert_eq!(
        parse_selector_value("true"),
        SelectorValue::Literal(Literal::Bool(true))
    );

    assert_eq!(
        parse_selector_value("3"),
        SelectorValue::Literal(Literal::Int(3))
    );

    // A string that YAML would read as another type is quoted, as it
    // is anywhere else in the document.
    assert_eq!(
        parse_selector_value("\"true\""),
        SelectorValue::Literal(Literal::String("true".into()))
    );
}

#[test]
fn a_selector_value_naming_a_value_source_kind_is_rejected() {
    // A reference that lost its path would otherwise read as the
    // string it spells, turning a provenance-bearing comparison into a
    // comparison with a constant.
    let message = selector_value_error("input:input.create_order.request");

    assert!(
        message.contains("reads as a string literal"),
        "error should refuse the ambiguous literal, got: {message}"
    );

    // The canonical form still declares such a string deliberately.
    assert_eq!(
        parse_selector_value(
            "kind: literal
value:
  kind: string
  value: input:input.create_order.request"
        ),
        SelectorValue::Literal(Literal::String("input:input.create_order.request".into()))
    );
}

#[test]
fn selector_value_keys_may_come_in_either_order() {
    assert_eq!(
        parse_selector_value(
            "path: idempotency_key
source: input:input.create_order.request"
        ),
        parse_selector_value(
            "source: input:input.create_order.request
path: idempotency_key"
        )
    );

    assert_eq!(
        parse_selector_value(
            "value: pending
kind: literal"
        ),
        SelectorValue::Literal(Literal::String("pending".into()))
    );
}

#[test]
fn an_unknown_selector_value_key_is_rejected() {
    let message = selector_value_error("reference: input:input.create_order.request");

    assert!(
        message.contains("unknown field `reference`"),
        "error should name the unknown key, got: {message}"
    );
}

#[test]
fn a_literal_shorthand_works_inside_the_canonical_wrapper() {
    assert_eq!(
        parse_selector_value(
            "kind: literal
value: pending"
        ),
        SelectorValue::Literal(Literal::String("pending".into()))
    );
}

#[test]
fn shorthand_selector_values_serialize_into_the_canonical_form() {
    let source = read_fixture("flash_checkout.yaml");

    let model = yaml::parse(&source).expect("flash checkout fixture should parse");

    let serialized = yaml::serialize(&model).expect("model should serialize");

    assert!(
        serialized.contains("kind: value"),
        "serialized model should carry the selector value tag"
    );

    let reparsed = yaml::parse(&serialized).expect("serialized model should parse");

    assert_eq!(model, reparsed);
}
