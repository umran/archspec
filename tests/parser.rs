use std::{
    fs,
    path::{Path, PathBuf},
};

use archspec::{
    parser::yaml,
    spec::{Id, Schema, SchemaCompleteness, ServiceKind, TopicOrdering},
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
