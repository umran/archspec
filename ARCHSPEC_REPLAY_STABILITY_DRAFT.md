# Archspec Replay-Stable Provenance Roots (V1)
## Resolution Draft for Revision Question 3

**Status:** Accepted 2026-08-20. Resolves open question 3 of `ARCHSPEC_SEMANTICS_REVISION_DRAFT.md` §27; reconciled into `ARCHSPEC_DSL_SEMANTICS.md` (§1, §6, §8.1, §11, §12, §16, §17, §18, §20, §24) and implemented (`RequestIdentity`, `MessageIdentity`, validation) the same day. Retained as the detailed rationale.
**Date:** 2026-08-20
**Scope:** The exact V1 rules for when a `ValueRef` is replay-stable across attempts of the same logical invocation; two new boundary declarations those rules consume; validation obligations; consequences for the idempotency, recoverability, and response-replay analyses.

---

## 1. The question

The composition rule at the center of the revised replay model is:

```text
deterministic derivation
        +
replay-stable provenance
        ↓
replay-deterministic produced value
```

`Derivation::Deterministic { from }` supplies the first premise. Nothing
currently defines the second. The main semantics document cites
"replay-stable provenance roots" in §11, §16, §18, and §20 as a judgment
the solver applies, and §11 constrains it:

> An input reference is not automatically replay-stable merely because
> two attempts share an idempotency key. Replay stability must follow
> from the operation's declared idempotency equivalence or other
> established provenance facts.

but the judgment itself is undefined. Until it is defined, natural
transaction replayability (revision §22.1), artifact reconstruction
(revision §23 route A), response replay (revision §18 route 1), and
direct-execution replay determinism (§16) are all unverifiable, because
each bottoms out in root stability.

This draft defines the judgment. The design principle, dictated by §1.1
of the semantics contract, is:

> Stability is definitional, declared, or derived. It is never assumed.

- **Definitional**: the components of the governing equivalence key are
  equal across attempts because class membership requires it.
- **Declared**: a boundary declaration asserts that stimuli sharing a
  declared identity are the same logical stimulus and therefore present
  the same payload.
- **Derived**: values reconstructed or recovered through the §17
  artifact routes, and values produced by deterministic derivation from
  roots that are themselves stable.

Everything else is `Unknown`.

---

## 2. The judgment being defined

### 2.1 Governing key and attempt population

Replay analysis is always relative to an operation `O` and a governing
equivalence key `K` — the `IdempotencyKey` of the idempotency,
recoverability, or response-replay obligation under proof.

**Rule R0 (governing-key admissibility).** V1 analysis proceeds only
when every component of `K` is a `ValueRef` sourced from **one** input
of `O`. That input is the *triggering input* of the analysis.

If any component of `K` names another source, the obligation's verdict
is `Unknown`, with that as the reason. A component sourced from mutable
persistent state or from an artifact the invocation itself produces
cannot define a pre-execution equivalence class: the "identity" of the
attempt would depend on when it is observed, and class membership would
not be a stable fact about the attempt.

The *attempt population* of the analysis is the set of invocations of
`O` triggered by the triggering input. An invocation triggered by a
different input has no value for `K`, is a member of no equivalence
class, and is not constrained by the obligation. This is the same
population reading the serialization checker applies to serialization
keys, for the same §7 reason: a concrete invocation is associated with
the input that triggered it, and a `ValueRef` whose source is an input
refers to the payload of that triggering input.

Two attempts in the population are in the same class exactly when all
components of `K` are equal in declared component order (§12).

An empty `K` places every attempt in one class. No component roots
exist, and no identity can be pinned by it (§4), so essentially nothing
is stable relative to an empty key. This is consistent with §12
assigning no special meaning to an empty component list; obligations
keyed by an empty key are `Unknown` in practice.

**Note.** `DeduplicatedBy` keys on transactions are *not* governing
keys and are exempt from R0. They are evaluated at their transaction
step and may name any structurally valid source; their fitness for the
recovery route is judged by rule R4 below.

### 2.2 The judgment

> Relative to `(O, K)`: a `ValueRef` `r` is **replay-stable** iff in
> every admitted execution, any two attempts in the same `K`-class that
> evaluate `r` obtain equal logical values.

The quantification is over evaluations. An attempt that crashes before
evaluating `r` imposes nothing; an attempt that evaluates `r` twice at
different flow points observes the class-determined value both times or
the reference is not stable.

Stability composes upward through the existing rule: a value produced
by `Deterministic { from }` whose roots are all replay-stable is
*replay-deterministic* — every attempt in the class that produces it
produces the same logical value.

---

## 3. Why stability cannot be automatic

Consider a request-driven operation with idempotency key
`[input.idempotency_key]` and a transaction write:

```text
Write Account.balance
  values: deterministic_from(input.amount)
```

Attempt 1 presents `{idempotency_key: k, amount: 100}`.
Attempt 2 presents `{idempotency_key: k, amount: 200}`.

Both are admitted: nothing in the model excludes a caller that reuses a
key with different parameters. The two attempts are in the same class,
and the derivation is deterministic, yet the produced values differ.
Treating `input.amount` as stable would let the solver prove natural
replayability for a transaction whose retry writes a different balance.
This is the failure §11's sentence exists to prevent, and it is why the
V1 rules require a *declared boundary fact* before extending stability
beyond the key components.

The declarations in §4 make that fact explicit, falsifiable, and
declared at the boundary that provides it — exactly the treatment
`ExternalEffect.idempotency` already gives to the other direction of
the same problem.

---

## 4. Boundary identity declarations

Both declarations instantiate one concept:

> **Stimulus identity.** A declared identity names the payload fields
> that identify one logical stimulus (one logical request, one logical
> message). Two stimuli sharing an identity are the same logical
> stimulus, and a logical stimulus has one payload.

This is an **implementation guarantee** in the §1 sense: the verifier
may rely on it, and every proof that uses it is conditional on the
boundary actually providing it (§1.3). The authoring rule of §26
applies with full force: declare an identity only if you are willing
for a correctness proof to rely on "same identity value implies same
payload".

### 4.1 `RequestIdentity` on request inputs

```rust
pub struct RequestInput {
    pub schema: Id,
    pub identity: RequestIdentity,
}

pub enum RequestIdentity {
    Unspecified,
    Keyed { fields: Vec<FieldPath> },
}
```

#### `unspecified`

No fact relates two requests sharing any field values. Distinct
attempts may present arbitrarily different payloads under equal keys.

#### `keyed`

> Any two requests arriving at this input whose values at the declared
> identity fields are equal present equal payloads.

Equivalently: the request payload is a function of its identity fields,
at the granularity of the modeled schema.

The canonical conforming implementations are a boundary that rejects a
retry whose payload disagrees with the original request under the same
identity (parameter-hash enforcement), and a caller contract strong
enough to stand in a proof. A rejected conflicting request is not an
admitted invocation of the operation, so rejection preserves the
guarantee.

`identity.fields` is an ordered tuple of paths on the request schema.
It declares where request identity lives; it does not deduplicate
anything, does not imply the server retains requests, and does not by
itself discharge any idempotency requirement.

### 4.2 `MessageIdentity` on topics

```rust
pub struct Topic {
    pub messages: BTreeSet<Id>,
    pub ordering: TopicOrdering,
    pub message_identity: MessageIdentity,
}

pub enum MessageIdentity {
    Unspecified,
    Keyed { mapping: BTreeMap<Id, Vec<FieldPath>> },
}
```

#### `unspecified`

No fact relates two carried messages sharing any field values.

#### `keyed`

For each mapped message schema, `mapping` gives the ordered tuple of
fields holding that schema's message identity. As with the ordering
key, different schemas may map differently named fields into the same
identity domain, and tuple positions correspond across schemas: all
mapped tuples SHALL have the same arity.

The guarantee is a single statement over the mapped population:

> Any two messages carried by the topic, each of a mapped schema, whose
> identity tuples are equal are the **same logical message** — hence of
> the same schema, with equal payloads.

Three consequences are deliberate:

1. **Publication-side deduplication of identity, not of delivery.**
   Two publications sharing an identity are attempts at publishing one
   logical message. The declaration says nothing about how many times
   that message is delivered; delivery semantics remain §8.2's.
2. **The delivery vocabulary gains its missing anchor.** §8.2 already
   speaks of "the same logical message" being redelivered. Message
   identity states where, in the payload, that sameness is observable.
3. **Cross-schema identity collisions are excluded.** Because equal
   identity implies same schema, an architect must not place two
   schemas in one identity domain if distinct logical messages of those
   schemas can share the identity value.

The third consequence carries the sharpest authoring pitfall, visible
in the flash-checkout fixture: `topic.order_events` orders by
`order_id`, and `OrderCreated` and `OrderPaid` for one order share that
`order_id` while being different logical messages. `order_id` is the
identity of the *order*, and a valid **ordering key**; it is not the
identity of the *message*. The message identity of that topic is
`event_id`. Object identity, ordering key, and message identity are
three declarations that may coincide but never imply one another.

A mapping MAY cover a subset of the topic's carried schemas. This is a
deliberate asymmetry with the ordering key: keyed *ordering* must route
every carried message and therefore requires a total mapping, while
identity is meaningful knowledge per schema. Messages of unmapped
schemas are simply outside the guarantee.

#### Producer conformance

Where the publishing operations are themselves modeled, a declared
message identity is a checkable claim rather than only an assumption: a
publisher whose publication payload is replay-deterministic under a key
propagated into the identity fields (§12, §13) conforms to it. V1 does
not perform this check; see §8. Until it does, the declaration is
relied on exactly as `deduplicated_by` on an external effect is.

### 4.3 Validation obligations

Structural validation SHALL enforce:

1. `RequestIdentity::Keyed.fields` is non-empty, and every path
   resolves against the request schema.
2. `MessageIdentity::Keyed.mapping` keys are schemas carried by the
   topic (mirror of `TopicKeySchemaNotOnTopic`).
3. Every mapped identity tuple is non-empty, its paths resolve against
   its schema, and all mapped tuples have equal arity
   (`MessageIdentityArityMismatch`).

Candidate error additions:

```text
EmptyRequestIdentity
EmptyMessageIdentity
MessageIdentitySchemaNotOnTopic
MessageIdentityArityMismatch
```

---

## 5. The V1 stability rules

Throughout, *canonical comparison* of field paths within a schema means
equality of canonical value paths after fragment expansion, per §4 of
the semantics contract: a fragment mapping asserts semantic identity of
the referenced value, so aliased paths denote one logical value.

> A path `p` is **pinned by `K` in schema `S`** when some component `c`
> of `K` (necessarily sourced from the triggering input, by R0)
> satisfies: `c.path` and `p` are canonically equal within `S`.

### R1 — key components

Every component of `K` is replay-stable relative to `(O, K)`.

*Soundness.* Two attempts are in the same class exactly when all
components are equal (§12). R0 guarantees the components are fields of
the triggering payload, which is fixed per attempt, so each attempt has
one value for each component and the class fixes it.

### R2 — literals

A literal is replay-stable. This matters for selector provenance
(§19), where predicates mix literals and `ValueRef`s.

### R3 — identified triggering payload

Let `i` be the triggering input.

**R3a (request).** If `i` declares `RequestIdentity::Keyed { fields }`
and every field of the tuple is pinned by `K` in the request schema,
then every field of `i`'s payload is replay-stable.

**R3b (subscription).** Let `i` subscribe to topic `t` and admit
message schemas `S₁ … Sₘ` (its selector's schemas, or all of `t`'s
messages under `all`). If:

1. `t` declares `MessageIdentity::Keyed { mapping }`;
2. every admitted schema is mapped; and
3. for each identity position `j`, a **single** component `c` of `K`
   pins `mapping[S][j]` in `S` for **every** admitted `S`;

then every field of `i`'s payload is replay-stable.

*Soundness of R3b.* Take attempts `a₁`, `a₂` in one class, triggered by
messages `m₁ : S₁`, `m₂ : S₂`. For each position `j`, with `cⱼ` the
pinning component: `m₁`'s identity at `j` equals `m₁`'s value at
`cⱼ.path` (canonical equality in `S₁`), which equals `m₂`'s value at
`cⱼ.path` (class equality on component `cⱼ`), which equals `m₂`'s
identity at `j` (canonical equality in `S₂`). The identity tuples are
equal, so by the declared guarantee `m₁` and `m₂` are the same logical
message: same schema, equal payloads. Every payload reference then
evaluates equally. R3a is the single-schema collapse of the same
argument.

The per-position "single component across all admitted schemas" clause
is not decoration. If different components pinned position `j` in
different schemas, class equality would relate each component to
itself across attempts but never relate `m₁`'s identity to `m₂`'s, and
the cross-schema step of the argument would fail.

*Redelivery.* Under R3b, at-least-once redelivery needs no separate
rule: redeliveries of one logical message trivially share its identity
and payload. The rule's force is that it also covers duplicate
*publications*, which redelivery reasoning alone cannot.

### R4 — recovered artifacts

Let `T` be a transaction of `O` with
`idempotency: DeduplicatedBy { key: K' }`, and let every component of
`K'` be replay-stable relative to `(O, K)` under these rules. Then for
every artifact established by `T` — each `InvocationResult`, each
explicitly established `EffectIntent`, and each transition side-effect
intent — the artifact's contents are replay-stable: references through
`ValueSource::InvocationResult`, and the effect instance consumed by
`ExecuteEffectIntent`, evaluate to the values fixed by the single
successful `Commit(T, K')`.

*Soundness.* `K'`-stability makes every attempt in the class evaluate
the same `K'`, hence address the same logical commit identity. At most
one `Commit(T, K')` ever succeeds (§17), it durably retains the exact
artifacts of that execution, and later encounters restore those exact
artifacts without re-executing the body. Any attempt that evaluates a
reference into such an artifact does so after the artifact is available
in its context — production, or recovery of the single commit — and in
both cases observes the same retained values.

*Remark (self-defeating keys).* Validation already evaluates a
`DeduplicatedBy` key in the operation-level context — the commit key is
evaluated for the invocation before the body executes, so it may not
observe transaction state — which rules the `TransactionRead` case out
structurally. The case that survives validation is a key over
unidentified non-key input fields: it is structurally coherent, but its
components are not replay-stable, so R4 does not apply. Attempts in one
class may evaluate different `K'`, address different commits, and each
commit the body once. The durable mechanism is only as good as the
stability of its key.

### R5 — reconstructed artifacts

Let `T` be naturally replayable under the V1 natural route (revision
§22.1: no `Transition`, no relevant provenance reaching a
`TransactionRead`, mutation targets and values replay-deterministic),
and let artifact `A`'s establishment derivation be replay-deterministic
(deterministic, with all roots replay-stable under these rules). Then
references into `A` are replay-stable.

*Soundness.* This is §17 route A: a class-equivalent re-execution
reproduces the same logical outcome, and the artifact derivation
reproduces the same artifact values from class-fixed roots.

### R6 — congruence

A value produced by `Derivation::Deterministic { from }` with every
root replay-stable is replay-deterministic. Where such a value is
itself referenceable — artifact contents under R4/R5, mutation values
and selector targets inside natural-replay analysis, a direct
`ExecuteEffect` instance at flow level (§16) — its references and uses
inherit stability from this rule.

### R7 — everything else

| Root | V1 judgment |
|---|---|
| Component of `K` | Stable (R1). |
| Literal | Stable (R2). |
| Triggering-input payload field | Stable only under R3; otherwise `Unknown`. |
| Field of a non-triggering input | Not evaluable by the population; unusable, and any obligation resting on it is `Unknown`. |
| `invocation_result` | Stable under R4 or R5; otherwise `Unknown`. |
| `effect` | `Unknown`. An effect instance is constructed per execution site; V1 does not treat its payload as an observable stable root. Intent contents are covered as artifacts (R4/R5) through `ExecuteEffectIntent`, which consumes the instance whole. Idempotency-key *propagation* declarations are unaffected: they are lineage assertions, not evaluated roots (§12). |
| `state_machine_subject` | `Unknown`, always. Mutable persistent state may change between attempts, including through the operation's own committed work; V1 attempts no invariance analysis, for the §18 reasons. |
| `transaction_read` | Never stable, and poisons any natural-replay provenance closure that reaches it. Already normative (§18); restated here for completeness of the table. |

The `Unknown` rows are epistemic (§1.1): they mean no V1 rule
establishes stability, not that instability is proven.

### 5.1 How the rules are computed

The stability judgment, replay-determinism of derivations, natural
transaction replayability, and artifact replay availability are one
simultaneous induction. It is well-founded and needs no fixpoint: a
single forward pass in flow order (and, within a transaction, step
order) suffices, because every rule consumes only roots (R1–R3, R7) or
facts established at earlier steps (R4–R6), and `TransactionRead`
dependence — the only backward-looking observation — is conservatively
excluded outright.

---

## 6. What the rules discharge

With the root judgment defined, the previously blocked analyses become
mechanical:

- **Natural replayability** (revision §22.1): mutation targets and
  values are replay-deterministic iff their derivations are
  deterministic over R1–R6-stable roots, with the existing `Transition`
  and `TransactionRead` exclusions.
- **Artifact replay** (revision §23): route A is R5; route B is R4.
- **Response replay** (revision §18): route 1 is R5 applied to the
  result-establishing transaction plus R6 on the result derivation;
  route 2 is R4.
- **Direct executions** (§16): an `ExecuteEffect` derivation is
  replay-deterministic iff deterministic over stable roots — R6 at flow
  level, where `transaction_read` roots are already structurally
  excluded.
- **Recoverability** (`resumable`): every artifact a post-crash
  continuation consumes must be replay-available via R4 or R5; the
  re-driven attempt's own roots are governed by the same table.

---

## 7. Worked examples

### 7.1 The naturally replayable file write (revision §26.1), completed

```text
operation.write_file
  input.request : request, schema { request_id, file_id, contents }
    identity: keyed [request_id]

  requirements.idempotency
    key: [input.request.request_id]

  Transaction T (idempotency: unspecified)
    Write File.contents
      target: File[id = input.file_id]
      values: deterministic_from(input.contents)
    EstablishInvocationResult R
      values: deterministic_from(input.file_id)
```

Governing `K = [request_id]`. R0: admissible. R1 makes `request_id`
stable. R3a: the declared identity `[request_id]` is pinned by `K`, so
`file_id` and `contents` are stable. R6: the write target and value and
the result derivation are replay-deterministic; the natural route
proves `T` replayable and R5 makes `R` stable. The example's "if the
solver proves `input.file_id` and `input.contents` replay-stable" is
now discharged — and **without** the `identity` declaration it is
`Unknown`, which is the poison-retry counterexample of §3 doing its
job.

### 7.2 Message identity on the order-events topic

```text
topic.order_events
  ordering: keyed by order_id          # unchanged
  message_identity:
    keyed:
      schema.OrderCreated:  [event_id]
      schema.PaymentCaptured: [event_id]
      ...
```

For `operation.reserve_inventory` (subscription admitting
`OrderCreated`, idempotency key `[input.event_id]`): R3b pins the
identity, so the whole message payload — `order_id`, `warehouse_id`,
`sku`, `quantity`, `amount` — is stable. The intent derivation
`deterministic_from(order_id, event_id)` is replay-deterministic.

The operation's idempotency obligation as a whole still does not prove:
its transaction is `not_deduplicated` and its stock write derives from
a transaction read, so natural replay is `Unknown` (§18). The rules
compose without overclaiming — root stability was the missing premise,
not the whole proof.

### 7.3 Cross-schema pinning

A subscription admits `OrderCreated` and `OrderCancelled`; both are
mapped with identity `[event_id]`; the governing key is
`[input.event_id]`. Position 1 is pinned by the same component in both
schemas: R3b holds, and same-class attempts are deliveries of one
logical message even across the two schemas.

Variant: `OrderCancelled` declares identity `[id]` while the key
remains `[input.event_id]`. No single component pins position 1 in
both schemas — for `OrderCancelled`, `event_id` is not its identity
field — so R3b fails and non-key payload fields of the subscription are
`Unknown`. Two distinct cancellation messages could share `event_id`
values without being the same logical message.

### 7.4 A self-defeating deduplication key

```text
operation.charge
  input.request : request, schema { idempotency_key, amount }
    identity: unspecified

  requirements.idempotency
    key: [input.request.idempotency_key]

  Transaction T
    idempotency: deduplicated_by [input.request.amount]
    ...
    EstablishInvocationResult R
```

The model validates: the commit key is an operation-level value, as
required. But relative to the governing key, `input.amount` is an
unidentified non-key field — no request identity is declared — so it is
not replay-stable (R7) and R4 does not apply. Two attempts sharing
`idempotency_key` may present different amounts, evaluate different
commit keys, address different commits, and each commit the body once.
`R` is not replay-stable, and any response-replay obligation resting on
it is `Unknown`. The durable mechanism was real; its key made it
useless. Declaring `identity: keyed [idempotency_key]` — or keying the
commit by the idempotency key itself — repairs it.

---

## 8. What V1 deliberately does not infer

1. **Producer-lineage discharge of message identity.** A future solver
   may verify a declared `MessageIdentity` against modeled producers —
   publications keyed by propagated idempotency identity (§12, §13)
   with replay-deterministic payload derivations — turning the
   assumption into a checked conclusion, and may even infer stability
   without the declaration by tracing lineage across the topic. V1
   consumes only the declaration, but since 2026-08-22 it also reads
   the declared propagations on the consumer's side and records, per
   producer, whether the identity is carried by the producer's key
   (§12 of the main document); the record informs the verdict's
   reader without changing the verdict. Inferring stability from that
   lineage remains out of scope.
2. **Read invariance.** No `R(S) = R(T(S))` analysis; `TransactionRead`
   remains excluded (§18).
3. **Subject invariance.** No lifecycle or immutability analysis makes
   a `state_machine_subject` root stable, even under proven
   serialization.
4. **Cross-input equivalence.** A logical invocation reachable through
   two inputs cannot share one equivalence class in V1, because a
   governing key names one input's payload (R0). This mirrors the
   serialization population rule and is a DSL expressiveness limit, not
   a checker choice.
5. **Effect-payload roots.** An `effect`-sourced derivation root stays
   `Unknown` even when the instance's own derivation is
   replay-deterministic; referencing instance payloads as roots needs a
   scoping story first (which instance, at which site).
6. **Flow applicability.** The stability judgment quantifies over
   evaluations, and an evaluation of an artifact reference presupposes
   the artifact is available in the attempt's context (§16). Which
   flows remain admissible for a resumed attempt after a partial
   execution — and therefore which evaluations can occur — is revision
   question 7, which this document deliberately does not prejudge.

---

## 9. Candidate structural diff

```rust
pub struct RequestInput {
    pub schema: Id,
    pub identity: RequestIdentity,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RequestIdentity {
    Unspecified,
    Keyed { fields: Vec<FieldPath> },
}

pub struct Topic {
    pub messages: BTreeSet<Id>,
    pub ordering: TopicOrdering,
    pub message_identity: MessageIdentity,
}

#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageIdentity {
    Unspecified,
    Keyed { mapping: BTreeMap<Id, Vec<FieldPath>> },
}
```

Both additions are backward-visible in YAML as new required fields with
an `unspecified` variant; existing fixtures gain
`identity: {kind: unspecified}` / `message_identity: {kind:
unspecified}` (or serde defaults, at the implementer's discretion —
the DSL convention of explicit `unspecified` over `Option` suggests
requiring them).

Verification-side vocabulary this unblocks (sketch, for the idempotency
slice):

```text
ReplayStability = Stable(rule) | Unknown(reason)

obstacles:
  GoverningKeyNotFromSingleInput
  RootNotReplayStable { root, reason }
  IdentityNotDeclared { input | topic, schema }
  IdentityNotPinnedByKey { schema, position }
  DeduplicationKeyNotStable { transaction, root }
```

---

## 10. Reconciliation checklist

Executed in full, 2026-08-20. The verification-side vocabulary sketched
in §9 first landed on 2026-08-21 with the replay engine
(`analyzer::verification::replay`) and the response-replay checker;
the remaining obstacles land with the idempotency slice.

1. **Main document §6**: add the `message_identity` subsection after
   `TopicKey.mapping`, including the ordering-key/message-identity
   distinction and the order-events pitfall.
2. **Main document §8.1**: add `RequestIdentity` semantics; note that a
   request input still encodes no transport or caller facts beyond it.
3. **Main document §11 (`ValueSource::input`)**: replace "or other
   established provenance facts" with a reference to these rules.
4. **Main document §12**: add R0 (governing-key admissibility and
   population); keep the empty-key caution and tie it to R0's note.
5. **Main document §16/§17/§18/§20**: where "replay-stable provenance"
   is cited, point at the rule set; restate route A/B as R5/R4.
6. **Main document §24**, new distinction rows:
   - *Ordering key vs message identity* — one sequences messages, the
     other identifies a logical message; neither implies the other.
   - *Object identity vs message identity* — `order_id` identifies the
     order, not the message about the order.
   - *Key equality vs payload equality* — class membership equates key
     components only; payload equality needs a declared identity.
   - *Stimulus identity vs deduplication* — an identity fixes what the
     payload is; only a mechanism (`deduplicated_by`, at-most-once
     boundaries) limits how often work happens.
7. **Revision draft §27**: mark question 3 resolved by this document;
   revisit example §26.1 to carry the `identity` declaration it now
   requires.
8. **Validation**: implement §4.3.
9. **Fixtures**: extend `flash_checkout.yaml` with `message_identity`
   on `topic.order_events` (`event_id` per schema) and
   `identity: keyed [idempotency_key]` on `input.create_order.request`.

---

## 11. Normative summary

> Replay stability is judged relative to a governing equivalence key
> whose components all name one triggering input's payload. The key's
> components are stable by definition of the class. The rest of that
> payload is stable only when a declared boundary identity — request
> identity or topic message identity — is pinned by the key, making
> same-class attempts presentations of one logical stimulus. Artifact
> contents are stable when recovered from a keyed commit whose key is
> itself stable, or reconstructed by a naturally replayable transaction
> with a replay-deterministic derivation. Deterministic derivation over
> stable roots is stable. Mutable persistent state, transaction reads,
> effect payloads, and unidentified input fields are not.

> A declared identity is an implementation guarantee, not a mechanism:
> it fixes what the payload of a logical stimulus is, deduplicates
> nothing, and every proof that uses it is conditional on the boundary
> honoring it.
