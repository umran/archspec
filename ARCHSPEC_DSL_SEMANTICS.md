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
| **Implementation guarantee / assumption** | A fact the model claims the implementation or external system provides. The verifier may rely on it, subject to implementation conformance. | topic ordering, delivery semantics, dispatch routing, transaction isolation, locks, effect idempotency, concurrency bounds, request/message identity |
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

### `Topic.message_identity`

Message identity is a **guarantee provided by the topic's producers**, declaring where the identity of one logical message lives in the payload.

#### `unspecified`

No fact relates two carried messages sharing any field values.

#### `keyed`

For each mapped message schema, the mapping gives the ordered tuple of fields holding that schema's message identity. As with the ordering key, different schemas may map differently named fields into the same identity domain; tuple positions correspond across schemas, so all mapped tuples must have the same arity.

The guarantee is one statement over the mapped population:

> Any two messages carried by the topic, each of a mapped schema, whose identity tuples are equal are the **same logical message** — hence of the same schema, with equal payloads.

Three consequences are deliberate:

1. Two publications sharing an identity are attempts at publishing one logical message. The declaration says nothing about how often that message is delivered; delivery semantics remain those of §8.2 — which already speak of "the same logical message" being redelivered, and here gain their payload-level anchor.
2. Because equal identity implies same schema, cross-schema identity collisions are excluded. An architect must not place two schemas in one identity domain if distinct logical messages of those schemas can share the identity value.
3. The mapping may cover a subset of the carried schemas. This is a deliberate asymmetry with the ordering key: keyed ordering must route every carried message, while identity is meaningful knowledge per schema.

The message identity is not the ordering key, and it is not object identity. On an order-events topic, `order_id` correctly orders the messages of one order and identifies the *order*; it does not identify the *message*, because `OrderCreated` and `OrderPaid` for one order share it while being different logical messages. The message identity of such a topic is an `event_id`.

Where the publishing operations are modeled, a declared message identity is a checkable claim: a publisher whose publication payload is replay-deterministic under a key propagated into the identity fields (§12, §13) conforms to it. V1 does not perform that check; the declaration is relied on exactly as external-effect idempotency is, subject to §1.3.

Declaring an identity is subject to the §26 authoring rule: declare it only if you are willing for a correctness proof to rely on "same identity value implies same payload". A producer that stamps a fresh timestamp into each publication attempt does not provide the guarantee.

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

### `RequestInput.identity`

Request identity is a **guarantee provided by the request boundary**, declaring where the identity of one logical request lives in the payload.

`unspecified` provides no fact: distinct attempts may present arbitrarily different payloads under equal field values.

`keyed` declares an ordered tuple of payload fields and guarantees:

> Any two requests arriving at this input whose values at the declared identity fields are equal present equal payloads, at the granularity of the modeled schema.

Equivalently, the payload is a function of its identity fields. The canonical conforming implementations are a boundary that rejects a retry whose payload disagrees with the original request under the same identity, and a caller contract strong enough to stand in a proof. A rejected conflicting request is not an admitted invocation of the operation, so rejection preserves the guarantee.

The declaration is an implementation guarantee, not a mechanism: it fixes what the payload of a logical request is, deduplicates nothing, and does not by itself discharge any idempotency requirement.

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

A logical lane dispatches its deliveries in the order they entered it. Affinity therefore preserves the topic's delivery order within a lane; whether dispatched invocations may then overlap is the lane's concurrency.

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

V1 recognizes one precedence source: the order the key's subscription topic declares (§6) — a keyed topic's per-key order, when the ordering key is established to carry the topic key for every admitted schema (the key identity of §4), or a global topic's order for any key. A request input has no precedence source, and a key not sourced from an input selects no population; both are unproven. The mechanism is the §8.2 composition: same-key deliveries enter one lane (`by_topic_key` on a keyed topic, or `single_lane`), a lane dispatches in delivery order, and lane concurrency `bounded(1)` stops overtaking. Because a redelivered earlier message may be processed after a later one, `at_least_once` or unspecified delivery preserves the precedence only when the operation's idempotency requirement keyed from the same input is proven, so that the late duplicate does no distinguishable work; `at_most_once` delivery admits no redelivery. Vacuously discharged: a subscription admitting no message schemas. See `ARCHSPEC_ORDERING_DRAFT.md`.

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

V1 discharges the requirement over each admitted flow — one with no response, or one with the triggering input's response — under the governing key's population (§12). Every transaction step must be retry-safe: a keyed commit over a stable key, or naturally replayable. There is no final-step exemption, because a duplicate delivery re-drives the whole flow even after terminal completion. Every effect-executing step must be duplicate-safe per the §13 rules, since even a recovered intent may be executed again (§14) — and those rules follow the work an attempt causes into other operations: a request is safe only when its target collapses duplicate invocations, a publication only when every modeled consumer collapses duplicate deliveries, each through its own proven requirement. A verdict therefore covers the cascade the operation starts, and V1 computes the mutually dependent verdicts as a least fixpoint. Response consistency is the separate response-replay obligation below. Vacuously discharged: an empty population; no admitted flow, so an attempt performs no modeled work; and a triggering subscription with `at_most_once` delivery whose payload is identity-pinned by the key (§18) — same-class messages are then one logical message delivered at most once, so a class holds at most one attempt. See `ARCHSPEC_EFFECT_SAFETY_DRAFT.md`.

### `ResponseReplayRequirement::replay_consistent`

When replay consistency is required, retries for the same logical invocation must resolve the same logical response.

A response sourced from an `InvocationResult` is replay-consistent only when the solver can establish a safe path to the same logical result. V1 recognizes two principal routes:

1. the establishing transaction is naturally replayable and the result derivation is replay-deterministic; or
2. the establishing transaction is `DeduplicatedBy { key }`, and the exact result produced by the prior successful keyed commit is durably retained and recovered.

`ResponseSource::InvocationResult` does not, by itself, imply durable memoization or transaction idempotency.

### `response: unspecified`

No replay-stability requirement is declared for the response.

This does not waive the operation's idempotency requirement for side effects.

### `RecoverabilityRequirement`

A recoverability requirement keyed by an `IdempotencyKey` means:

> The logical invocation identified by that key must reach terminal execution of a declared flow.

Recoverability is a **progress** obligation. Idempotency is a **safety** obligation. They are deliberately separate requirements because neither implies the other.

An idempotency requirement constrains what repeated attempts may do. It is satisfied vacuously by never retrying at all: an invocation that crashes after its transaction commits and is never re-driven produces no duplicate work, and therefore violates nothing. Idempotency consequently says nothing about whether the remaining steps of an interrupted flow ever execute.

This is exactly the gap left by §14 and §22. Consider:

```text
Transaction T   DeduplicatedBy(K)
    Transition pending -> paid
        establishes effect intent E
COMMIT

<crash>

ExecuteEffectIntent E
```

The keyed commit makes `E` *recoverable*, and the operation's idempotency requirement is satisfied. But nothing in the model yet obliges anyone to come back and execute `E`. The order is durably `paid` and the payment capture never happens. A recoverability requirement is what makes that outcome a declared violation rather than an unremarked silence.

The key identifies the retry-equivalence class in the same sense as §12: attempts sharing the key are attempts at the same logical invocation, so re-driving one of them continues that invocation rather than starting a new one.

The requirement does not name a flow. An invocation takes one of the operation's alternative flows (§7), and a resumed attempt reaching the terminal step of any admitted flow discharges the obligation. Which flows remain admissible after a partial execution is the open question recorded in the revision draft; the requirement is deliberately stated so as not to prejudge it.

### `completion: resumable`

An interrupted attempt must be **able** to resume and drive a declared flow to its terminal step.

For every prefix at which the invocation may fail, the solver must establish that a continuation exists:

- each already-committed transaction resolves on re-encounter, by natural replay or by `Commit(T,K)` (§17);
- every artifact a later step consumes is replay-available by route A or route B of §17;
- no step is left in a state from which the flow cannot proceed.

V1 discharges this by **same-flow continuation**: for every admitted flow — one with no response, or one whose response belongs to the triggering input — re-driving that same flow from its first step must reach its terminal completion. Every transaction step needs re-encounter resolution except one that is the final step of a response-less flow, after which no failing prefix exists. Consumed artifacts are judged by the replay rules of §17 and §18, with references inside the establishing transaction exempt by atomicity, and a commit key judged by the re-encounter analysis rather than double-counted as consumption. This is a sufficient route and deliberately does not prejudge which other flows a resumed attempt may take (`ARCHSPEC_FLOW_RESUMPTION_DRAFT.md`).

`resumable` does **not** oblige the architecture to actually re-drive the invocation. It is the right declaration when the retry driver lies outside the model — most commonly a request input whose caller Archspec does not model.

### `completion: guaranteed`

In addition to resumability, the architecture must guarantee that the logical invocation **is** re-driven until a declared flow terminates.

This is a liveness obligation and additionally requires a modeled retry driver, such as:

- `delivery: at_least_once` on the triggering subscription, or
- an inbound `RequestEffect` whose `retry` is `may_repeat`.

An inbound repeatable request may be declared among a modeled caller's effects or as a state-machine transition side effect, which is a `RequestEffect` under §22. Both driver facts re-drive the *same logical invocation*: a redelivery is another delivery of one logical message, and `may_repeat` repeats one logical request, so the re-driven attempt carries the same payload and hence the same key.

Two cautions apply.

First, the driver facts in the current DSL are duplicate-delivery facts, not bounded-liveness facts. §8.2 states that `at_least_once` "does not encode retry timing, retry count, backoff, or a bounded eventual-delivery liveness guarantee." A `guaranteed` proof is therefore conditional on the delivery abstraction genuinely redelivering until the invocation succeeds, in the sense of §1.3.

Second, a request input alone supplies no driver. The caller is outside the model, so `guaranteed` on a request-only operation is normally not dischargeable unless the calling side is itself modeled as a `may_repeat` request effect.

### Idempotency versus recoverability

A recoverability requirement makes retries *expected* rather than merely *possible*. It therefore strengthens the case for also declaring an idempotency requirement, but the DSL does not couple them:

- **recoverability without idempotency** is coherent where repeating the work is harmless;
- **idempotency without recoverability** is coherent where best-effort completion is acceptable.

The checker does not couple them either, but it says when the coupling is absent: a `guaranteed` recoverability proof for an operation that declares no idempotency requirement keyed from the triggering input carries a warning, because the driver makes retries expected and nothing declares them safe.

Neither implies exactly-once external execution. Driving a flow to termination still leaves the effect-level uncertainty described in §14: an external effect may have succeeded before a crash without that success being durably known.

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

### Reference scope

A value reference is evaluated by some set of invocations, and may only name a source those invocations can actually observe.

The evaluating invocations are determined by where the reference is declared:

- a reference declared within an operation — in its requirements, its effects, or its transactions — is evaluated by invocations of **that operation**;
- a reference declared on a state-machine transition side effect is evaluated by invocations of **whichever operation applies that transition**.

From that scope:

- `input` and `invocation_result` must name declarations of an admitted operation. Another operation's input is never the "current invocation's input payload", and another operation's result is never available to this invocation.
- `effect` must name an effect of an admitted operation, or a transition side effect of a transition an admitted operation applies.
- `state_machine_subject` is unrestricted. State machines are global, and any operation may address the persistent objects they govern.
- `transaction_read` is restricted further, to the transaction execution that produces it. See §18.

This is a structural coherence rule, not a replay-stability claim. A reference being observable says nothing about whether its value is stable across retries.

### `ValueSource::input`

References a field in the current invocation's input payload.

An input reference is not automatically replay-stable merely because two attempts share an idempotency key. Replay stability must follow from the V1 rules of §18: the governing key's own components, a declared request or message identity pinned by that key, artifact recovery or reconstruction, or deterministic derivation over such roots.

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

### Governing keys and the attempt population

When an idempotency, recoverability, or response-replay obligation is verified, its key is the **governing key** of the analysis. V1 analysis proceeds only when every component of a governing key is sourced from **one** input of the operation — the *triggering input* of the analysis. A component sourced from mutable persistent state, or from an artifact the invocation itself produces, cannot define a pre-execution equivalence class, and the obligation is `Unknown`.

The attempt population of such an obligation is the set of invocations triggered by that input. An invocation triggered by a different input has no value for the key, belongs to no equivalence class, and is not constrained by the obligation — the same population reading that applies to serialization and ordering keys, for the §7 reason that a concrete invocation is associated with the input that triggered it.

An empty governing key places every attempt in one class; no component roots exist and no identity can be pinned by it (§18), so essentially nothing is replay-stable relative to it.

`DeduplicatedBy` keys on transactions are not governing keys and are exempt from the single-input restriction; their fitness for artifact recovery is judged by the §18 rules.

### `IdempotencyKeyPropagation`

A propagation declares that the target values carry the same logical idempotency identity as the source values.

This is a **lineage assertion**.

It can bridge renamed fields or different message/request schemas.

Propagation does **not** itself deduplicate anything. It allows the verifier to trace the same logical key across an effect boundary.

V1 reads it on the consumer's side: for a governing key whose population rests on a topic's keyed message identity, each modeled producer of an admitted schema either declares a propagation whose targets cover the identity fields — the identity then carries the producer's key, and when that key is one of the producer's own idempotency requirements, distinct logical invocations of the producer publish distinct messages — or declares none, in which case the identity rests on the topic declaration alone. Both facts are recorded next to the consumer's verdict; neither changes it.

---

## 13. Effects

Effects describe work outside the operation's immediate transaction state.

### Effect contract versus effect instance

An effect declaration is a contract describing what kind of logical work occurs. Depending on effect kind, this includes information such as the destination or target, the schema, retry semantics, idempotency-key propagation, and external idempotency guarantees.

It does not define how the values of a particular effect instance are computed. An effect instance is constructed at an execution or establishment site, and each such site declares the provenance of the values used to construct it:

- a direct `execute_effect` flow step declares `values` (§16);
- an explicit `establish_effect_intent` transaction step declares `values` (§14);
- a `transition` transaction step declares `effect_values`, one derivation per side effect of the applied transition (§22).

`execute_effect_intent` consumes an already-established effect instance and therefore declares no derivation: the instance's values were fixed at establishment (§14).

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

### Duplicate publication

For an upstream idempotency requirement, a duplicate execution of a publication effect is safe exactly when:

1. the topic declares a keyed message identity mapping the published schema (§6) and the published instance is class-fixed — replay-deterministic for a direct execution, or an intent replay-available by route A or B of §17 — so that every attempt publishes the **same logical message**; and
2. every modeled consumer of that message collapses duplicate deliveries of it. A consumer is an operation subscribing to the topic with a message selection admitting the schema; it collapses duplicates either through an idempotency requirement keyed from that subscription that is itself proven, or by receiving the subscription with `at_most_once` delivery, under which one logical message is delivered no more than once however often it is published.

Condition 1 makes the duplicate no new logical work *at the topic*: at most it raises delivery multiplicity. Condition 2 makes it no new logical work anywhere the model can see. Delivery multiplicity is a degree of freedom the topic contract admits, but the work a redelivery causes in a consumer is still work the upstream attempt caused, and the requirement's "must not cause" is transitive: an operation whose retries double a downstream card charge is not idempotent, however faithfully it republishes one message. A consumer the model does not contain is outside the proof, which is conditional on the model's closed world of consumers (§1.3). Producer and consumer verdicts are mutually dependent; V1 computes them together with request discharge (§13.2) as a least fixpoint, and publication cycles settle unproven.

`idempotency_key_propagation` plays no role in this discharge: a class-fixed instance already makes every duplicate payload-equal, so a consumer's key evaluates equally across them whichever fields it reads. Propagation remains lineage for the consumer's analysis (§12) and deduplicates nothing on the publishing side.

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

### Duplicate request

A duplicate execution of a request effect invokes the target again, and nothing admits invocation multiplicity by default — the asymmetry with duplicate publication is deliberate: a request identity on the target input fixes payload consistency, but only a mechanism collapses invocations. The duplicate is safe exactly when the instance is class-fixed, the effect's schema is the targeted input's schema, and the target operation declares an idempotency requirement, keyed from the targeted input, that is itself proven: payload-equal duplicates then fall into one class of that requirement, which collapses them to the work of a single logical invocation. V1 computes these mutually dependent verdicts as a greatest fixpoint: a cycle of requirements that each collapse the others' duplicates is proven, by the minimal-counterexample argument of `ARCHSPEC_EFFECT_SAFETY_DRAFT.md` §4.1, and such proofs are marked coinductive.

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

For an upstream idempotency requirement, a duplicate execution is safe when every component of the declared key is replay-stable relative to the governing key (§18): all attempts then execute under one key, and the boundary collapses them. No instance condition is needed, since the guarantee is scoped to key equality alone.

---

## 14. Effect intents

### `EffectIntent`

An effect intent is a **logical transaction artifact** describing an intended effect execution.

An effect intent is not inherently synonymous with a durable database record, and declaring one does not establish it. `EstablishEffectIntent` establishes the logical artifact as part of a transaction execution.

The current `IntentExecutionSemantics::{Unspecified, Recoverable}` model is superseded by this revision. An intent declaration does not imply an invisible independent executor or independent rediscovery mechanism.

### Intent derivation

Establishing an intent constructs one logical instance of its effect. For `EstablishEffectIntent(I, D)`, where intent `I` refers to effect `E`:

1. one logical instance of `E` is constructed;
2. its values are obtained according to derivation `D`;
3. `I` is established as a transaction artifact representing that exact effect instance.

`EstablishEffectIntent.values` is therefore the provenance declaration for the constructed instance.

If the intent is deterministically derived from replay-stable provenance and the establishing transaction is naturally replayable, a retry may reconstruct the same logical intent without requiring the intent payload itself to have been durably materialized.

If the establishing transaction is explicitly `DeduplicatedBy { key }`, the exact intent produced by the first successful logical commit is retained with that commit and recovered when the transaction step is encountered again under the same key.

### `ExecuteEffectIntent`

A flow step executing an intent performs or attempts the work represented by the logical intent available to the current invocation.

`ExecuteEffectIntent` is the modeled execution authority for the intent. Intent establishment alone does not execute the underlying effect.

The effect instance was already constructed when the intent was established, so `ExecuteEffectIntent` declares no derivation and must never recompute or replace the intent's values.

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

For `ExecuteEffect(E, D)`, the step:

1. constructs one logical instance of effect `E`;
2. obtains its values according to derivation `D`;
3. executes that effect instance.

`values` declares the provenance of the complete logical effect instance, for every effect kind, using the same `Derivation` vocabulary as transaction-level provenance declarations (§18). Unknown provenance must be declared explicitly as `unspecified` rather than omitted.

Because the step occurs at flow level rather than inside a transaction, the derivation is evaluated in the operation-level value context. It may not reference `transaction_read` results, which are local to a transaction execution (§18).

For natural replay idempotency, the analyzer must prove `D` replay-deterministic: `deterministic` plus replay-stable provenance roots (§18) establishes that a retry constructs the same logical effect instance. Effect payload stability and duplicate-execution safety remain separate proof obligations. Validation checks only that the derivation's references and field paths are structurally coherent; replay stability is solver responsibility.

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

Route A's derivation roots and route B's commit-key components are judged by the replay-stability rules of §18. In particular, a commit key over roots that are not replay-stable earns no recovery route: attempts may evaluate different keys, address different commits, and each commit the body once.

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

### Replay-stable provenance roots (V1)

Replay stability is judged relative to an operation and a governing key
`K` (§12):

> A `ValueRef` is **replay-stable** iff in every admitted execution,
> any two attempts in the same `K`-class that evaluate it obtain equal
> logical values.

The quantification is over evaluations: an attempt that crashes before
evaluating a reference imposes nothing, and evaluating an artifact
reference presupposes the artifact is available in the attempt's
context (§16).

A path `p` is *pinned by `K` in schema `S`* when some component of `K`
has a path canonically equal to `p` within `S` — equality of canonical
value paths after fragment expansion, since a fragment mapping asserts
semantic identity of the referenced value (§4).

The V1 rules. Stability is definitional, declared, or derived — never
assumed:

1. **Key components.** Every component of `K` is replay-stable: class
   membership requires their equality (§12).
2. **Literals.** A literal is replay-stable; this matters for selector
   provenance (§19).
3. **Identified triggering payload.** Let `i` be the triggering input.
   - `i` is a request declaring a keyed identity (§8.1) whose every
     field is pinned by `K`: every field of `i`'s payload is
     replay-stable.
   - `i` is a subscription whose topic declares a keyed message
     identity (§6), every admitted schema is mapped, and for each
     identity position a **single** component of `K` pins that
     position's field in **every** admitted schema: every field of
     `i`'s payload is replay-stable.

   Same-class attempts are then presentations of one logical stimulus.
   The per-position single-component clause is what carries key
   equality across schemas: with different components pinning a
   position in different schemas, class equality would relate each
   component to itself across attempts but never relate one message's
   identity to the other's.
4. **Recovered artifacts.** For a transaction `T` declared
   `DeduplicatedBy { key }` whose key components are all replay-stable,
   the contents of every artifact established by `T` are replay-stable:
   all attempts in a class address the same `Commit(T,K)`, which
   durably retains the exact artifacts of the single successful
   execution (§17 route B).
5. **Reconstructed artifacts.** For a naturally replayable transaction,
   an artifact whose establishment derivation is replay-deterministic
   is replay-stable (§17 route A).
6. **Congruence.** A value produced by `Deterministic { from }` with
   every root replay-stable is replay-deterministic, and its uses
   inherit stability.
7. **Everything else is `Unknown`**: unidentified non-key input fields,
   fields of a non-triggering input, `state_machine_subject` state
   (always, in V1), `effect` payload roots, and `transaction_read`
   results, which additionally poison any natural-replay provenance
   closure that reaches them.

The `Unknown` cases are epistemic (§1.1): no rule establishes
stability; instability is not proven.

Why rule 3 requires a declaration rather than following from the key:
with `K = [input.idempotency_key]` and no declared identity, attempts
`{idempotency_key: k, amount: 100}` and `{idempotency_key: k, amount:
200}` are both admitted and share a class. A write derived
`deterministic_from(input.amount)` is deterministic yet produces
different values across the class. Only a boundary fact excludes the
conflicting attempt.

These judgments — stability, replay determinism, natural
replayability, artifact replay availability — form one simultaneous
induction, computable in a single forward pass in flow order and,
within a transaction, step order: every rule consumes only roots or
facts established at earlier steps, and transaction-read dependence,
the only backward-looking observation, is excluded outright.

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

A deterministic derivation describes value computation, not replayability by itself. Natural replay analysis must additionally establish replay stability of the selected target and all derivation roots (§18).

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

The current DSL therefore cannot declare a deadlock-safe acquisition of several specific instances of one object: a selector admits no disjunction, so one lock step cannot name them, and no fact orders separate steps. The locking facts the DSL lacks are open question 8 of `ARCHSPEC_SEMANTICS_REVISION_DRAFT.md` §27, and the model-wide deadlock checker that would consume them is question 9; no V1 verifier reasons about locks.

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

An implicitly established intent needs a stable logical identity so a later `ExecuteEffectIntent` step can name it. That identity is supplied by an operation-level `EffectIntent` whose `effect` is the transition side effect. The operation declares the handle; the transition establishes the artifact.

Two rules keep that identity well defined:

1. **Uniqueness.** An operation may declare at most one `EffectIntent` for a given transition side effect. A transition establishes exactly one logical intent per side effect, so two competing declarations would leave no rule for deciding which one the commit fills.
2. **Establishability.** An operation may declare such an intent only if one of its transactions applies the owning transition. Otherwise nothing in the operation could ever establish it, and the handle names an artifact that never enters the invocation artifact context.

Neither rule constrains explicitly established intents. Two `EffectIntent`s may name the same operation-owned effect, because each is established by its own `EstablishEffectIntent` step and the two are therefore distinguishable.

Conversely, a transition side effect must **not** be established explicitly, and must not be executed by a direct `ExecuteEffect` step. Establishment is the transition's, and execution is `ExecuteEffectIntent`'s.

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

### Transition effect values

The state-machine transition owns the effect contract; the applying `transition` transaction step owns the concrete instance provenance. `StateTransition.effect_values` maps each side effect declared by the applied transition to the `Derivation` used to construct that side effect's instance when the transition is applied.

The mapping must be exact:

```text
transition.side_effects.keys()
==
state_transition.effect_values.keys()
```

Missing derivations, extra derivations, and derivations keyed by another transition's effects are all structural validation errors. A transition without side effects declares an empty map. Unknown provenance must be declared explicitly as `unspecified`; the validator does not synthesize missing entries, preserving the distinction between provenance intentionally unspecified and a provenance declaration accidentally missing.

Each derivation is evaluated in the enclosing transaction context at the point of the `transition` step. It may therefore reference valid operation-level values, available invocation results, and preceding `transaction_read` results, subject to the usual read-before-use and field-selection rules (§18). It is not evaluated in a static state-machine-transition context, because its values belong to a concrete transaction application of the transition.

A successful transition transaction logically performs the following atomically:

1. evaluate the state-transition guard;
2. apply the state transition;
3. construct each transition side-effect instance using its corresponding `effect_values` derivation;
4. implicitly establish the corresponding effect-intent artifacts;
5. commit the transition state and established artifacts together.

Because every transition-containing transaction declares `DeduplicatedBy { key }` in V1, these derivations are evaluated only during the first successful keyed execution. A retry with the same transaction idempotency identity resolves `Commit(T,K)` and recovers the exact original artifacts without evaluating the derivations again. This is what permits transition effect values to depend on transaction-local reads even though those reads may not be replay-stable.

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
| **Idempotency vs recoverability** | Idempotency bounds what retries may do and is satisfied by never retrying; recoverability obliges the flow to actually reach its terminal step. |
| **Resumable vs guaranteed completion** | Being able to resume is a property of the flow's artifacts; being re-driven requires a modeled retry driver. |
| **Duplicate-delivery fact vs liveness** | `at_least_once` and `may_repeat` say a retry may happen, not that retries continue until success. |
| **Ordering key vs message identity** | The ordering key sequences messages; the message identity identifies one logical message. They may coincide; neither implies the other. |
| **Object identity vs message identity** | `order_id` identifies the order, not the message about the order. |
| **Key equality vs payload equality** | Class membership equates the governing key's components only; payload equality needs a declared stimulus identity pinned by that key. |
| **Stimulus identity vs deduplication** | An identity fixes what the payload of a logical request or message is; only a mechanism limits how often work happens. |

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
