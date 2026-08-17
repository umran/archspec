# Archspec DSL Semantics

**Status:** Normative semantic contract for the current DSL, prior to the verification/proof-solver stage.  
**Source of truth inspected:** `master`, 2026-08-16.  
**Implementation namespace:** `src/spec/`.

This document defines what an Archspec declaration means, what it does **not** mean, and what a verifier may soundly infer from it. It is intentionally stricter than a field reference: the purpose is to prevent the analyzer, an LLM author, and a human reader from silently assigning different meanings to the same declaration.

---

## 1. Interpretation model

Archspec describes a **logical architecture**, not a deployment manifest and not executable code.

A declaration belongs to one of three semantic categories:

| Category | Meaning | Examples |
|---|---|---|
| **Structural fact** | Describes what the modeled program can do or how entities relate. | operations, flows, effects, transactions, schemas |
| **Implementation guarantee / assumption** | A fact the model claims the implementation or external system provides. The verifier may rely on it, subject to implementation conformance. | topic ordering, delivery semantics, dispatch routing, transaction isolation, locks, effect idempotency, concurrency bounds |
| **Requirement / obligation** | A property the architecture says must hold. It is **not** a guarantee merely because it is declared. The verifier must prove it from facts and structure. | operation serialization, operation ordering, operation idempotency, object linearizability |

A structurally valid model is therefore not necessarily a safe model. Validation establishes that declarations are coherent and references are meaningful. Verification establishes whether the declared requirements follow from the declared facts and architecture.

### 1.1 `unspecified` is epistemic

Across the DSL, `unspecified` means:

> The model provides no fact from which the corresponding property may be inferred.

It does **not** mean that the property is false, and it does not mean the implementation is allowed to violate a requirement. It means the verifier must treat the fact as unknown.

For example:

- `ordering: unspecified` does not prove messages are unordered.
- `concurrency: unspecified` does not prove concurrent execution exists.
- `idempotency: unspecified` does not prove an external effect is non-idempotent.
- `isolation: unspecified` does not prove transactions are weakly isolated.

An unknown fact cannot be used as evidence for a proof.

### 1.2 Absence of a guarantee is not evidence of a violation

`unordered`, `unbounded`, and `not_deduplicated` are stronger negative declarations than `unspecified`:

- **Unordered** explicitly says no ordering guarantee is provided.
- **Unbounded** explicitly says no finite bound is declared.
- **NotDeduplicated** explicitly says duplicate executions are not deduplicated at that boundary.

Even these declarations describe guarantees, not necessarily observed runtime behavior. An unordered topic may happen to emit messages in order in one execution; the verifier simply may not rely on that.

### 1.3 Requirements are conditional on model conformance

Any proof produced by Archspec is conditional on the real implementation satisfying the declarations used by the proof. A proof based on `serializable`, `recoverable`, or `deduplicated_by`, for example, is invalid if the concrete implementation does not actually provide those semantics.

---

## 2. Model, revision, and IDs

### `Model`

`Model` is the root semantic object. It contains:

- services,
- schemas,
- data models,
- topics,
- state machines,
- operations,
- and a revision.

The collections describe one architecture snapshot.

### `Revision`

`revision` is an opaque numeric revision marker for the model.

The current DSL assigns no ordering, compatibility, migration, or version-negotiation semantics beyond its numeric identity. A verifier must not infer that revision `2` is semantically compatible with, derived from, or newer in any meaningful architectural sense than revision `1` unless surrounding tooling establishes that convention.

### `Id`

`Id` is a logical identifier, serialized as a string.

Archspec uses one common ID type rather than entity-specific Rust ID types. The semantic kind of a reference is determined by context and structural validation.

IDs should be treated as stable logical names, not as runtime addresses, URLs, database keys, or deployment identifiers unless a higher layer explicitly gives them that meaning.

---

## 3. Services

### `Service`

A service is a logical ownership/grouping boundary for operations.

### `ServiceKind`

Current kinds are:

- `backend`
- `frontend`
- `worker`
- `job`

These are descriptive classifications only.

A service kind does **not** by itself imply:

- a process boundary,
- a network hop,
- a host or container,
- a trust boundary,
- a replica count,
- availability semantics,
- concurrency,
- transactional boundaries,
- or failure independence.

Those facts must come from other declarations if they matter to a proof.

---

## 4. Schemas and field paths

### `Schema::Canonical`

A canonical schema describes the logical shape of a value.

Its fields have a type and an `optional` flag.

#### `SchemaCompleteness::complete`

`complete` claims that the declaration describes the complete logical schema.

Subject to conformance, the verifier may treat a field absent from a complete schema as nonexistent.

#### `SchemaCompleteness::partial`

`partial` explicitly permits the real schema to contain undeclared fields.

The verifier may reason about declared fields, but it must **not** infer that undeclared fields do not exist.

This distinction matters when proving properties that depend on exhaustive field knowledge.

### `Schema::Fragment`

A fragment is a projection/aliasing view over another declared schema.

`source` identifies the source schema. `mapping` maps each fragment field name to a `FieldPath` in the source.

A mapping asserts semantic identity of the referenced value across the fragment boundary. It may therefore preserve value lineage even when a field is renamed.

A fragment does **not**:

- create a new independent value,
- create a new storage object,
- imply that unmapped source fields are absent,
- or establish ordering/idempotency by itself.

Fragment chains must remain acyclic and resolvable.

### `Field`

`optional: true` means the logical value may be absent. `optional: false` means the declared schema requires it.

This is a schema-shape claim, not a runtime availability/liveness claim.

### `TypeRef`

#### `scalar`

Declares one of the current primitive logical types:

`string`, `bool`, `int`, `float`, `decimal`, `uuid`, `timestamp`.

These are logical types. No storage width, precision, locale, timezone encoding, or wire representation is implied beyond what the scalar name itself requires.

#### `schema`

References another declared schema as the logical type.

#### `list`

Declares a collection of values of the nested type.

A list declaration does not itself imply uniqueness, sortedness, stable ordering across executions, bounded length, or set semantics.

### `FieldPath`

A field path identifies a nested value relative to a schema.

For example, `[customer, id]` means the `id` field nested under `customer`.

A `FieldPath` has meaning only relative to the schema of its containing declaration or value source.

---

## 5. Data models and persistent objects

### `DataModel`

A data model is a **logical transactional state boundary** containing persistent objects.

It is not necessarily one database server, one vendor product, one schema namespace, or one physical storage engine. What matters is that a transaction declaring this data model is modeled as operating against this shared transactional boundary.

### `DataObject`

A data object is a logical class of persistent object instances.

`schema` identifies the canonical state schema for an instance.

### `identity`

`identity` is the complete logical identity of one object instance.

The vector contains the components of a **single composite identity**.

For example:

```yaml
identity:
  - [tenant_id]
  - [account_id]
```

means the object is identified by the tuple:

`(tenant_id, account_id)`

It does **not** mean that `tenant_id` and `account_id` are alternative independent keys.

Object identity is important for selector precision, unique claims, conflict analysis, linearizability domains, and lock reasoning.

### `ObjectRequirements.history`

Object history declarations are **requirements**, not guarantees.

#### `linearizable`

For each logical object instance, all modeled operations observing or mutating that instance must collectively admit a legal sequential history that respects real-time precedence.

Important consequences:

- linearizability is per logical object identity unless a broader object is modeled;
- it is stronger than serializability because it includes real-time precedence;
- serializable transactions do not automatically prove object linearizability;
- linearizability of one object does not imply atomicity or serializability across different object identities.

The proof solver must discharge this requirement from the architecture's actual synchronization, ordering, transactional, and execution facts.

---

## 6. Topics and ordering

### `Topic.messages`

`messages` is the set of schemas that may be published to the topic.

Membership means the topic is allowed to carry that schema. It does not assert that such a message is ever published.

### `Topic.ordering`

Ordering is a **guarantee provided by the topic abstraction**.

#### `unspecified`

No usable ordering fact is declared.

#### `unordered`

No message-order guarantee is provided.

The verifier may not rely on observed publication or delivery order.

#### `global`

All messages accepted by the topic participate in one logical ordered sequence.

This is an ordering guarantee at the topic boundary. It does not by itself serialize consumer execution.

#### `keyed`

Messages sharing the same logical key participate in one ordered sequence for that key.

Messages with different keys need not be ordered relative to one another.

### `TopicKey.mapping`

For every message schema carried by a keyed topic, the mapping identifies the field that represents the topic's logical key.

Different schemas may map differently named fields into the same logical key domain.

For example:

- `OrderCreated.order_id`
- `OrderCancelled.id`

may both represent the same logical `order` key domain if the topic mapping says so.

The mapping establishes key-domain equivalence; it does not itself establish causal precedence between independently produced messages.

### Topic order is not execution serialization

A topic ordering guarantee describes the order in which messages are logically observed by the subscription abstraction.

It does **not** imply:

- that two consumer invocations cannot overlap,
- that the consumer executes one message at a time,
- that effects produced by the consumer cannot overtake one another,
- or that independent producers had a meaningful business-level happens-before relationship.

To carry topic ordering through operation execution, dispatch and concurrency facts must also support it.

### Ordered transport does not invent business order

If two independent upstream producers concurrently publish messages for the same logical key, a keyed topic may impose a transport sequence between them. That sequence is a real transport order, but it does not prove that either message was semantically required to precede the other.

The verifier must distinguish:

1. an order that merely exists because a transport serialized concurrent inputs, and
2. an upstream semantic/causal precedence that the architecture is required to preserve.

This distinction is central to ambiguous-ordering analysis.

---

## 7. Operations

### `Operation`

An operation is a logical unit of application behavior owned by one service.

Its declaration contains possible invocation sources, effects, durable primitives, transactions, flows, requirements, and execution facts.

`description` is documentation only and has no proof semantics.

### Multiple inputs

Each `Input` declaration is a possible source of an invocation of the operation.

A concrete invocation is associated with the input that triggered it. A `ValueRef` whose source is an input refers to the payload of that triggering logical input.

Multiple input declarations do not mean that one invocation simultaneously receives all of them.

### Declared effects are capabilities, not executions

An effect appearing in `operation.effects` is an effect the operation **may execute**.

Declaration alone does not mean the effect occurs.

Execution is represented by a flow step, a durable effect-intent path, or another construct that explicitly associates the effect with behavior.

### Transactions are declarations, not executions

A transaction in `operation.transactions` is an atomic unit available to the operation's flows.

It executes only when a flow references it.

### Flows are alternative complete paths

Each `InvocationFlow` describes one permitted terminal path through an invocation.

Its steps occur in declaration order.

Multiple flows represent alternatives. Their mere existence does not mean that the flows execute concurrently or that every invocation executes every flow.

A flow may terminate with a declared response. `response: null` is natural for subscription-driven operations or other paths with no request response.

---

## 8. Inputs

## 8.1 Request input

A request input declares a directly invoked operation input and the schema of its payload.

The current request declaration does **not** itself encode:

- transport protocol,
- caller identity,
- retry behavior,
- timeout behavior,
- synchronous network semantics,
- or whether the request originated from a user versus another service.

Outbound operation-to-operation calls are modeled separately through `RequestEffect`.

## 8.2 Subscription input

A subscription declares invocation from a topic.

Its semantics are the combination of:

- topic,
- selected message schemas,
- delivery semantics,
- dispatch routing,
- lane concurrency.

### `MessageSelector::all`

Every schema carried by the topic may invoke this operation through this subscription.

### `MessageSelector::only`

Only the listed topic message schemas may invoke through this subscription.

It does not restrict what other schemas the topic itself may carry.

### Delivery semantics

#### `unspecified`

Duplicate/loss behavior is unknown.

#### `at_most_once`

The same logical message is delivered no more than once.

Loss may still occur.

This is not an exactly-once guarantee.

#### `at_least_once`

A successfully published logical message may be delivered more than once.

Therefore duplicate operation invocation must be considered possible.

The current declaration is primarily a duplicate-delivery fact. It does not encode retry timing, retry count, backoff, or a bounded eventual-delivery liveness guarantee.

### Dispatch routing

Dispatch routing says how deliveries are assigned to logical execution lanes.

#### `unspecified`

No lane-affinity fact is available.

#### `unconstrained`

No useful affinity between related deliveries and lanes is guaranteed.

#### `single_lane`

Every delivery for this subscription enters one logical lane.

This creates affinity, but does not alone imply serial execution; lane concurrency still matters.

#### `by_topic_key`

Deliveries sharing the topic's logical ordering key enter the same logical lane.

This preserves same-key affinity. It is meaningful only in conjunction with a topic ordering/key model that establishes the relevant key domain.

It does not itself imply that the lane processes one invocation at a time.

### Lane concurrency

#### `bounded(n)`

At most `n` operation invocations from the same logical lane may be simultaneously active.

`bounded(1)` is the important serialization case: invocations in one lane cannot overlap.

#### `unbounded`

No finite per-lane concurrency bound is declared.

#### `unspecified`

No per-lane concurrency fact is available.

### Topic order + routing + lane concurrency

A common proof pattern for same-key ordered serial execution is:

`keyed topic order`
→ `by_topic_key dispatch`
→ `lane concurrency = 1`

Each declaration contributes a different fact:

- the topic establishes an observed same-key sequence,
- routing keeps that key on one lane,
- concurrency one prevents overlap on that lane.

None of the three should be silently substituted for another.

---

## 9. Operation requirements

Operation requirements are **proof obligations**.

Declaring one does not assert that the operation already satisfies it.

### `SerializationRequirement`

A serialization requirement keyed by a `ValueRef` means:

> Invocations with the same logical key must not execute concurrently.

Different keys may execute concurrently unless constrained elsewhere.

Serialization establishes mutual exclusion/non-overlap. It does **not** establish which same-key invocation should come first.

Thus a lock, single-lane execution, or another mechanism may prove serialization without proving ordering.

### `OrderingRequirement`

An ordering requirement keyed by a `ValueRef` means:

> Same-key invocations for which a meaningful logical precedence exists must preserve that precedence through the operation's semantically relevant execution.

Ordering is stronger than merely choosing *some* serial order.

A proof must therefore establish both:

1. where the relevant precedence comes from, and
2. that the execution mechanism preserves it.

Arbitrarily serializing concurrent inputs can satisfy a serialization requirement but cannot invent a semantic precedence required by an ordering proof.

Where preserving the required order entails preventing later invocations from overtaking earlier ones, the proof must also establish the necessary execution serialization.

### Serialization versus ordering

These terms are deliberately separate:

- **serialization**: same-key invocations do not overlap;
- **ordering**: the correct same-key precedence is preserved.

A FIFO mutex may provide both if its acquisition order is proven to correspond to the required input order. A non-FIFO mutex may provide serialization without providing the required ordering.

### `IdempotencyRequirement`

An idempotency requirement identifies a logical invocation by a composite `IdempotencyKey`.

The requirement means:

> Repeated attempts representing the same logical invocation must not cause externally distinguishable duplicate logical work beyond what the declared idempotency contract permits.

The solver must follow duplicate/retry paths through transactions, publications, requests, external effects, and durable intent recovery.

The requirement is not discharged merely because the operation has a field named `idempotency_key`.

### `ResponseReplayRequirement::replay_consistent`

When replay consistency is required, retries for the same idempotency key must return the same logical response rather than recomputing a potentially different response from mutable state.

A durable immutable `InvocationResult` is the DSL primitive intended to support this proof.

### `response: unspecified`

No replay-stability requirement is declared for the response.

This does not waive the operation's idempotency requirement for side effects.

---

## 10. Operation execution concurrency

### `ExecutionSemantics.concurrency`

This is an **implementation fact**, not an operation requirement.

#### `bounded(n)`

At most `n` invocations of the logical deployed operation may be simultaneously active across the operation as a whole.

This is a global operation bound, distinct from subscription lane concurrency.

A bound greater than one does not prove same-key serialization.

#### `unbounded`

No finite global concurrency bound is declared.

This does not mean infinitely many invocations literally execute; it means the verifier cannot rely on a finite global cap.

#### `unspecified`

No global concurrency fact is available.

---

## 11. Value references

### `ValueRef`

A value reference consists of:

- a `ValueSource`,
- and a `FieldPath` relative to that source's schema.

It identifies a logical value and is the main mechanism for linking keys and predicates across the model.

### `ValueSource::input`

References a field in the current invocation's input payload.

### `ValueSource::effect`

References a field in the payload of a declared `PublicationEffect` or `RequestEffect`.

Declaring such a reference establishes value lineage only if the surrounding declaration states how the value is propagated. It does not mean the effect has already executed.

An external effect has no inspectable payload schema in the current DSL and therefore cannot provide ordinary field-path value references.

### `ValueSource::invocation_result`

References a field in a durable invocation result.

### `ValueSource::state_machine_subject`

References a field on the persistent object governed by the identified state-machine subject.

The path is interpreted against that subject object's schema.

---

## 12. Idempotency keys and propagation

### `IdempotencyKey`

An idempotency key is an ordered tuple of `ValueRef` components.

Two attempts have the same declared idempotency identity when all components are equal in the declared component order.

A composite key is one logical key, not a set of independent alternative keys.

The current DSL assigns no special semantic meaning to an empty component list; authors should not rely on one unless a future contract explicitly defines it.

### `IdempotencyKeyPropagation`

A propagation declares that the target values carry the same logical idempotency identity as the source values.

This is a **lineage assertion**.

It can bridge renamed fields or different message/request schemas.

Propagation does **not** itself deduplicate anything. It allows the verifier to trace the same logical key across an effect boundary.

---

## 13. Effects

Effects describe work outside the operation's immediate transaction state.

## 13.1 Publication effect

A publication effect declares publication of one schema to one topic.

When executed, the resulting logical message participates in the topic's declared delivery and ordering semantics.

`idempotency_key_propagation` describes which values in the published payload preserve an upstream idempotency identity.

A publication declaration does **not** by itself imply:

- exactly-once publication,
- atomic publication with a database transaction,
- deduplication,
- eventual delivery,
- or that the effect executes at all.

Those properties require additional structure/facts.

## 13.2 Request effect

A request effect invokes a specific request input of another operation with the declared schema.

`target.operation` and `target.input` identify the destination.

### Retry semantics

#### `never`

The modeled request mechanism does not intentionally repeat the logical request.

This is a sender-side retry fact. It should not be inflated into a general exactly-once guarantee for every lower-level failure mode unless implementation conformance provides that stronger semantics.

#### `may_repeat`

The logical request may be attempted more than once.

Downstream duplicate invocation must therefore be considered possible.

#### `unspecified`

No retry fact is available.

`idempotency_key_propagation` links the outbound request's key fields to upstream logical identity.

## 13.3 External effect

An external effect marks a boundary beyond which Archspec does not inspect implementation structure.

`name` is descriptive.

Because the checker cannot analyze the external implementation, its idempotency behavior is supplied as an explicit assumption.

### `IdempotencyGuarantee::unspecified`

No deduplication fact is available.

### `not_deduplicated`

Repeated execution is not deduplicated at this external boundary.

A retry/duplicate path reaching such an effect is therefore potentially observably unsafe for an upstream idempotency requirement.

### `deduplicated_by`

The external boundary guarantees deduplication for executions sharing the declared idempotency key.

The guarantee is scoped to equality of that logical key. It does not imply ordering, transactionality, or deduplication across different keys.

---

## 14. Durable effect intents

### `EffectIntent`

An effect intent associates a logical effect with durable execution state.

It is useful for outbox-style or recoverable side-effect execution.

Declaring an intent does not establish it. `EstablishEffectIntent` does.

### `IntentExecutionSemantics::unspecified`

The intent may be durably established, but the model provides no guarantee that abandoned pending work is independently rediscovered after the creating invocation disappears.

### `recoverable`

Once established, pending work remains durably discoverable and eligible for retry independently of the invocation that created it.

This is a **recoverability/retry guarantee**, not an exactly-once guarantee and not a guarantee of eventual success.

A recoverable intent can cause the underlying effect to be attempted repeatedly. Idempotency analysis must therefore account for duplicates at the effect boundary.

### `ExecuteEffectIntent`

A flow step executing an intent performs/attempts the work represented by the established intent.

The durable intent remains the recovery anchor; the flow step is not equivalent to a claim that the effect can execute only once.

---

## 15. Invocation results and responses

### `InvocationResult`

An invocation result is a durable logical result identified by an `IdempotencyKey` and shaped by a schema.

It is intended as the immutable replay anchor for request idempotency.

The semantic contract is that an established invocation result represents the stable logical result associated with that key. A conforming implementation must not silently replace the result for the same logical replay identity with a different result.

### `EstablishInvocationResult`

Establishes the durable invocation result as part of the surrounding transaction.

When combined with other transaction steps, establishment participates in the same atomic transaction boundary.

### `ReadInvocationResult`

Reads the previously established durable invocation result.

Because invocation-result storage is framework-level durable state, a transaction containing only framework-level result/intent operations may omit `data_model`.

### `Response`

A response belongs to a request input and declares the response schema.

### `ResponseSource::unspecified`

The model gives no stable replay source for the response.

No replay-consistency proof may be derived solely from the response declaration.

### `ResponseSource::invocation_result`

The response is obtained from the referenced durable invocation result.

This allows the verifier to use the result as a stable replay source, subject to key/schema consistency and the operation's idempotency structure.

---

## 16. Invocation flows

### `InvocationFlow.steps`

Steps execute in the order declared within that flow.

Current flow-step kinds are:

- `transaction`
- `execute_effect`
- `execute_effect_intent`

### `transaction`

Executes the referenced operation-local transaction.

### `execute_effect`

Executes the referenced logical effect directly.

A direct effect execution is not automatically durable or retry-safe. The verifier must use the effect's retry/deduplication environment and the invocation's possible failure/retry paths.

### `execute_effect_intent`

Executes work through the referenced durable intent.

This is distinct from directly executing its effect because the intent may have been established atomically with transaction state and may be recoverable after invocation failure.

### `response`

If present, the response is terminal for that flow.

The response declaration itself does not imply every preceding external effect succeeded exactly once; the solver must analyze the path.

---

## 17. Transactions

### `Transaction`

A transaction is one atomic commit/abort unit.

Its object accesses are interpreted against its declared `data_model`.

Its steps are logically ordered as written.

Atomicity does not imply serializability, and serializability does not imply linearizability.

### `data_model: <id>`

The transaction operates against the identified logical transactional state boundary.

Object reads/writes/locks/transitions must refer to objects belonging to that data model.

### `data_model: null`

Permitted for a transaction that only manipulates framework-level durable state such as invocation results or effect intents.

It must not be used to imply atomic application-object access with no declared transactional boundary.

### Isolation

The solver should use the following abstract semantics.

#### `unspecified`

No isolation fact may be assumed.

#### `read_committed`

Reads do not observe uncommitted writes from other transactions.

The verifier must still consider anomalies permitted by read-committed execution, including non-repeatable reads and concurrent read/modify/write races unless prevented by stronger facts such as locks, unique claims, atomic write semantics, or serialization.

Read committed is not serializable.

#### `snapshot`

A transaction reads from one consistent committed snapshot for ordinary reads.

Snapshot isolation does not in general imply serializability; write skew and predicate-level anomalies must remain possible unless ruled out by additional facts.

#### `serializable`

Committed transactions admit an equivalent serial execution order.

Serializable does **not** by itself imply real-time precedence and therefore does not automatically prove linearizability.

### Transaction step order

The declared step sequence represents logical program order inside the transaction.

This is especially important for lock-order/deadlock analysis and for reasoning about when durable framework primitives are established relative to application state.

---

## 18. Object selectors and predicates

### `ObjectSelector`

An object selector identifies zero or more instances of one `DataObject`.

It contains the object type and a predicate.

A selector is not assumed to identify exactly one instance unless its predicate can be proven to constrain the complete declared object identity.

### `SelectorPredicate::all`

Selects all instances of the object.

### `eq`

Constrains one object field path to equal the specified selector value.

### `and`

All nested predicates must hold.

The current predicate language has no explicit `or`, range, inequality, or negation semantics.

### `SelectorValue::value`

Uses a `ValueRef` from the invocation/model.

### `SelectorValue::literal`

Uses a literal constant.

Current literal kinds are `string`, `bool`, and `int`.

Selector equality does not itself imply a lock or atomic check. It only describes the logical target set.

---

## 19. Read, write, insert, and delete steps

### `Read`

Reads the selected object instances.

`fields` describes the read set visible to conflict analysis.

#### `FieldSelection::all`

Reads all fields represented by the declared object schema.

If that schema is partial, the verifier must not silently treat this as proof that undeclared real-world fields do not exist; it means all fields represented by the model.

#### `only`

Reads only the listed field paths for the modeled semantics.

### `Write`

Mutates the listed fields of the selected object instances.

The DSL records the write set, not the new values.

A write declaration does not by itself say whether the implementation uses compare-and-swap, a blind update, an increment, or another physical primitive.

### `Insert`

Creates a new instance of the declared object type.

The current step does not encode field assignments. Identity/value lineage must therefore come from other declarations, such as a unique claim or surrounding operation semantics, when needed by a proof.

### `Delete`

Deletes the instances selected by the object selector.

---

## 20. Locks

### `Lock`

A lock is an explicit synchronization guarantee inside a transaction.

For verification, a conforming `Lock` step means:

1. the logical lock is acquired at that point in transaction program order;
2. it protects the object instances selected by `target`;
3. it is held until the surrounding transaction terminates.

Without hold-to-transaction-end semantics, the current DSL would not provide enough information for its intended serialization and deadlock reasoning.

### `shared`

Shared locks are mutually compatible with other shared locks on the same logical target, but conflict with exclusive locks.

### `exclusive`

An exclusive lock conflicts with both shared and exclusive locks on the same logical target.

### `LockOrder::unspecified`

No acquisition-order fact is provided for multiple concrete locks arising from the selector.

### `LockOrder::by`

Locks selected as part of this lock step are acquired according to the ordered list of `OrderingTerm`s.

Each term contains a field path and ascending/descending direction.

This is an acquisition-order fact, not an ordering requirement on business events.

A lock-order declaration may be used for deadlock reasoning only when competing transactions can be shown to use compatible order domains.

### Separate lock steps

Program order between separate `Lock` steps is itself relevant to the lock-order graph.

A `by` order within one selector does not automatically reconcile contradictory order between two separately declared lock steps.

---

## 21. Unique claims

### `AcquireUniqueClaim`

A unique claim establishes exclusive uniqueness for one logical object identity.

`mapping` maps object identity field paths to invocation values.

The mapping must cover the complete declared identity of the object.

If the full identity is covered, the verifier may treat competing attempts with the same mapped identity as contending for the same unique claim: they cannot both successfully establish distinct ownership of that one logical identity.

A unique claim is commonly implementable with a unique constraint, conditional insert, compare-and-set reservation, or another equivalent mechanism, but the DSL is implementation-independent.

A unique claim:

- is scoped to the declared object identity;
- does not serialize different identities;
- does not establish business ordering;
- and does not substitute for a lock on unrelated mutable fields.

---

## 22. State machines

### `StateMachine`

A state machine defines the legal states and legal transitions of a persistent object field.

### `StateMachineSubject::object`

`object` identifies the persistent object class.

`state` identifies the field in the object's canonical schema that stores the logical machine state.

### `states`

The set of legal logical states.

### `initial`

The initial state for a newly created logical machine instance.

It does not imply that every existing persistent record is currently in that state.

### `Transition.from`

The set of states from which this transition is legal.

### `Transition.to`

The destination state.

### `TransactionStep::transition`

Selects a concrete persistent machine instance and applies the named transition.

The transition's `from` condition and update to `to` are interpreted as one logical state transition within the surrounding transaction.

The state machine declares **legality**, not concurrency safety. Two individually legal transitions can still race. The verifier must use isolation, locks, serialization, ordering, or other facts to prove that concurrent execution cannot produce an illegal history.

### Transition side effects

A transition may declare publication or request side effects associated with taking that transition.

These are logical effects of the transition. Their declaration does not magically make an external transport participate in the local database transaction.

The verifier must not infer exactly-once or atomic external delivery merely from the fact that an effect is attached to a transition.

If crash-safe atomic coupling is required, the model must contain a structure that actually establishes that guarantee.

---

## 23. Framework durable state versus application data

Effect intents and invocation results are framework-level durable primitives.

This is why a transaction containing only operations such as:

- `EstablishEffectIntent`
- `EstablishInvocationResult`
- `ReadInvocationResult`

may have `data_model: null`.

Once a transaction reads, writes, locks, inserts, deletes, or transitions an application `DataObject`, its application transactional boundary must be declared.

Framework durability should not be interpreted as a hidden global transaction spanning arbitrary application data models.

---

## 24. Crucial distinctions for the proof solver

The solver must preserve these distinctions:

| Concepts | Why they are not interchangeable |
|---|---|
| **Requirement vs guarantee** | A declared requirement still needs proof. |
| **Validation vs verification** | A coherent model can still describe an unsafe architecture. |
| **Unspecified vs negative guarantee** | Unknown is not the same as explicitly unordered/unbounded/non-deduplicated. |
| **Topic ordering vs execution ordering** | Ordered delivery can still lead to concurrent/overtaking execution. |
| **Ordering vs serialization** | Serialization prevents overlap; ordering preserves the correct precedence. |
| **Transport order vs semantic order** | A broker can serialize concurrent producers without establishing a business-level happens-before relation. |
| **Operation concurrency vs lane concurrency** | One is global to the deployed operation; the other is per dispatch lane. |
| **Serializability vs linearizability** | Serializable histories need not respect real-time precedence. |
| **Atomic transaction vs external side effect** | Local atomic commit does not imply an external publication/request is atomic with it. |
| **Idempotency lineage vs deduplication** | Propagating a key lets the analyzer trace identity; only a guarantee/mechanism actually deduplicates. |
| **Recoverability vs exactly once** | A durable intent can be retried after failure, which often increases duplicate-execution risk. |
| **Object identity vs ordering key** | They may coincide, but neither declaration automatically implies the other. |
| **State-machine legality vs race safety** | A valid transition graph does not ensure concurrent transitions are safely coordinated. |

---

## 25. What a successful Archspec proof means

A successful proof should be read as:

> Given the declared architecture facts, given the semantic contract in this document, and assuming the concrete implementation conforms to the declarations used by the proof, the specified requirement follows for all executions admitted by the model.

It should **not** be read as:

> The implementation is universally correct.

Archspec proves selected application-level properties over a declared abstraction. Its strength comes from making the abstraction explicit and forcing correctness arguments to state which facts they depend on.

---

## 26. Authoring rule of thumb

When declaring a fact, ask:

> Would I be willing for the verifier to rely on this statement in a correctness proof?

If not, use `unspecified` or omit the stronger claim.

When declaring a requirement, ask:

> What observable property would make the architecture wrong if it failed?

Keep that requirement separate from the mechanism expected to satisfy it. The solver's job is to connect the two.
