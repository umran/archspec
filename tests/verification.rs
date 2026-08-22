use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use archspec::{
    analyzer::{
        DiagnosticCode, Severity, VerificationCode, validation,
        verification::{
            self, ArtifactReplay, ConsumerCollapse, EffectRetrySafety, EffectSafety,
            GoverningKeyDefect, IdempotencyObstacle, IdempotencyProof, IdempotencyVerdict,
            KeyIdentity, PayloadIdentityGap, RecoverabilityNote, RecoverabilityObstacle,
            RecoverabilityProof, RecoverabilityVerdict, ReplayGap, Resolution,
            ResponseReplayObstacle, ResponseReplayProof, ResponseReplayVerdict, RetryDriver,
            SerializationObstacle, SerializationProof, SerializationVerdict, StabilityGap,
            StabilityRule, StableRoot, TransactionResolution, canonical_value_path,
        },
    },
    parser::yaml,
    spec::{
        CompletionRequirement, Derivation, DispatchRouting, EstablishInvocationResult, FieldPath,
        FlowStep, Id, IdempotencyGuarantee, IdempotencyKey, IdempotencyRequirement, Input,
        InvocationFlow, InvocationResult, LaneConcurrency, MessageIdentity, MessageSelector,
        Model, ObjectSelector, OperationConcurrency, RecoverabilityRequirement, RequestIdentity,
        ResponseReplayRequirement, ResponseSource, Schema, SchemaFragment, SelectorPredicate,
        SelectorValue, SerializationRequirement, SubscriptionInput, TopicOrdering, Transaction,
        TransactionIsolation, TransactionStep, ValueRef, ValueSource, Write,
    },
};

fn id(value: &str) -> Id {
    Id(value.to_owned())
}

fn path(components: &[&str]) -> FieldPath {
    FieldPath(components.iter().map(|c| (*c).to_owned()).collect())
}

fn input_key(input: &str, components: &[&str]) -> ValueRef {
    ValueRef {
        source: ValueSource::Input(id(input)),
        path: path(components),
    }
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

fn subscription_mut<'a>(model: &'a mut Model, operation: &str, input: &str) -> &'a mut SubscriptionInput {
    let input = model
        .operations
        .get_mut(&id(operation))
        .unwrap()
        .inputs
        .get_mut(&id(input))
        .unwrap();

    match input {
        Input::Subscription(subscription) => subscription,
        Input::Request(_) => panic!("`{input:?}` is not a subscription"),
    }
}

fn serialization_verdict(model: &Model, operation: &str, requirement: usize) -> SerializationVerdict {
    let report = verification::verify(model);

    report
        .serialization
        .iter()
        .find(|check| check.operation == id(operation) && check.requirement == requirement)
        .unwrap_or_else(|| panic!("no serialization check for `{operation}` #{requirement}"))
        .verdict
        .clone()
}

fn obstacles(verdict: &SerializationVerdict) -> &[SerializationObstacle] {
    match verdict {
        SerializationVerdict::Unproven { obstacles } => obstacles,
        SerializationVerdict::Proven { proof } => {
            panic!("expected an unproven verdict, found proof {proof:?}")
        }
    }
}

#[test]
fn flash_checkout_serialization_requirements_are_proven() {
    let model = load_flash_checkout();

    let report = verification::verify(&model);

    // reserve_inventory, charge_payment, and apply_payment each
    // declare one serialization requirement.
    assert_eq!(report.serialization.len(), 3);

    assert!(
        report
            .serialization
            .iter()
            .all(|check| matches!(check.verdict, SerializationVerdict::Proven { .. })),
        "expected every serialization requirement proven:\n{report:#?}"
    );

    // The fixture's honest gaps: reserve_inventory's recoverability,
    // reserve_inventory's idempotency, charge_payment's idempotency
    // (the not_deduplicated card charge), and create_order's
    // idempotency through its cascade into reserve_inventory.
    assert!(!report.all_proven());
    assert_eq!(report.diagnostics().len(), 4);
}

#[test]
fn keyed_lane_proof_states_its_facts() {
    let model = load_flash_checkout();

    let verdict = serialization_verdict(&model, "operation.reserve_inventory", 0);

    let SerializationVerdict::Proven {
        proof:
            SerializationProof::KeyedLaneSerial {
                input,
                topic,
                message_keys,
            },
    } = verdict
    else {
        panic!("expected a keyed-lane proof, found {verdict:?}");
    };

    assert_eq!(input, id("input.reserve_inventory.created"));
    assert_eq!(topic, id("topic.order_events"));

    // The subscription admits only OrderCreated, and the topic keys it
    // by the same field the requirement uses.
    assert_eq!(message_keys.len(), 1);
    assert_eq!(message_keys[0].schema, id("schema.OrderCreated"));
    assert_eq!(message_keys[0].topic_key, path(&["order_id"]));
    assert_eq!(message_keys[0].identity, KeyIdentity::SamePath);
}

#[test]
fn operation_concurrency_of_one_proves_any_key() {
    let mut model = load_flash_checkout();

    let operation = model.operations.get_mut(&id("operation.transfer_stock")).unwrap();

    operation
        .requirements
        .serialization
        .push(SerializationRequirement {
            key: input_key("input.transfer_stock.request", &["sku"]),
        });

    operation.execution.concurrency = OperationConcurrency::Bounded(NonZeroU32::new(1).unwrap());

    assert!(validation::validate(&model).is_empty());

    let verdict = serialization_verdict(&model, "operation.transfer_stock", 0);

    assert_eq!(
        verdict,
        SerializationVerdict::Proven {
            proof: SerializationProof::OperationSerial,
        }
    );
}

#[test]
fn request_input_key_without_global_bound_is_unproven() {
    let mut model = load_flash_checkout();

    model
        .operations
        .get_mut(&id("operation.transfer_stock"))
        .unwrap()
        .requirements
        .serialization
        .push(SerializationRequirement {
            key: input_key("input.transfer_stock.request", &["sku"]),
        });

    assert!(validation::validate(&model).is_empty());

    let verdict = serialization_verdict(&model, "operation.transfer_stock", 0);

    assert_eq!(
        obstacles(&verdict),
        &[
            SerializationObstacle::OperationConcurrencyNotSerial {
                declared: OperationConcurrency::Unbounded,
            },
            SerializationObstacle::RequestInputHasNoDispatchFacts {
                input: id("input.transfer_stock.request"),
            },
        ]
    );

    let report = verification::verify(&model);

    // The added serialization gap, plus the fixture's four standing
    // gaps.
    assert_eq!(report.diagnostics().len(), 5);
}

#[test]
fn single_lane_with_lane_bound_of_one_proves_the_subscription() {
    let mut model = load_flash_checkout();

    subscription_mut(
        &mut model,
        "operation.reserve_inventory",
        "input.reserve_inventory.created",
    )
    .dispatch
    .routing = DispatchRouting::SingleLane;

    assert!(validation::validate(&model).is_empty());

    let verdict = serialization_verdict(&model, "operation.reserve_inventory", 0);

    assert_eq!(
        verdict,
        SerializationVerdict::Proven {
            proof: SerializationProof::SubscriptionSerial {
                input: id("input.reserve_inventory.created"),
            },
        }
    );
}

#[test]
fn lane_concurrency_above_one_defeats_the_lane_routes() {
    let mut model = load_flash_checkout();

    subscription_mut(
        &mut model,
        "operation.reserve_inventory",
        "input.reserve_inventory.created",
    )
    .dispatch
    .lane_concurrency = LaneConcurrency::Bounded(NonZeroU32::new(2).unwrap());

    let verdict = serialization_verdict(&model, "operation.reserve_inventory", 0);

    // The affinity leg holds, so the only missing facts are the two
    // concurrency bounds.
    assert_eq!(
        obstacles(&verdict),
        &[
            SerializationObstacle::OperationConcurrencyNotSerial {
                declared: OperationConcurrency::Unbounded,
            },
            SerializationObstacle::LaneConcurrencyNotSerial {
                input: id("input.reserve_inventory.created"),
                declared: LaneConcurrency::Bounded(NonZeroU32::new(2).unwrap()),
            },
        ]
    );
}

#[test]
fn unconstrained_routing_defeats_the_lane_routes() {
    let mut model = load_flash_checkout();

    subscription_mut(
        &mut model,
        "operation.reserve_inventory",
        "input.reserve_inventory.created",
    )
    .dispatch
    .routing = DispatchRouting::Unconstrained;

    let verdict = serialization_verdict(&model, "operation.reserve_inventory", 0);

    // The lane bound of one is still declared; only routing is
    // missing.
    assert_eq!(
        obstacles(&verdict),
        &[
            SerializationObstacle::OperationConcurrencyNotSerial {
                declared: OperationConcurrency::Unbounded,
            },
            SerializationObstacle::RoutingProvidesNoAffinity {
                input: id("input.reserve_inventory.created"),
                declared: DispatchRouting::Unconstrained,
            },
        ]
    );
}

#[test]
fn serialization_key_diverging_from_topic_key_is_unproven() {
    let mut model = load_flash_checkout();

    model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap()
        .requirements
        .serialization
        .push(SerializationRequirement {
            key: input_key("input.reserve_inventory.created", &["warehouse_id"]),
        });

    assert!(validation::validate(&model).is_empty());

    // Requirement 0 (order_id) is still proven.
    assert!(matches!(
        serialization_verdict(&model, "operation.reserve_inventory", 0),
        SerializationVerdict::Proven { .. }
    ));

    // Requirement 1 (warehouse_id) does not coincide with the topic
    // key domain.
    let verdict = serialization_verdict(&model, "operation.reserve_inventory", 1);

    assert_eq!(
        obstacles(&verdict),
        &[
            SerializationObstacle::OperationConcurrencyNotSerial {
                declared: OperationConcurrency::Unbounded,
            },
            SerializationObstacle::KeyIdentityUnestablished {
                input: id("input.reserve_inventory.created"),
                topic: id("topic.order_events"),
                schema: id("schema.OrderCreated"),
                topic_key: path(&["order_id"]),
            },
        ]
    );
}

#[test]
fn topic_without_keyed_ordering_defeats_by_topic_key_routing() {
    let mut model = load_flash_checkout();

    model.topics.get_mut(&id("topic.order_events")).unwrap().ordering = TopicOrdering::Global;

    let verdict = serialization_verdict(&model, "operation.reserve_inventory", 0);

    assert_eq!(
        obstacles(&verdict),
        &[
            SerializationObstacle::OperationConcurrencyNotSerial {
                declared: OperationConcurrency::Unbounded,
            },
            SerializationObstacle::TopicNotKeyed {
                input: id("input.reserve_inventory.created"),
                topic: id("topic.order_events"),
            },
        ]
    );
}

#[test]
fn key_not_sourced_from_an_input_admits_only_the_global_route() {
    let mut model = load_flash_checkout();

    model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .requirements
        .serialization = vec![SerializationRequirement {
        key: ValueRef {
            source: ValueSource::StateMachineSubject(id("machine.order_lifecycle")),
            path: path(&["order_id"]),
        },
    }];

    assert!(validation::validate(&model).is_empty());

    let verdict = serialization_verdict(&model, "operation.apply_payment", 0);

    assert_eq!(
        obstacles(&verdict),
        &[
            SerializationObstacle::OperationConcurrencyNotSerial {
                declared: OperationConcurrency::Unbounded,
            },
            SerializationObstacle::KeyNotFromInput {
                source: ValueSource::StateMachineSubject(id("machine.order_lifecycle")),
            },
        ]
    );
}

#[test]
fn empty_message_selection_is_vacuously_proven() {
    let mut model = load_flash_checkout();

    subscription_mut(
        &mut model,
        "operation.reserve_inventory",
        "input.reserve_inventory.created",
    )
    .messages = MessageSelector::Only(BTreeSet::new());

    let verdict = serialization_verdict(&model, "operation.reserve_inventory", 0);

    assert_eq!(
        verdict,
        SerializationVerdict::Proven {
            proof: SerializationProof::NoAdmittedInvocations {
                input: id("input.reserve_inventory.created"),
            },
        }
    );
}

#[test]
fn fragment_aliasing_establishes_key_identity() {
    let mut model = load_flash_checkout();

    // A fragment view of OrderCreated in which `ref` aliases
    // `order_id` under a different name.
    let mut mapping = BTreeMap::new();

    for field in ["order_id", "event_id", "warehouse_id", "sku", "quantity", "amount"] {
        mapping.insert(field.to_owned(), path(&[field]));
    }

    mapping.insert("ref".to_owned(), path(&["order_id"]));

    model.schemas.insert(
        id("schema.OrderCreatedView"),
        Schema::Fragment(SchemaFragment {
            source: id("schema.OrderCreated"),
            mapping,
        }),
    );

    let topic = model.topics.get_mut(&id("topic.order_events")).unwrap();

    topic.messages.insert(id("schema.OrderCreatedView"));

    let TopicOrdering::Keyed(key) = &mut topic.ordering else {
        panic!("fixture topic is keyed");
    };

    // The topic keys the fragment schema by its aliased name.
    key.mapping.insert(id("schema.OrderCreatedView"), path(&["ref"]));

    let subscription = subscription_mut(
        &mut model,
        "operation.reserve_inventory",
        "input.reserve_inventory.created",
    );

    subscription.messages = MessageSelector::Only(BTreeSet::from([id("schema.OrderCreatedView")]));

    assert!(
        validation::validate(&model).is_empty(),
        "the fragment mutation should keep the model valid"
    );

    // The requirement key is `order_id`; the topic keys the admitted
    // schema by `ref`. Both expand to OrderCreated.order_id.
    let verdict = serialization_verdict(&model, "operation.reserve_inventory", 0);

    let SerializationVerdict::Proven {
        proof: SerializationProof::KeyedLaneSerial { message_keys, .. },
    } = verdict
    else {
        panic!("expected a keyed-lane proof, found {verdict:?}");
    };

    assert_eq!(message_keys.len(), 1);
    assert_eq!(message_keys[0].schema, id("schema.OrderCreatedView"));
    assert_eq!(message_keys[0].topic_key, path(&["ref"]));
    assert_eq!(
        message_keys[0].identity,
        KeyIdentity::SameCanonicalValue {
            schema: id("schema.OrderCreated"),
            path: path(&["order_id"]),
        }
    );
}

#[test]
fn missing_topic_key_mapping_is_an_obstacle_not_a_panic() {
    let mut model = load_flash_checkout();

    let topic = model.topics.get_mut(&id("topic.order_events")).unwrap();

    let TopicOrdering::Keyed(key) = &mut topic.ordering else {
        panic!("fixture topic is keyed");
    };

    key.mapping.remove(&id("schema.OrderCreated"));

    // The model no longer validates; verification must stay total and
    // conservative.
    assert!(!validation::validate(&model).is_empty());

    let verdict = serialization_verdict(&model, "operation.reserve_inventory", 0);

    assert_eq!(
        obstacles(&verdict),
        &[
            SerializationObstacle::OperationConcurrencyNotSerial {
                declared: OperationConcurrency::Unbounded,
            },
            SerializationObstacle::TopicKeyMappingMissing {
                input: id("input.reserve_inventory.created"),
                topic: id("topic.order_events"),
                schema: id("schema.OrderCreated"),
            },
        ]
    );
}

#[test]
fn dangling_key_input_is_unproven_not_a_panic() {
    let mut model = load_flash_checkout();

    model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap()
        .requirements
        .serialization = vec![SerializationRequirement {
        key: input_key("input.missing", &["order_id"]),
    }];

    let verdict = serialization_verdict(&model, "operation.reserve_inventory", 0);

    assert_eq!(
        obstacles(&verdict),
        &[
            SerializationObstacle::OperationConcurrencyNotSerial {
                declared: OperationConcurrency::Unbounded,
            },
            SerializationObstacle::KeyNotFromInput {
                source: ValueSource::Input(id("input.missing")),
            },
        ]
    );
}

#[test]
fn canonical_value_path_expands_fragment_chains() {
    let mut model = load_flash_checkout();

    model.schemas.insert(
        id("schema.ViewA"),
        Schema::Fragment(SchemaFragment {
            source: id("schema.OrderCreated"),
            mapping: BTreeMap::from([("a".to_owned(), path(&["order_id"]))]),
        }),
    );

    model.schemas.insert(
        id("schema.ViewB"),
        Schema::Fragment(SchemaFragment {
            source: id("schema.ViewA"),
            mapping: BTreeMap::from([("b".to_owned(), path(&["a"]))]),
        }),
    );

    let canonical = canonical_value_path(&model, &id("schema.ViewB"), &path(&["b"])).unwrap();

    assert_eq!(canonical.schema, id("schema.OrderCreated"));
    assert_eq!(canonical.path, path(&["order_id"]));

    // Unresolvable paths resolve to nothing rather than panicking.
    assert!(canonical_value_path(&model, &id("schema.ViewB"), &path(&["missing"])).is_none());
    assert!(canonical_value_path(&model, &id("schema.missing"), &path(&["b"])).is_none());
}

#[test]
fn verification_report_round_trips_through_json() {
    let mut model = load_flash_checkout();

    // Make one requirement unproven so both verdict shapes serialize.
    model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap()
        .requirements
        .serialization
        .push(SerializationRequirement {
            key: input_key("input.reserve_inventory.created", &["warehouse_id"]),
        });

    let report = verification::verify(&model);

    let json = serde_json::to_string(&report).expect("report should serialize");

    let restored: verification::VerificationReport =
        serde_json::from_str(&json).expect("report should deserialize");

    assert_eq!(report, restored);
}

fn ikey(input: &str, components: &[&[&str]]) -> IdempotencyKey {
    IdempotencyKey {
        components: components
            .iter()
            .map(|component| input_key(input, component))
            .collect(),
    }
}

fn deterministic(from: Vec<ValueRef>) -> Derivation {
    Derivation::Deterministic { from }
}

fn response_replay_verdict(
    model: &Model,
    operation: &str,
    requirement: usize,
) -> ResponseReplayVerdict {
    let report = verification::verify(model);

    report
        .response_replay
        .iter()
        .find(|check| check.operation == id(operation) && check.requirement == requirement)
        .unwrap_or_else(|| panic!("no response-replay check for `{operation}` #{requirement}"))
        .verdict
        .clone()
}

fn create_order_transaction(model: &mut Model) -> &mut Transaction {
    model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .transactions
        .get_mut(&id("tx.create_order.new"))
        .unwrap()
}

/// Replaces create_order's transaction body with a naturally
/// replayable shape: a key-derived write and a result establishment,
/// with no keyed commit.
fn make_create_order_natural(model: &mut Model) {
    let transaction = create_order_transaction(model);

    transaction.idempotency = IdempotencyGuarantee::Unspecified;

    transaction.steps = vec![
        TransactionStep::Write(Write {
            target: ObjectSelector {
                object: id("object.order"),
                predicate: SelectorPredicate::Eq {
                    field: path(&["order_id"]),
                    value: SelectorValue::Value(input_key(
                        "input.create_order.request",
                        &["order_id"],
                    )),
                },
            },
            fields: BTreeSet::from([path(&["amount"])]),
            values: deterministic(vec![
                input_key("input.create_order.request", &["order_id"]),
                input_key("input.create_order.request", &["amount"]),
            ]),
        }),
        TransactionStep::EstablishInvocationResult(EstablishInvocationResult {
            result: id("result.create_order"),
            values: deterministic(vec![input_key("input.create_order.request", &["order_id"])]),
        }),
    ];

    // The intent is no longer established, so the flow must not
    // execute it.
    model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .flows
        .get_mut(&id("flow.create_order.new"))
        .unwrap()
        .steps = vec![FlowStep::Transaction {
        transaction: id("tx.create_order.new"),
    }];
}

#[test]
fn flash_checkout_response_replay_is_proven_by_recovery() {
    let model = load_flash_checkout();

    let report = verification::verify(&model);

    // Only create_order declares `response: replay_consistent`.
    assert_eq!(report.response_replay.len(), 1);

    let check = &report.response_replay[0];

    assert_eq!(check.operation, id("operation.create_order"));
    assert_eq!(check.requirement, 0);

    let ResponseReplayVerdict::Proven {
        proof:
            ResponseReplayProof::ClassFixedResult {
                result,
                transaction,
                replay,
                flows,
            },
    } = &check.verdict
    else {
        panic!("expected a class-fixed result, found {:?}", check.verdict);
    };

    assert_eq!(result, &id("result.create_order"));
    assert_eq!(transaction, &id("tx.create_order.new"));
    assert_eq!(flows, &vec![id("flow.create_order.new")]);

    // Route B: the commit key is the governing key itself.
    let ArtifactReplay::Recovered { key, .. } = replay else {
        panic!("expected recovery, found {replay:?}");
    };

    assert_eq!(
        key,
        &vec![StableRoot {
            root: input_key("input.create_order.request", &["idempotency_key"]),
            rule: StabilityRule::KeyComponent,
        }]
    );
}

#[test]
fn natural_reconstruction_proves_response_replay() {
    let mut model = load_flash_checkout();

    make_create_order_natural(&mut model);

    assert!(validation::validate(&model).is_empty());

    let verdict = response_replay_verdict(&model, "operation.create_order", 0);

    let ResponseReplayVerdict::Proven {
        proof: ResponseReplayProof::ClassFixedResult { replay, .. },
    } = &verdict
    else {
        panic!("expected a class-fixed result, found {verdict:?}");
    };

    // Route A: every root is covered by the request identity pinned by
    // the governing key.
    let ArtifactReplay::Reconstructed { transaction, derivation } = replay else {
        panic!("expected reconstruction, found {replay:?}");
    };

    assert_eq!(transaction, &id("tx.create_order.new"));

    assert_eq!(
        derivation,
        &vec![StableRoot {
            root: input_key("input.create_order.request", &["order_id"]),
            rule: StabilityRule::IdentifiedPayload,
        }]
    );
}

#[test]
fn poison_retry_defeats_natural_reconstruction() {
    let mut model = load_flash_checkout();

    make_create_order_natural(&mut model);

    // Without the boundary identity, same-key attempts may present
    // different payloads.
    let Some(Input::Request(request)) = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .inputs
        .get_mut(&id("input.create_order.request"))
    else {
        panic!("create_order input should be a request");
    };

    request.identity = RequestIdentity::Unspecified;

    assert!(validation::validate(&model).is_empty());

    let verdict = response_replay_verdict(&model, "operation.create_order", 0);

    let ResponseReplayVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected an unproven verdict, found {verdict:?}");
    };

    assert_eq!(obstacles.len(), 1);

    let ResponseReplayObstacle::ResultNotReplayAvailable {
        recovery,
        reconstruction,
        ..
    } = &obstacles[0]
    else {
        panic!("expected an unavailable result, found {:?}", obstacles[0]);
    };

    assert_eq!(recovery, &vec![ReplayGap::NoKeyedCommit]);

    assert!(
        reconstruction.contains(&ReplayGap::MutationDerivationRootUnstable {
            root: input_key("input.create_order.request", &["amount"]),
            gap: StabilityGap::UnidentifiedPayloadField {
                input: id("input.create_order.request"),
                identity: PayloadIdentityGap::NotDeclared,
            },
        }),
        "expected the poison-retry gap on `amount`:\n{reconstruction:#?}"
    );
}

#[test]
fn unstable_commit_key_defeats_recovery() {
    let mut model = load_flash_checkout();

    create_order_transaction(&mut model).idempotency = IdempotencyGuarantee::DeduplicatedBy {
        key: ikey("input.create_order.request", &[&["amount"]]),
    };

    let Some(Input::Request(request)) = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .inputs
        .get_mut(&id("input.create_order.request"))
    else {
        panic!("create_order input should be a request");
    };

    request.identity = RequestIdentity::Unspecified;

    assert!(validation::validate(&model).is_empty());

    let verdict = response_replay_verdict(&model, "operation.create_order", 0);

    let ResponseReplayVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected an unproven verdict, found {verdict:?}");
    };

    let ResponseReplayObstacle::ResultNotReplayAvailable {
        recovery,
        reconstruction,
        ..
    } = &obstacles[0]
    else {
        panic!("expected an unavailable result, found {:?}", obstacles[0]);
    };

    // The keyed commit exists, but its key is an unidentified non-key
    // field, so attempts may address different commits.
    assert_eq!(
        recovery,
        &vec![ReplayGap::CommitKeyRootUnstable {
            root: input_key("input.create_order.request", &["amount"]),
            gap: StabilityGap::UnidentifiedPayloadField {
                input: id("input.create_order.request"),
                identity: PayloadIdentityGap::NotDeclared,
            },
        }]
    );

    assert!(reconstruction.contains(&ReplayGap::ContainsInsert));
}

#[test]
fn identified_payload_stabilizes_commit_key() {
    let mut model = load_flash_checkout();

    // Same non-key commit key, but the declared request identity is
    // pinned by the governing key, so `amount` is class-fixed.
    create_order_transaction(&mut model).idempotency = IdempotencyGuarantee::DeduplicatedBy {
        key: ikey("input.create_order.request", &[&["amount"]]),
    };

    assert!(validation::validate(&model).is_empty());

    let verdict = response_replay_verdict(&model, "operation.create_order", 0);

    let ResponseReplayVerdict::Proven {
        proof: ResponseReplayProof::ClassFixedResult { replay, .. },
    } = &verdict
    else {
        panic!("expected a class-fixed result, found {verdict:?}");
    };

    let ArtifactReplay::Recovered { key, .. } = replay else {
        panic!("expected recovery, found {replay:?}");
    };

    assert_eq!(
        key,
        &vec![StableRoot {
            root: input_key("input.create_order.request", &["amount"]),
            rule: StabilityRule::IdentifiedPayload,
        }]
    );
}

#[test]
fn chained_artifact_recovery_is_class_fixed() {
    let mut model = load_flash_checkout();

    // A second keyed transaction whose commit key is a field of the
    // first transaction's recovered result.
    let operation = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap();

    operation.invocation_results.insert(
        id("result.create_order.receipt"),
        InvocationResult {
            schema: id("schema.CreateOrderResponse"),
        },
    );

    operation.transactions.insert(
        id("tx.create_order.receipt"),
        Transaction {
            data_model: None,
            isolation: TransactionIsolation::Unspecified,
            idempotency: IdempotencyGuarantee::DeduplicatedBy {
                key: IdempotencyKey {
                    components: vec![ValueRef {
                        source: ValueSource::InvocationResult(id("result.create_order")),
                        path: path(&["order_id"]),
                    }],
                },
            },
            steps: vec![TransactionStep::EstablishInvocationResult(
                EstablishInvocationResult {
                    result: id("result.create_order.receipt"),
                    values: deterministic(vec![input_key(
                        "input.create_order.request",
                        &["order_id"],
                    )]),
                },
            )],
        },
    );

    let flow = operation.flows.get_mut(&id("flow.create_order.new")).unwrap();

    flow.steps.insert(
        1,
        FlowStep::Transaction {
            transaction: id("tx.create_order.receipt"),
        },
    );

    operation
        .responses
        .get_mut(&id("response.create_order"))
        .unwrap()
        .source = ResponseSource::InvocationResult {
        result: id("result.create_order.receipt"),
    };

    assert!(validation::validate(&model).is_empty());

    let verdict = response_replay_verdict(&model, "operation.create_order", 0);

    let ResponseReplayVerdict::Proven {
        proof:
            ResponseReplayProof::ClassFixedResult {
                result,
                transaction,
                replay,
                ..
            },
    } = &verdict
    else {
        panic!("expected a class-fixed result, found {verdict:?}");
    };

    assert_eq!(result, &id("result.create_order.receipt"));
    assert_eq!(transaction, &id("tx.create_order.receipt"));

    // The second commit key is stable because the first artifact is
    // recovered: stability chains through the artifact context.
    let ArtifactReplay::Recovered { key, .. } = replay else {
        panic!("expected recovery, found {replay:?}");
    };

    assert_eq!(key.len(), 1);

    assert_eq!(
        key[0].rule,
        StabilityRule::RecoveredArtifact {
            transaction: id("tx.create_order.new"),
        }
    );
}

#[test]
fn subscription_key_without_response_is_vacuous() {
    let mut model = load_flash_checkout();

    model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap()
        .requirements
        .idempotency[0]
        .response = ResponseReplayRequirement::ReplayConsistent;

    assert!(validation::validate(&model).is_empty());

    let verdict = response_replay_verdict(&model, "operation.reserve_inventory", 0);

    assert_eq!(
        verdict,
        ResponseReplayVerdict::Proven {
            proof: ResponseReplayProof::NoResolvedResponse {
                input: id("input.reserve_inventory.created"),
            },
        }
    );
}

#[test]
fn governing_key_mixing_sources_is_inadmissible() {
    let mut model = load_flash_checkout();

    let requirement = &mut model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .requirements
        .idempotency[0];

    requirement.key.components.push(ValueRef {
        source: ValueSource::StateMachineSubject(id("machine.order_lifecycle")),
        path: path(&["order_id"]),
    });

    assert!(validation::validate(&model).is_empty());

    let verdict = response_replay_verdict(&model, "operation.create_order", 0);

    assert_eq!(
        verdict,
        ResponseReplayVerdict::Unproven {
            obstacles: vec![ResponseReplayObstacle::GoverningKeyInadmissible {
                defect: GoverningKeyDefect::ComponentNotFromInput {
                    source: ValueSource::StateMachineSubject(id("machine.order_lifecycle")),
                },
            }],
        }
    );
}

#[test]
fn read_dependent_result_is_not_reconstructible() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.transfer_stock"))
        .unwrap();

    operation.invocation_results.insert(
        id("result.transfer_stock"),
        InvocationResult {
            schema: id("schema.TransferStockResponse"),
        },
    );

    operation
        .responses
        .get_mut(&id("response.transfer_stock"))
        .unwrap()
        .source = ResponseSource::InvocationResult {
        result: id("result.transfer_stock"),
    };

    operation
        .transactions
        .get_mut(&id("tx.transfer_stock"))
        .unwrap()
        .steps
        .push(TransactionStep::EstablishInvocationResult(
            EstablishInvocationResult {
                result: id("result.transfer_stock"),
                values: deterministic(vec![ValueRef {
                    source: ValueSource::TransactionRead(id("read.transfer_stock.source_stock")),
                    path: path(&["on_hand"]),
                }]),
            },
        ));

    operation
        .requirements
        .idempotency
        .push(IdempotencyRequirement {
            key: ikey("input.transfer_stock.request", &[&["sku"]]),
            response: ResponseReplayRequirement::ReplayConsistent,
        });

    assert!(validation::validate(&model).is_empty());

    let verdict = response_replay_verdict(&model, "operation.transfer_stock", 0);

    let ResponseReplayVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected an unproven verdict, found {verdict:?}");
    };

    let ResponseReplayObstacle::ResultNotReplayAvailable {
        recovery,
        reconstruction,
        ..
    } = &obstacles[0]
    else {
        panic!("expected an unavailable result, found {:?}", obstacles[0]);
    };

    assert_eq!(recovery, &vec![ReplayGap::NoKeyedCommit]);

    // The artifact derives from a transaction read, which is never
    // replay-stable.
    assert!(
        reconstruction.contains(&ReplayGap::ArtifactDerivationRootUnstable {
            root: ValueRef {
                source: ValueSource::TransactionRead(id("read.transfer_stock.source_stock")),
                path: path(&["on_hand"]),
            },
            gap: StabilityGap::TransactionReadRoot {
                read: id("read.transfer_stock.source_stock"),
            },
        }),
        "expected the transaction-read gap:\n{reconstruction:#?}"
    );

    // The destination write declares no provenance at all.
    assert!(reconstruction.contains(&ReplayGap::MutationDerivationUnspecified));
}

#[test]
fn result_never_established_is_an_obstacle() {
    let mut model = load_flash_checkout();

    create_order_transaction(&mut model)
        .steps
        .retain(|step| !matches!(step, TransactionStep::EstablishInvocationResult(_)));

    assert!(validation::validate(&model).is_empty());

    let verdict = response_replay_verdict(&model, "operation.create_order", 0);

    assert_eq!(
        verdict,
        ResponseReplayVerdict::Unproven {
            obstacles: vec![ResponseReplayObstacle::ResultNotEstablished {
                flow: id("flow.create_order.new"),
                response: id("response.create_order"),
                result: id("result.create_order"),
            }],
        }
    );
}

#[test]
fn divergent_flows_are_not_class_fixed() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap();

    // A second flow establishing the same result through a different
    // keyed transaction.
    operation.transactions.insert(
        id("tx.create_order.replayed"),
        Transaction {
            data_model: None,
            isolation: TransactionIsolation::Unspecified,
            idempotency: IdempotencyGuarantee::DeduplicatedBy {
                key: ikey("input.create_order.request", &[&["idempotency_key"]]),
            },
            steps: vec![TransactionStep::EstablishInvocationResult(
                EstablishInvocationResult {
                    result: id("result.create_order"),
                    values: deterministic(vec![input_key(
                        "input.create_order.request",
                        &["order_id"],
                    )]),
                },
            )],
        },
    );

    operation.flows.insert(
        id("flow.create_order.replayed"),
        InvocationFlow {
            steps: vec![FlowStep::Transaction {
                transaction: id("tx.create_order.replayed"),
            }],
            response: Some(id("response.create_order")),
        },
    );

    assert!(validation::validate(&model).is_empty());

    let verdict = response_replay_verdict(&model, "operation.create_order", 0);

    let ResponseReplayVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected an unproven verdict, found {verdict:?}");
    };

    assert_eq!(obstacles.len(), 1);

    let ResponseReplayObstacle::DivergentResponseSites { sites } = &obstacles[0] else {
        panic!("expected divergent sites, found {:?}", obstacles[0]);
    };

    assert_eq!(sites.len(), 2);
    assert_eq!(sites[0].transaction, id("tx.create_order.new"));
    assert_eq!(sites[1].transaction, id("tx.create_order.replayed"));
}

fn recoverability_verdict(
    model: &Model,
    operation: &str,
    requirement: usize,
) -> RecoverabilityVerdict {
    let report = verification::verify(model);

    report
        .recoverability
        .iter()
        .find(|check| check.operation == id(operation) && check.requirement == requirement)
        .unwrap_or_else(|| panic!("no recoverability check for `{operation}` #{requirement}"))
        .verdict
        .clone()
}

#[test]
fn flash_checkout_recoverability_verdicts() {
    let model = load_flash_checkout();

    let report = verification::verify(&model);

    // create_order (resumable), reserve_inventory (guaranteed), and
    // apply_payment (guaranteed).
    assert_eq!(report.recoverability.len(), 3);

    // create_order resumes through its keyed commit; the intent and
    // result are recovered, and the response resolves the result.
    let verdict = recoverability_verdict(&model, "operation.create_order", 0);

    let RecoverabilityVerdict::Proven {
        proof: RecoverabilityProof::Resumable { flows },
    } = &verdict
    else {
        panic!("expected create_order resumable, found {verdict:?}");
    };

    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0].flow, id("flow.create_order.new"));

    assert!(matches!(
        &flows[0].transactions[..],
        [TransactionResolution {
            resolution: Resolution::KeyedCommit { .. },
            ..
        }]
    ));

    // The consumed intent and the response's result, both recovered.
    assert_eq!(flows[0].artifacts.len(), 2);

    assert!(
        flows[0]
            .artifacts
            .iter()
            .all(|artifact| matches!(artifact.replay, ArtifactReplay::Recovered { .. }))
    );

    // apply_payment is guaranteed: keyed commit, recovered transition
    // intent, and at-least-once delivery as the driver.
    let verdict = recoverability_verdict(&model, "operation.apply_payment", 0);

    let RecoverabilityVerdict::Proven {
        proof: RecoverabilityProof::Guaranteed { driver, .. },
    } = &verdict
    else {
        panic!("expected apply_payment guaranteed, found {verdict:?}");
    };

    assert_eq!(
        driver,
        &RetryDriver::AtLeastOnceDelivery {
            input: id("input.apply_payment.captured"),
            topic: id("topic.order_events"),
        }
    );

    // reserve_inventory has the driver but no continuation: the
    // committed reservation resolves by neither route, and the
    // publication intent is replay-available by neither route.
    let verdict = recoverability_verdict(&model, "operation.reserve_inventory", 0);

    let RecoverabilityVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected reserve_inventory unproven, found {verdict:?}");
    };

    assert_eq!(obstacles.len(), 2);

    let RecoverabilityObstacle::TransactionNotResolvable {
        transaction,
        recovery,
        reconstruction,
        ..
    } = &obstacles[0]
    else {
        panic!("expected an unresolvable transaction, found {:?}", obstacles[0]);
    };

    assert_eq!(transaction, &id("tx.reserve_inventory"));
    assert_eq!(recovery, &vec![ReplayGap::NoKeyedCommit]);

    assert!(
        reconstruction
            .iter()
            .any(|gap| matches!(gap, ReplayGap::MutationDerivationRootUnstable {
                gap: StabilityGap::TransactionReadRoot { .. },
                ..
            })),
        "expected the read-dependent write gap:\n{reconstruction:#?}"
    );

    assert!(matches!(
        &obstacles[1],
        RecoverabilityObstacle::ArtifactNotReplayAvailable { artifact, .. }
            if artifact == &id("intent.reserve_inventory.publish_reserved")
    ));
}

#[test]
fn final_transaction_of_a_response_less_flow_needs_no_replay_route() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.transfer_stock"))
        .unwrap();

    // The transaction is unreplayable in every way, but nothing
    // follows it once the response is removed.
    operation
        .flows
        .get_mut(&id("flow.transfer_stock"))
        .unwrap()
        .response = None;

    operation
        .requirements
        .recoverability
        .push(RecoverabilityRequirement {
            key: ikey("input.transfer_stock.request", &[&["sku"]]),
            completion: CompletionRequirement::Resumable,
        });

    assert!(validation::validate(&model).is_empty());

    let verdict = recoverability_verdict(&model, "operation.transfer_stock", 0);

    let RecoverabilityVerdict::Proven {
        proof: RecoverabilityProof::Resumable { flows },
    } = &verdict
    else {
        panic!("expected resumable, found {verdict:?}");
    };

    assert_eq!(
        flows[0].transactions,
        vec![TransactionResolution {
            transaction: id("tx.transfer_stock"),
            resolution: Resolution::TerminalStep,
        }]
    );
}

#[test]
fn response_after_the_final_transaction_requires_resolution() {
    let mut model = load_flash_checkout();

    // Same flow, but the declared response follows the transaction,
    // so a failing prefix exists after its commit.
    model
        .operations
        .get_mut(&id("operation.transfer_stock"))
        .unwrap()
        .requirements
        .recoverability
        .push(RecoverabilityRequirement {
            key: ikey("input.transfer_stock.request", &[&["sku"]]),
            completion: CompletionRequirement::Resumable,
        });

    assert!(validation::validate(&model).is_empty());

    let verdict = recoverability_verdict(&model, "operation.transfer_stock", 0);

    let RecoverabilityVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected an unproven verdict, found {verdict:?}");
    };

    assert_eq!(obstacles.len(), 1);

    assert!(matches!(
        &obstacles[0],
        RecoverabilityObstacle::TransactionNotResolvable { transaction, .. }
            if transaction == &id("tx.transfer_stock")
    ));
}

#[test]
fn guaranteed_completion_needs_a_modeled_driver() {
    let mut model = load_flash_checkout();

    subscription_mut(
        &mut model,
        "operation.apply_payment",
        "input.apply_payment.captured",
    )
    .delivery = archspec::spec::DeliverySemantics::AtMostOnce;

    assert!(validation::validate(&model).is_empty());

    let verdict = recoverability_verdict(&model, "operation.apply_payment", 0);

    assert_eq!(
        verdict,
        RecoverabilityVerdict::Unproven {
            obstacles: vec![RecoverabilityObstacle::NoModeledRetryDriver {
                input: id("input.apply_payment.captured"),
                delivery: Some(archspec::spec::DeliverySemantics::AtMostOnce),
            }],
        }
    );
}

#[test]
fn inbound_repeatable_request_supplies_the_driver() {
    let mut model = load_flash_checkout();

    // Guaranteed completion on a request-driven operation: no driver
    // until a modeled caller declares a repeatable request effect.
    model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .requirements
        .recoverability[0]
        .completion = CompletionRequirement::Guaranteed;

    assert!(validation::validate(&model).is_empty());

    let verdict = recoverability_verdict(&model, "operation.create_order", 0);

    assert_eq!(
        verdict,
        RecoverabilityVerdict::Unproven {
            obstacles: vec![RecoverabilityObstacle::NoModeledRetryDriver {
                input: id("input.create_order.request"),
                delivery: None,
            }],
        }
    );

    model
        .operations
        .get_mut(&id("operation.transfer_stock"))
        .unwrap()
        .effects
        .insert(
            id("effect.transfer_stock.create_order"),
            archspec::spec::Effect::Request(archspec::spec::RequestEffect {
                target: archspec::spec::RequestTarget {
                    operation: id("operation.create_order"),
                    input: id("input.create_order.request"),
                },
                schema: id("schema.CreateOrderRequest"),
                retry: archspec::spec::RetrySemantics::MayRepeat,
                idempotency_key_propagation: vec![],
            }),
        );

    assert!(validation::validate(&model).is_empty());

    let verdict = recoverability_verdict(&model, "operation.create_order", 0);

    let RecoverabilityVerdict::Proven {
        proof: RecoverabilityProof::Guaranteed { driver, .. },
    } = &verdict
    else {
        panic!("expected guaranteed, found {verdict:?}");
    };

    assert_eq!(
        driver,
        &RetryDriver::InboundRepeatableRequest {
            operation: id("operation.transfer_stock"),
            effect: id("effect.transfer_stock.create_order"),
        }
    );
}

#[test]
fn executed_intent_must_be_established_by_the_flow() {
    let mut model = load_flash_checkout();

    create_order_transaction(&mut model)
        .steps
        .retain(|step| !matches!(step, TransactionStep::EstablishEffectIntent(_)));

    assert!(validation::validate(&model).is_empty());

    let verdict = recoverability_verdict(&model, "operation.create_order", 0);

    let RecoverabilityVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected an unproven verdict, found {verdict:?}");
    };

    assert_eq!(
        obstacles,
        &vec![RecoverabilityObstacle::ArtifactNotEstablished {
            flow: id("flow.create_order.new"),
            artifact: id("intent.create_order.publish_created"),
            consumer: id("intent.create_order.publish_created"),
        }]
    );
}

#[test]
fn unavailable_consumed_artifacts_block_resumption() {
    let mut model = load_flash_checkout();

    // Natural-shape transaction with no keyed commit and no declared
    // request identity: the result is neither recoverable nor
    // reconstructible, and the transaction itself cannot resolve.
    make_create_order_natural(&mut model);

    let Some(Input::Request(request)) = model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .inputs
        .get_mut(&id("input.create_order.request"))
    else {
        panic!("create_order input should be a request");
    };

    request.identity = RequestIdentity::Unspecified;

    assert!(validation::validate(&model).is_empty());

    let verdict = recoverability_verdict(&model, "operation.create_order", 0);

    let RecoverabilityVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected an unproven verdict, found {verdict:?}");
    };

    assert!(obstacles.iter().any(|obstacle| matches!(
        obstacle,
        RecoverabilityObstacle::TransactionNotResolvable { transaction, .. }
            if transaction == &id("tx.create_order.new")
    )));

    assert!(obstacles.iter().any(|obstacle| matches!(
        obstacle,
        RecoverabilityObstacle::ArtifactNotReplayAvailable { artifact, .. }
            if artifact == &id("result.create_order")
    )));
}

#[test]
fn empty_subscription_population_is_vacuously_recoverable() {
    let mut model = load_flash_checkout();

    subscription_mut(
        &mut model,
        "operation.reserve_inventory",
        "input.reserve_inventory.created",
    )
    .messages = MessageSelector::Only(BTreeSet::new());

    let verdict = recoverability_verdict(&model, "operation.reserve_inventory", 0);

    assert_eq!(
        verdict,
        RecoverabilityVerdict::Proven {
            proof: RecoverabilityProof::NoAdmittedInvocations {
                input: id("input.reserve_inventory.created"),
            },
        }
    );
}

#[test]
fn recoverability_key_mixing_sources_is_inadmissible() {
    let mut model = load_flash_checkout();

    model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .requirements
        .recoverability[0]
        .key
        .components
        .push(ValueRef {
            source: ValueSource::StateMachineSubject(id("machine.order_lifecycle")),
            path: path(&["order_id"]),
        });

    assert!(validation::validate(&model).is_empty());

    let verdict = recoverability_verdict(&model, "operation.apply_payment", 0);

    assert_eq!(
        verdict,
        RecoverabilityVerdict::Unproven {
            obstacles: vec![RecoverabilityObstacle::GoverningKeyInadmissible {
                defect: GoverningKeyDefect::ComponentNotFromInput {
                    source: ValueSource::StateMachineSubject(id("machine.order_lifecycle")),
                },
            }],
        }
    );
}

fn order_events_message_identity(
    model: &mut Model,
) -> &mut BTreeMap<Id, Vec<FieldPath>> {
    let topic = model.topics.get_mut(&id("topic.order_events")).unwrap();

    let MessageIdentity::Keyed { mapping } = &mut topic.message_identity else {
        panic!("order_events should declare a keyed message identity");
    };

    mapping
}

fn idempotency_verdict(model: &Model, operation: &str, requirement: usize) -> IdempotencyVerdict {
    let report = verification::verify(model);

    report
        .idempotency
        .iter()
        .find(|check| check.operation == id(operation) && check.requirement == requirement)
        .unwrap_or_else(|| panic!("no idempotency check for `{operation}` #{requirement}"))
        .verdict
        .clone()
}

#[test]
fn flash_checkout_idempotency_verdicts() {
    let model = load_flash_checkout();

    let report = verification::verify(&model);

    assert_eq!(report.idempotency.len(), 4);

    // create_order: keyed commit plus a recovered publication intent
    // whose schema the topic identifies — but its cascade does not
    // collapse: reserve_inventory consumes OrderCreated, and its own
    // requirement is unproven, so a retried create_order is not
    // established to avoid duplicate work downstream.
    let verdict = idempotency_verdict(&model, "operation.create_order", 0);

    let IdempotencyVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected create_order unproven, found {verdict:?}");
    };

    assert!(
        matches!(
            &obstacles[..],
            [IdempotencyObstacle::PublicationConsumerRequirementUnproven {
                schema,
                operation,
                input,
                ..
            }] if schema == &id("schema.OrderCreated")
                && operation == &id("operation.reserve_inventory")
                && input == &id("input.reserve_inventory.created")
        ),
        "expected only the cascade obstacle for create_order:\n{obstacles:#?}"
    );

    // apply_payment: the transition transaction and its implicitly
    // established intent, both through the keyed commit.
    assert!(matches!(
        idempotency_verdict(&model, "operation.apply_payment", 0),
        IdempotencyVerdict::Proven { .. }
    ));

    // charge_payment: the capture publication is safe, but the card
    // charge is explicitly not deduplicated — the model admits
    // charging the card twice, and that is the only obstacle.
    let verdict = idempotency_verdict(&model, "operation.charge_payment", 0);

    let IdempotencyVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected charge_payment unproven, found {verdict:?}");
    };

    assert_eq!(
        obstacles,
        &vec![IdempotencyObstacle::ExternalEffectNotDeduplicated {
            flow: id("flow.charge_payment"),
            effect: id("effect.charge_payment.card"),
        }]
    );

    // reserve_inventory: the reservation transaction is retry-unsafe
    // and the intent it establishes is unavailable.
    let verdict = idempotency_verdict(&model, "operation.reserve_inventory", 0);

    let IdempotencyVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected reserve_inventory unproven, found {verdict:?}");
    };

    assert!(obstacles.iter().any(|obstacle| matches!(
        obstacle,
        IdempotencyObstacle::TransactionNotRetrySafe { transaction, .. }
            if transaction == &id("tx.reserve_inventory")
    )));

    assert!(obstacles.iter().any(|obstacle| matches!(
        obstacle,
        IdempotencyObstacle::IntentNotReplayAvailable { intent, .. }
            if intent == &id("intent.reserve_inventory.publish_reserved")
    )));

    // Its cascade reaches charge_payment, whose card charge is not
    // deduplicated.
    assert!(obstacles.iter().any(|obstacle| matches!(
        obstacle,
        IdempotencyObstacle::PublicationConsumerRequirementUnproven { operation, .. }
            if operation == &id("operation.charge_payment")
    )));
}

#[test]
fn declared_external_deduplication_completes_the_charge_proof() {
    let mut model = load_flash_checkout();

    // Declare the payment provider's own idempotency: the card charge
    // deduplicates by the propagated event id.
    let Some(archspec::spec::Effect::External(card)) = model
        .operations
        .get_mut(&id("operation.charge_payment"))
        .unwrap()
        .effects
        .get_mut(&id("effect.charge_payment.card"))
    else {
        panic!("card charge should be an external effect");
    };

    card.idempotency = IdempotencyGuarantee::DeduplicatedBy {
        key: ikey("input.charge_payment.reserved", &[&["event_id"]]),
    };

    assert!(validation::validate(&model).is_empty());

    let verdict = idempotency_verdict(&model, "operation.charge_payment", 0);

    let IdempotencyVerdict::Proven {
        proof: IdempotencyProof::RetrySafeFlows { flows },
    } = &verdict
    else {
        panic!("expected charge_payment proven, found {verdict:?}");
    };

    assert!(matches!(
        &flows[0].effects[0],
        EffectRetrySafety {
            safety: EffectSafety::ExternallyDeduplicated { key },
            ..
        } if matches!(key[0].rule, StabilityRule::KeyComponent)
    ));
}

#[test]
fn unstable_external_deduplication_key_is_an_obstacle() {
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

    card.idempotency = IdempotencyGuarantee::DeduplicatedBy {
        key: IdempotencyKey {
            components: vec![ValueRef {
                source: ValueSource::StateMachineSubject(id("machine.order_lifecycle")),
                path: path(&["order_id"]),
            }],
        },
    };

    assert!(validation::validate(&model).is_empty());

    let verdict = idempotency_verdict(&model, "operation.charge_payment", 0);

    let IdempotencyVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected charge_payment unproven, found {verdict:?}");
    };

    assert!(matches!(
        &obstacles[..],
        [IdempotencyObstacle::ExternalDeduplicationKeyUnstable { roots, .. }]
            if matches!(roots[0].gap, StabilityGap::MutableSubjectState { .. })
    ));
}

#[test]
fn unidentified_publication_defeats_the_duplicate_discharge() {
    let mut model = load_flash_checkout();

    // The topic no longer identifies PaymentCaptured messages; the
    // partial mapping remains valid, but the capture publication loses
    // its same-logical-message argument.
    order_events_message_identity(&mut model).remove(&id("schema.PaymentCaptured"));

    assert!(validation::validate(&model).is_empty());

    let verdict = idempotency_verdict(&model, "operation.charge_payment", 0);

    let IdempotencyVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected charge_payment unproven, found {verdict:?}");
    };

    assert!(obstacles.iter().any(|obstacle| matches!(
        obstacle,
        IdempotencyObstacle::PublicationNotIdentified { schema, .. }
            if schema == &id("schema.PaymentCaptured")
    )));
}

#[test]
fn single_delivery_discharges_idempotency_vacuously() {
    let mut model = load_flash_checkout();

    subscription_mut(
        &mut model,
        "operation.reserve_inventory",
        "input.reserve_inventory.created",
    )
    .delivery = archspec::spec::DeliverySemantics::AtMostOnce;

    assert!(validation::validate(&model).is_empty());

    let verdict = idempotency_verdict(&model, "operation.reserve_inventory", 0);

    assert_eq!(
        verdict,
        IdempotencyVerdict::Proven {
            proof: IdempotencyProof::SingleDelivery {
                input: id("input.reserve_inventory.created"),
                topic: id("topic.order_events"),
            },
        }
    );
}

#[test]
fn request_discharge_needs_a_proven_target_through_the_fixpoint() {
    let mut model = load_flash_checkout();

    // create_order's own requirement proves only when its cascade
    // collapses: deliver OrderCreated to reserve_inventory at most
    // once, so the one logical message reaches it once.
    subscription_mut(
        &mut model,
        "operation.reserve_inventory",
        "input.reserve_inventory.created",
    )
    .delivery = archspec::spec::DeliverySemantics::AtMostOnce;

    // transfer_stock forwards a class-fixed request into create_order,
    // whose own idempotency requirement is proven; the fixpoint's
    // rounds discharge the cascade and then the request leg.
    let operation = model
        .operations
        .get_mut(&id("operation.transfer_stock"))
        .unwrap();

    operation.effects.insert(
        id("effect.transfer_stock.forward"),
        archspec::spec::Effect::Request(archspec::spec::RequestEffect {
            target: archspec::spec::RequestTarget {
                operation: id("operation.create_order"),
                input: id("input.create_order.request"),
            },
            schema: id("schema.CreateOrderRequest"),
            retry: archspec::spec::RetrySemantics::MayRepeat,
            idempotency_key_propagation: vec![],
        }),
    );

    operation.flows.get_mut(&id("flow.transfer_stock")).unwrap().steps =
        vec![FlowStep::ExecuteEffect {
            effect: id("effect.transfer_stock.forward"),
            values: deterministic(vec![input_key("input.transfer_stock.request", &["sku"])]),
        }];

    operation
        .requirements
        .idempotency
        .push(IdempotencyRequirement {
            key: ikey("input.transfer_stock.request", &[&["sku"]]),
            response: ResponseReplayRequirement::Unspecified,
        });

    assert!(validation::validate(&model).is_empty());

    let verdict = idempotency_verdict(&model, "operation.transfer_stock", 0);

    let IdempotencyVerdict::Proven {
        proof: IdempotencyProof::RetrySafeFlows { flows },
    } = &verdict
    else {
        panic!("expected transfer_stock proven, found {verdict:?}");
    };

    assert!(matches!(
        &flows[0].effects[..],
        [EffectRetrySafety {
            safety: EffectSafety::DeduplicatedByTarget { operation, input, .. },
            ..
        }] if operation == &id("operation.create_order")
            && input == &id("input.create_order.request")
    ));

    // Without the target's requirement, nothing collapses duplicate
    // invocations.
    model
        .operations
        .get_mut(&id("operation.create_order"))
        .unwrap()
        .requirements
        .idempotency
        .clear();

    let verdict = idempotency_verdict(&model, "operation.transfer_stock", 0);

    let IdempotencyVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected transfer_stock unproven, found {verdict:?}");
    };

    assert!(matches!(
        &obstacles[..],
        [IdempotencyObstacle::RequestTargetHasNoKeyedRequirement { operation, .. }]
            if operation == &id("operation.create_order")
    ));
}

#[test]
fn cyclic_request_dependencies_settle_unproven() {
    let mut model = load_flash_checkout();

    // transfer_stock and cancel_order request each other; the least
    // fixpoint leaves both request legs unproven.
    let transfer = model
        .operations
        .get_mut(&id("operation.transfer_stock"))
        .unwrap();

    transfer.effects.insert(
        id("effect.transfer_stock.cancel"),
        archspec::spec::Effect::Request(archspec::spec::RequestEffect {
            target: archspec::spec::RequestTarget {
                operation: id("operation.cancel_order"),
                input: id("input.cancel_order.request"),
            },
            schema: id("schema.CancelOrderRequest"),
            retry: archspec::spec::RetrySemantics::Unspecified,
            idempotency_key_propagation: vec![],
        }),
    );

    transfer.flows.get_mut(&id("flow.transfer_stock")).unwrap().steps =
        vec![FlowStep::ExecuteEffect {
            effect: id("effect.transfer_stock.cancel"),
            values: deterministic(vec![input_key("input.transfer_stock.request", &["sku"])]),
        }];

    transfer
        .requirements
        .idempotency
        .push(IdempotencyRequirement {
            key: ikey("input.transfer_stock.request", &[&["sku"]]),
            response: ResponseReplayRequirement::Unspecified,
        });

    let cancel = model
        .operations
        .get_mut(&id("operation.cancel_order"))
        .unwrap();

    cancel.effects.insert(
        id("effect.cancel_order.transfer"),
        archspec::spec::Effect::Request(archspec::spec::RequestEffect {
            target: archspec::spec::RequestTarget {
                operation: id("operation.transfer_stock"),
                input: id("input.transfer_stock.request"),
            },
            schema: id("schema.TransferStockRequest"),
            retry: archspec::spec::RetrySemantics::Unspecified,
            idempotency_key_propagation: vec![],
        }),
    );

    cancel.flows.get_mut(&id("flow.cancel_order")).unwrap().steps =
        vec![FlowStep::ExecuteEffect {
            effect: id("effect.cancel_order.transfer"),
            values: deterministic(vec![input_key("input.cancel_order.request", &["order_id"])]),
        }];

    cancel
        .requirements
        .idempotency
        .push(IdempotencyRequirement {
            key: ikey("input.cancel_order.request", &[&["order_id"]]),
            response: ResponseReplayRequirement::Unspecified,
        });

    assert!(validation::validate(&model).is_empty());

    for operation in ["operation.transfer_stock", "operation.cancel_order"] {
        let verdict = idempotency_verdict(&model, operation, 0);

        let IdempotencyVerdict::Unproven { obstacles } = &verdict else {
            panic!("expected `{operation}` unproven, found {verdict:?}");
        };

        assert!(
            matches!(
                &obstacles[..],
                [IdempotencyObstacle::RequestTargetRequirementUnproven { .. }]
            ),
            "expected the cyclic request obstacle for `{operation}`:\n{obstacles:#?}"
        );
    }
}

#[test]
fn publication_cascade_needs_collapsing_consumers_through_the_fixpoint() {
    let mut model = load_flash_checkout();

    // With the card charge deduplicated, charge_payment's remaining
    // leg is its cascade: PaymentCaptured reaches apply_payment, whose
    // own requirement the fixpoint proves in its first round, so the
    // second round discharges the publication.
    let Some(archspec::spec::Effect::External(card)) = model
        .operations
        .get_mut(&id("operation.charge_payment"))
        .unwrap()
        .effects
        .get_mut(&id("effect.charge_payment.card"))
    else {
        panic!("card charge should be an external effect");
    };

    card.idempotency = IdempotencyGuarantee::DeduplicatedBy {
        key: ikey("input.charge_payment.reserved", &[&["event_id"]]),
    };

    assert!(validation::validate(&model).is_empty());

    let verdict = idempotency_verdict(&model, "operation.charge_payment", 0);

    let IdempotencyVerdict::Proven {
        proof: IdempotencyProof::RetrySafeFlows { flows },
    } = &verdict
    else {
        panic!("expected charge_payment proven, found {verdict:?}");
    };

    assert!(matches!(
        &flows[0].effects[..],
        [
            EffectRetrySafety {
                safety: EffectSafety::ExternallyDeduplicated { .. },
                ..
            },
            EffectRetrySafety {
                safety: EffectSafety::SameLogicalMessage {
                    schema,
                    consumers,
                    ..
                },
                ..
            },
        ] if schema == &id("schema.PaymentCaptured")
            && consumers
                == &vec![ConsumerCollapse::ProvenRequirement {
                    operation: id("operation.apply_payment"),
                    input: id("input.apply_payment.captured"),
                }]
    ));

    // Without the consumer's requirement, nothing collapses the
    // duplicate deliveries a duplicate publication causes there.
    model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .requirements
        .idempotency
        .clear();

    let verdict = idempotency_verdict(&model, "operation.charge_payment", 0);

    let IdempotencyVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected charge_payment unproven, found {verdict:?}");
    };

    assert!(
        matches!(
            &obstacles[..],
            [IdempotencyObstacle::PublicationConsumerNotKeyed {
                schema,
                operation,
                input,
                ..
            }] if schema == &id("schema.PaymentCaptured")
                && operation == &id("operation.apply_payment")
                && input == &id("input.apply_payment.captured")
        ),
        "expected the unkeyed-consumer obstacle:\n{obstacles:#?}"
    );
}

#[test]
fn at_most_once_consumers_collapse_duplicates_by_delivery() {
    let mut model = load_flash_checkout();

    // An at-most-once consumer never sees a second delivery of the one
    // logical message, so it needs no requirement of its own.
    subscription_mut(
        &mut model,
        "operation.reserve_inventory",
        "input.reserve_inventory.created",
    )
    .delivery = archspec::spec::DeliverySemantics::AtMostOnce;

    model
        .operations
        .get_mut(&id("operation.reserve_inventory"))
        .unwrap()
        .requirements
        .idempotency
        .clear();

    assert!(validation::validate(&model).is_empty());

    let verdict = idempotency_verdict(&model, "operation.create_order", 0);

    let IdempotencyVerdict::Proven {
        proof: IdempotencyProof::RetrySafeFlows { flows },
    } = &verdict
    else {
        panic!("expected create_order proven, found {verdict:?}");
    };

    assert!(matches!(
        &flows[0].effects[..],
        [EffectRetrySafety {
            safety: EffectSafety::SameLogicalMessage { consumers, .. },
            ..
        }] if consumers
            == &vec![ConsumerCollapse::SingleDelivery {
                operation: id("operation.reserve_inventory"),
                input: id("input.reserve_inventory.created"),
            }]
    ));
}

#[test]
fn cyclic_publication_dependencies_settle_unproven() {
    let mut model = load_flash_checkout();

    // apply_payment also consumes the OrderPaid it publishes, so its
    // cascade collapses only if its own requirement is proven; the
    // least fixpoint leaves it unproven.
    subscription_mut(
        &mut model,
        "operation.apply_payment",
        "input.apply_payment.captured",
    )
    .messages = MessageSelector::Only(
        [id("schema.PaymentCaptured"), id("schema.OrderPaid")]
            .into_iter()
            .collect(),
    );

    assert!(validation::validate(&model).is_empty());

    let verdict = idempotency_verdict(&model, "operation.apply_payment", 0);

    let IdempotencyVerdict::Unproven { obstacles } = &verdict else {
        panic!("expected apply_payment unproven, found {verdict:?}");
    };

    assert!(
        matches!(
            &obstacles[..],
            [IdempotencyObstacle::PublicationConsumerRequirementUnproven {
                schema,
                operation,
                ..
            }] if schema == &id("schema.OrderPaid")
                && operation == &id("operation.apply_payment")
        ),
        "expected the cyclic cascade obstacle:\n{obstacles:#?}"
    );
}

#[test]
fn guaranteed_completion_without_keyed_idempotency_is_noted() {
    let mut model = load_flash_checkout();

    let apply_payment = |model: &Model| {
        verification::verify(model)
            .recoverability
            .into_iter()
            .find(|check| check.operation == id("operation.apply_payment"))
            .expect("apply_payment declares recoverability")
    };

    // apply_payment guarantees completion through at-least-once
    // redelivery and declares those retries safe with an idempotency
    // requirement keyed from the same input: nothing to note.
    let check = apply_payment(&model);

    assert!(matches!(
        check.verdict,
        RecoverabilityVerdict::Proven {
            proof: RecoverabilityProof::Guaranteed { .. }
        }
    ));

    assert!(check.notes.is_empty());

    // Without that requirement the proof stands — recoverability is
    // progress only — but the expected retries have undeclared safety.
    model
        .operations
        .get_mut(&id("operation.apply_payment"))
        .unwrap()
        .requirements
        .idempotency
        .clear();

    assert!(validation::validate(&model).is_empty());

    let check = apply_payment(&model);

    assert!(matches!(check.verdict, RecoverabilityVerdict::Proven { .. }));

    assert_eq!(
        check.notes,
        vec![RecoverabilityNote::RetrySafetyUndeclared {
            input: id("input.apply_payment.captured"),
        }]
    );

    let diagnostics = verification::verify(&model).diagnostics();

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Warning
                && diagnostic.code
                    == DiagnosticCode::Verification(
                        VerificationCode::RecoverabilityRetrySafetyUndeclared,
                    )
                && diagnostic.subject == Some(id("operation.apply_payment"))
        }),
        "expected the retry-safety warning:\n{diagnostics:#?}"
    );
}

#[test]
fn no_admitted_flow_is_vacuously_idempotent() {
    let mut model = load_flash_checkout();

    let operation = model
        .operations
        .get_mut(&id("operation.cancel_order"))
        .unwrap();

    operation.flows.clear();

    operation
        .requirements
        .idempotency
        .push(IdempotencyRequirement {
            key: ikey("input.cancel_order.request", &[&["order_id"]]),
            response: ResponseReplayRequirement::Unspecified,
        });

    assert!(validation::validate(&model).is_empty());

    let verdict = idempotency_verdict(&model, "operation.cancel_order", 0);

    assert_eq!(
        verdict,
        IdempotencyVerdict::Proven {
            proof: IdempotencyProof::NoAdmittedFlows {
                input: id("input.cancel_order.request"),
            },
        }
    );
}
