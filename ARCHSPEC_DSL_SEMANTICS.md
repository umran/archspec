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

Any proof produced by Archspec is conditional on the real implementation satisfying the declarations used by the proof. A proof based on `serializable`, deterministic provenance, or `deduplicated_by`, for example, is invalid if the concrete implementation does not actually provide those semantics.

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

`identity` is the complete, non-empty logical identity of one object instance.

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

Object identity is important for selector precision, insertion uniqueness, conflict analysis, linearizability domains, and lock reasoning. The declared identity is intrinsic to the logical object model: two distinct successfully created instances cannot share the same complete identity.

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

Its declaration contains possible invocation sources, effects, transaction artifacts, transactions, flows, requirements, and execution facts.

`description` is documentation only and has no proof semantics.

### Multiple inputs

Each `Input` declaration is a possible source of an invocation of the operation.

A concrete invocation is associated with the input that triggered it. A `ValueRef` whose source is an input refers to the payload of that triggering logical input.

Multiple input declarations do not mean that one invocation simultaneously receives all of them.

### Declared effects are capabilities, not executions

An effect appearing in `operation.effects` is an effect the operation **may execute**.

Declaration alone does not mean the effect occurs.

Execution is represented by a flow step, an effect-intent path, or another construct that explicitly associates the effect with behavior.

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

The solver must analyze the complete admitted retry path through transactions, transaction artifacts, publications, requests, and external effects.

A transaction may contribute to the proof in two distinct ways:

1. **natural replayability**, derived from the transaction's declared semantics and deterministic provenance; or
2. **explicit durable keyed commit deduplication**, declared with `DeduplicatedBy { key }` on the transaction.

These mechanisms are not interchangeable. A transaction that merely prevents a second commit is not necessarily naturally replayable, because a retry may need to reproduce transaction artifacts required by later flow steps.

The requirement is not discharged merely because the operation has a field named `idempotency_key`, because an `InvocationResult` exists, or because an `EffectIntent` exists.

### `ResponseReplayRequirement::replay_consistent`

When replay consistency is required, retries for the same logical invocation must resolve the same logical response.

A response sourced from an `InvocationResult` is replay-consistent only when the solver can establish a safe path to the same logical result. V1 recognizes two principal routes:

1. the establishing transaction is naturally replayable and the result derivation is replay-deterministic; or
2. the establishing transaction is `DeduplicatedBy { key }`, and the exact result produced by the prior successful keyed commit is durably retained and recovered.

`ResponseSource::InvocationResult` does not, by itself, imply durable memoization or transaction idempotency.

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

It identifies a logical value and is the main mechanism for linking keys, predicates, and deterministic provenance across the model.

### `ValueSource::input`

References a field in the current invocation's input payload.

An input reference is not automatically replay-stable merely because two attempts share an idempotency key. Replay stability must follow from the operation's declared idempotency equivalence or other established provenance facts.

### `ValueSource::effect`

References a field in the payload of a declared `PublicationEffect` or `RequestEffect`.

Declaring such a reference establishes value lineage only if the surrounding declaration states how the value is propagated. It does not mean the effect has already executed.

An external effect has no inspectable payload schema in the current DSL and therefore cannot provide ordinary field-path value references.

### `ValueSource::invocation_result`

References a field in a logical `InvocationResult` available to the current invocation.

Availability may come from production earlier in the current flow, deterministic reconstruction by a naturally replayable establishing transaction, or recovery from an explicitly keyed committed transaction. The source kind does not itself imply independent durable storage.

### `ValueSource::state_machine_subject`

References a field on the persistent object governed by the identified state-machine subject.

The path is interpreted against that subject object's schema. Mutable subject state is not automatically replay-stable.

### `ValueSource::transaction_read`

References a field observed by a named `Read` earlier in the same transaction execution.

Transaction-read results are transaction-local provenance sources. They are not durable cross-transaction artifacts and are not available to later transactions merely because the surrounding flow continues.

V1 permits them in the semantic model but does not use a provenance chain that reaches a transaction read to prove natural transaction replayability. See §18.

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

## 14. Effect intents

### `EffectIntent`

An effect intent is a **logical transaction artifact** describing an intended effect execution.

An effect intent is not inherently synonymous with a durable database record, and declaring one does not establish it. `EstablishEffectIntent` establishes the logical artifact as part of a transaction execution.

The current `IntentExecutionSemantics::{Unspecified, Recoverable}` model is superseded by this revision. An intent declaration does not imply an invisible independent executor or independent rediscovery mechanism.

### Intent derivation

The establishment site should declare how the intent's logical contents are produced through a `Derivation` declaration.

If the intent is deterministically derived from replay-stable provenance and the establishing transaction is naturally replayable, a retry may reconstruct the same logical intent without requiring the intent payload itself to have been durably materialized.

If the establishing transaction is explicitly `DeduplicatedBy { key }`, the exact intent produced by the first successful logical commit is retained with that commit and recovered when the transaction step is encountered again under the same key.

### `ExecuteEffectIntent`

A flow step executing an intent performs or attempts the work represented by the logical intent available to the current invocation.

`ExecuteEffectIntent` is the modeled execution authority for the intent. Intent establishment alone does not execute the underlying effect.

Reconstructing or recovering the same intent does **not** prove that repeating the external effect is safe. A crash after an external effect succeeds but before completion is durably known may still lead to another effect attempt. Effect-level idempotency/retry semantics must handle that uncertainty.

---

## 15. Invocation results and responses

### `InvocationResult`

An invocation result is a logical transaction artifact shaped by a declared schema.

It is semantically separate from transaction idempotency. Establishing an invocation result does **not**, by itself, prevent the enclosing transaction from executing or committing again.

An invocation result is not inherently synonymous with a durable database record. Its logical availability after retry may come from deterministic reconstruction or from durable retention by an explicitly keyed transaction commit.

An artifact-level idempotency key, if still present in an interim implementation shape, must not be interpreted as an independent transaction-deduplication or durability guarantee. The revised structural model may remove that field entirely.

### `EstablishInvocationResult`

Establishes the logical result produced by the surrounding transaction execution.

The establishment site should declare result-value provenance through `Derivation`.

If the transaction is naturally replayable and the result derivation is replay-deterministic, a retry may reproduce the same logical result without independent durable result storage.

If the transaction is `DeduplicatedBy { key }`, the exact result produced by the first successful commit is retained with `Commit(T,K)` and recovered on replay instead of being recomputed.

### `ReadInvocationResult`

The explicit transaction step `ReadInvocationResult` is removed by the revised model unless a separate concrete semantic use case is established.

A later transaction may reference an available result directly through `ValueSource::InvocationResult`.

### `Response`

A response belongs to a request input and declares the response schema.

### `ResponseSource::unspecified`

The model gives no stable replay source for the response.

No replay-consistency proof may be derived solely from the response declaration.

### `ResponseSource::invocation_result`

The response is obtained from the logical invocation result available to the current invocation.

The solver may treat that response as replay-consistent only when it can prove that the same logical result will be reconstructed or recovered on retry.

---

## 16. Invocation flows and transaction artifacts

### `InvocationFlow.steps`

Steps execute in the order declared within that flow.

Current flow-step kinds remain:

- `transaction`
- `execute_effect`
- `execute_effect_intent`

No explicit `RecoverInvocationResult` or `RecoverEffectIntent` flow step is introduced.

### `transaction`

Executes or resolves the referenced operation-local transaction.

For an ordinary transaction, this means executing the transaction body.

For a transaction explicitly `DeduplicatedBy { key }`, if the same logical commit already exists, the step resolves that prior commit instead of committing the body again and restores the artifacts retained by that commit.

### `execute_effect`

Executes the referenced logical effect directly.

A direct effect execution is not automatically durable or retry-safe. The verifier must use the effect's retry/deduplication environment and the invocation's possible failure/retry paths.

### `execute_effect_intent`

Executes the referenced logical effect intent currently available to the invocation.

The intent may have been produced by an earlier transaction in this invocation, reconstructed by naturally replaying that transaction, or recovered from an explicitly keyed transaction commit.

### Transaction-artifact visibility

A successful transaction may make `InvocationResult` and `EffectIntent` artifacts available to subsequent flow steps and subsequent transactions in the same invocation.

Conceptually, the invocation carries an abstract artifact context:

```text
ArtifactContext
    InvocationResult R -> logical result value
    EffectIntent E     -> logical effect intent
```

This context is semantic bookkeeping, not a new DSL workflow construct.

Artifact availability may arise from:

1. production earlier in the current invocation;
2. deterministic reconstruction during natural transaction replay; or
3. recovery from a prior `Commit(T,K)` for an explicitly deduplicated transaction.

Transaction-read results are excluded: they remain local to the transaction execution that produced them.

### `response`

If present, the response is terminal for that flow.

The response declaration itself does not imply every preceding external effect succeeded exactly once; the solver must analyze the path.

---

## 17. Transactions, replayability, and explicit idempotency

### `Transaction`

A transaction is one atomic commit/abort unit.

Its object accesses are interpreted against its declared `data_model`. Its steps are logically ordered as written.

Atomicity does not imply serializability, and serializability does not imply linearizability.

Framework transaction artifacts established by the transaction participate in the same logical atomic boundary as application-state mutations.

### `data_model: <id>`

The transaction operates against the identified logical transactional state boundary.

Object reads/writes/locks/inserts/deletes/transitions must refer to objects belonging to that data model.

### `data_model: null`

Permitted when the transaction performs no application `DataObject` access and only produces or consumes framework transaction artifacts.

It must not be used to imply atomic application-object access with no declared transactional boundary.

### Transaction idempotency guarantee

A transaction should expose an `IdempotencyGuarantee` independently of any invocation-result or effect-intent declaration.

#### `unspecified`

No explicit keyed transaction-commit deduplication fact is available.

The analyzer may still prove **natural replayability** from the transaction's declared semantics.

#### `not_deduplicated`

The architecture explicitly declares that the execution environment provides no keyed transaction-commit deduplication for this transaction.

The analyzer may still prove natural replayability.

#### `deduplicated_by`

For transaction declaration `T` and evaluated key `K`, the execution environment guarantees a durable logical commit identity:

```text
Commit(T,K)
```

At most one logical execution of `T(K)` may successfully commit.

On the first successful execution, application state, `Commit(T,K)`, and the exact transaction artifacts produced by that execution commit atomically.

If `Commit(T,K)` already exists on a later encounter:

- the transaction body is not committed again;
- the prior logical commit is resolved;
- artifacts retained by that commit are restored to the invocation artifact context.

Concurrent attempts with the same `(T,K)` must not both successfully commit.

This is a concrete implementation/conformance obligation, not a claim that arbitrary transaction code is mathematically idempotent.

### Natural replayability

Natural replayability is derived, not declared with a boolean.

A transaction is naturally replayable only when the verifier can establish that another execution for the same logical invocation can safely reproduce the same logical transaction outcome and any artifacts required by later flow steps.

This is stronger than merely showing that a second commit cannot happen.

A one-shot guard that makes a second attempt abort may establish at-most-once commit behavior while still preventing the flow from reconstructing artifacts after a crash. Such a guard therefore does not, by itself, prove natural replayability.

V1 may use deterministic target/value provenance and mutation semantics where sufficient. If required facts are absent, natural replayability is `Unknown`.

### Artifact replay after a transaction

For an artifact required after a crash, V1 accepts either:

```text
A. reconstruction
   establishing transaction naturally replayable
   +
   artifact derivation replay-deterministic

OR

B. recovery
   establishing transaction DeduplicatedBy(K)
   +
   artifact retained by Commit(T,K)
```

Otherwise the artifact's retry availability/consistency is not proven.

### Isolation

The solver should use the following abstract semantics.

#### `unspecified`

No isolation fact may be assumed.

#### `read_committed`

Reads do not observe uncommitted writes from other transactions.

The verifier must still consider anomalies permitted by read-committed execution, including non-repeatable reads and concurrent read/modify/write races unless prevented by stronger facts such as locks, atomic mutation semantics, uniqueness, or serialization.

Read committed is not serializable.

#### `snapshot`

A transaction reads from one consistent committed snapshot for ordinary reads.

Snapshot isolation does not in general imply serializability; write skew and predicate-level anomalies must remain possible unless ruled out by additional facts.

#### `serializable`

Committed transactions admit an equivalent serial execution order.

Serializable does **not** by itself imply real-time precedence and therefore does not automatically prove linearizability.

Serializable execution also does not imply that a transaction is replayable across separate invocation attempts.

### Transaction step order

The declared step sequence represents logical program order inside the transaction.

This is especially important for lock-order/deadlock analysis, transaction-read provenance, state transitions, and reasoning about when transaction artifacts are established relative to application state.

---

## 18. Deterministic derivation and transaction reads

### `Derivation`

The revised DSL introduces a small provenance declaration for opaque value computation:

```rust
pub enum Derivation {
    Unspecified,
    Deterministic { from: Vec<ValueRef> },
}
```

`Deterministic { from }` means:

> The produced values are a deterministic function solely of the declared source values.

It does **not** assert that those source values are replay-stable.

The verifier separately determines replay stability of provenance roots.

Therefore:

```text
deterministic derivation
        +
replay-stable provenance
        ↓
replay-deterministic produced value
```

### Transaction read results

A `Read` should identify a transaction-local result so later steps in the same transaction can reference fields from that result through `ValueSource::TransactionRead`.

A transaction-read result is an observation of transaction state, not a replay-stability guarantee.

Validation should require that a transaction-read source:

- refers to a read in the same transaction;
- refers only to fields selected by that read; and
- is used only after that read in transaction program order.

### V1 read-dependent replay rule

V1 is deliberately conservative:

> If the provenance closure of a persistent mutation target, mutation value, or transaction artifact reaches a `TransactionRead`, V1 does not use that path to prove natural transaction replayability.

The result is `Unknown`, not necessarily `Violated`.

Determinism of the computation is insufficient. The value observed by the read may differ on retry even when no other process modified it.

In particular, a transaction can read a field and then deterministically write a function of that value back to the same object:

```text
Read A.counter -> r
Write A.counter = f(r.counter)
```

For deterministic `f(x) = x + 1`, the first execution may observe `5` and commit `6`, while the retry observes `6` and commits `7`. The computation is deterministic but the transaction is not naturally replayable.

A future solver may attempt a stronger invariance proof. Such a proof must account for both:

1. mutations by other admitted executions between attempts; and
2. the establishing transaction's own effect on the state it later re-reads.

For a read observation function `R` and transaction state transformation `T`, absence of external writers is insufficient; the solver may need to establish an invariant corresponding to `R(S) = R(T(S))` over the relevant admitted states, together with any required interleaving guarantees.

---

## 19. Object selectors and predicates

### `ObjectSelector`

A selector identifies which instances of one declared `DataObject` a transaction step addresses.

The selector is a logical predicate, not a claim about a particular database query plan or index.

### `SelectorPredicate::all`

Selects every modeled instance of the object satisfying no narrower condition.

This is a broad selector and may imply many concrete object accesses.

### `eq`

Requires the selected object's field to equal either:

- a `ValueRef`, or
- a literal.

The equality is a logical predicate over modeled values.

Because the selector explicitly exposes its literals and `ValueRef`s, selector provenance should be derived structurally rather than asserted with a separate `deterministic` flag.

### `and`

All nested predicates must hold.

The list is conjunctive. It does not define short-circuit evaluation order or physical query evaluation order.

### Selector precision and object identity

A selector constraining every field of a `DataObject.identity` to one logical value identifies at most one logical object instance.

A selector constraining only part of a composite identity may match multiple logical instances.

A verifier must not treat partial identity coverage as single-object selection.

---

## 20. Read, write, insert, and delete steps

### `Read`

Reads the selected object instances.

`fields` describes the read set visible to conflict analysis.

The revised model should also name the transaction-local read result so later steps in the same transaction can use it as deterministic provenance.

#### `FieldSelection::all`

Reads all fields represented by the declared object schema.

If that schema is partial, the verifier must not silently treat this as proof that undeclared real-world fields do not exist; it means all fields represented by the model.

#### `only`

Reads only the listed field paths for the modeled semantics.

### `Write`

Mutates the listed fields of the selected object instances.

The revised model should declare the provenance of the values written through `Derivation`.

A deterministic derivation describes value computation, not replayability by itself. Natural replay analysis must additionally establish replay stability of the selected target and all derivation roots.

A write whose derivation is `Unspecified` normally leaves natural replayability `Unknown` when that mutation matters to the proof.

### `Insert`

Creates a new instance of the declared object type.

The revised model should declare inserted-value provenance through `Derivation` but must **not** redeclare object identity.

`DataObject.identity` already defines the strict non-empty logical identity of every object instance. Two distinct successful inserts cannot create two logical instances with the same complete identity. A separate `AcquireUniqueClaim`/`UniqueClaim` primitive is therefore redundant and is removed by the revised model.

Whether retrying a conflicting insert can participate in a natural replayability proof depends on the final duplicate-identity/insert outcome semantics. Until that behavior is explicitly defined, V1 must not infer full transaction replayability merely from object identity uniqueness.

### `Delete`

Deletes the instances selected by the object selector.

Deletion replay behavior depends on what the model guarantees when the selected instance is already absent. Unless sufficient semantics establish a reproducible outcome, the verifier must not silently treat deletion as naturally replayable merely because applying deletion twice leaves no object.

---

## 21. Locks

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

The state machine declares legality, not concurrency safety. Two individually legal transitions can still race. The verifier must use isolation, locks, serialization, ordering, or other facts to prove that concurrent execution cannot produce an illegal history.

### V1 transition replay rule

A transaction containing any `Transition` is **not naturally replayable in V1**.

Once a transition-containing transaction has committed, its state-dependent transition cannot be assumed to execute again in a way that reproduces the original transaction outcome and artifacts. A transition that prevents a second commit may provide an at-most-once gate, but that is not sufficient for flow crash recovery.

Accordingly, under the V1 contract:

> Every transaction containing a `Transition` MUST declare explicit durable keyed transaction idempotency with `DeduplicatedBy { key }`.

The purpose is not merely to suppress a second transition. The keyed commit acts as the durable recovery boundary: after a successful commit, later encounters resolve the prior `Commit(T,K)` and recover its retained transaction artifacts without reapplying the transition.

### Transition side effects

A transition may declare publication or request side effects associated with taking that transition.

For replay semantics, these side effects are treated as **implicitly established effect-intent transaction artifacts** when the transition successfully commits. They are not direct external executions inside the application-state transaction.

Therefore transition side effects commit logically with the transition as intents, enter the invocation artifact context, and are subject to the same retention/recovery rules as explicitly established `EffectIntent`s.

The current Rust `TransitionSideEffect` representation may require a structural adjustment so an implicitly established intent has a stable logical identity that can be referenced by a later `ExecuteEffectIntent` step. That is an implementation-shape requirement; it does not change the semantics above.

In particular, consider:

```text
Transaction T
    Transition pending -> paid
        establishes effect intent E
COMMIT

ExecuteEffectIntent E
```

If the invocation crashes after `T` commits but before `ExecuteEffectIntent E`, natural replay cannot be relied on to reproduce `E`, because V1 will not replay the transition transaction naturally. `DeduplicatedBy { key }` ensures that retrying `T` resolves the prior commit and restores `E`, allowing the flow to continue.

This still does not imply exactly-once external execution. Effect-level idempotency/retry analysis remains necessary.

---

## 23. Framework transaction artifacts versus application data

`InvocationResult` and `EffectIntent` are framework-level **logical transaction artifacts**, not inherently durable primitives.

They may participate atomically in a transaction without belonging to the application `DataModel` namespace.

A transaction containing only framework artifact-establishment operations may therefore have `data_model: null`.

Once a transaction reads, writes, locks, inserts, deletes, or transitions an application `DataObject`, its application transactional boundary must be declared.

Artifact durability depends on the replay mechanism:

- a naturally replayable transaction may reconstruct replay-deterministic artifacts;
- a transaction `DeduplicatedBy { key }` must durably retain the exact artifacts of its successful `Commit(T,K)` because its body is not committed again on replay.

This framework retention must not be interpreted as a hidden global transaction spanning arbitrary application data models.

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
| **Deterministic derivation vs replay stability** | The same sources produce the same value, but those source values may differ on retry. |
| **Natural replayability vs at-most-once commit** | Preventing a second commit does not guarantee that a retry can reconstruct the original outcome or artifacts. |
| **Natural replay vs keyed recovery** | Natural replay recomputes the same logical outcome; keyed transaction idempotency resolves a prior durable commit without committing the body again. |
| **Artifact availability vs intrinsic durability** | An artifact may be reconstructed naturally or recovered from a keyed commit; its declaration alone does not imply durable storage. |
| **Transaction read determinism vs read invariance** | A deterministic computation from a read can still change on retry because the observed state may have changed, including due to the transaction itself. |
| **Object identity vs ordering key** | They may coincide, but neither declaration automatically implies the other. |
| **State-machine legality vs replayability** | A legal transition graph does not imply that a transition-containing transaction can be naturally replayed after commit. |
| **Effect-intent recovery vs exactly once** | Recovering the same intent does not establish whether the external effect already occurred or whether another attempt is safe. |

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
