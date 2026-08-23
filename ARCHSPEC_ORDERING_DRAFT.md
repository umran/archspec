# Archspec Ordering (V1)
## A Precedence-Source Stance for Ordering Requirements

**Status:** Accepted 2026-08-22. States the V1 analysis for
`OrderingRequirement` and reconciles it into `ARCHSPEC_DSL_SEMANTICS.md`
§8.2 and §9. Implemented the same day
(`analyzer::verification::ordering`).

---

## 1. The requirement and what a proof owes

An ordering requirement keyed by a `ValueRef` means (§9):

> Same-key invocations for which a meaningful logical precedence
> exists must preserve that precedence through the operation's
> semantically relevant execution.

Two things distinguish it from serialization. A proof must say *where
the precedence comes from* — arbitrarily serializing concurrent
inputs "cannot invent a semantic precedence" — and it must show the
mechanism *preserves* it, including whatever serialization stops a
later invocation overtaking an earlier one. The open item that kept
ordering unverified was the first: the semantics named no precedence
source. This document names one.

---

## 2. The precedence source: the topic's declared order

The DSL declares exactly one ordering fact anywhere: `Topic.ordering`
(§6). A `keyed` topic puts same-key messages into one ordered sequence
per key; a `global` topic puts every message into one sequence. Both
are guarantees at the topic boundary, and both are *observed*
sequences — the order in which the topic delivers.

V1 takes that sequence as the logical precedence of the invocations it
triggers:

> For an ordering requirement keyed from a subscription input, the
> precedence among same-key invocations is the order the subscribed
> topic declares among the messages that trigger them.

For a keyed topic this is a precedence *for the ordering key* only when
the ordering key is established to carry the topic's key for every
admitted schema — the same key identity the serialization verifier
computes (same path, or the same canonical value through declared
fragment mappings, §4). A key that does not carry the topic key —
ordering by `customer_id` on a topic keyed by `order_id` — inherits no
precedence from the topic, and V1 has no other source. For a global
topic every key inherits the order.

Two things this stance deliberately does not claim. The topic's
observed order is not shown to match any *business* precedence: if two
producers publish same-key messages concurrently, their publication
order is whatever happened. Declaring a keyed topic is the model's
assertion that per-key publication order is meaningful, and an
operation keyed by it inherits that assertion, not a proof of it. And a
request input has no precedence at all: the DSL declares no arrival
order among requests from unmodeled callers, so an ordering
requirement keyed from a request input is unproven.

---

## 3. The mechanism: one lane, in order, one at a time

The §8.2 composition is the mechanism, and each of its three facts
does distinct work:

1. **Affinity.** `by_topic_key` routing puts same-key deliveries into
   one logical lane through the topic's key domain; `single_lane`
   routing puts every delivery of the subscription into one lane.
   Either keeps the messages whose precedence matters together.
   `by_topic_key` is meaningful only for a keyed topic: a global
   topic declares no key domain to route by (§8.2), so that pairing
   is an obstacle rather than a proof.
2. **Order within the lane.** A logical lane dispatches its deliveries
   in the order they entered it. §8.2 now states this explicitly; it
   is what makes a lane a lane rather than a pool, and the §8.2 proof
   pattern was always read this way.
3. **No overtaking.** Lane concurrency `bounded(1)` means invocations
   in one lane cannot overlap, so a later one cannot overtake an
   earlier one through its execution. Any larger or absent bound
   admits overtaking.

---

## 4. Redelivery: the fact serialization does not need

Serialization only asks that same-key invocations not overlap.
Ordering asks that they take effect in order, and a redelivery can
break that without any overlap: under `at_least_once`, an earlier
message may be delivered again after a later one was processed.
Executing the earlier logical invocation behind the later one inverts
the precedence.

V1 discharges this in one of two ways:

- `at_most_once` delivery: a logical message is never delivered
  again, so no late duplicate exists; or
- the operation's **idempotency requirement keyed from the same input
  is proven**: a late duplicate is another attempt at a logical
  invocation that already took effect, and the proof says it does no
  externally distinguishable work — so the observable history still
  respects the precedence.

`unspecified` delivery is treated like `at_least_once`: redelivery
cannot be excluded. This is the one place ordering depends on another
family's verdicts; the ordering verifier therefore runs after
idempotency's fixpoint and reads its verdicts.

---

## 5. Routes and obstacles

Per requirement, in order:

- **Population.** A key not sourced from an input selects no
  population (`KeyNotFromInput`); a request input has no precedence
  source (`RequestInputHasNoPrecedenceSource`).
- **Vacuous.** A subscription admitting no message schemas triggers
  nothing (`NoAdmittedInvocations`, proven).
- **Precedence.** Keyed topic with the key identity established for
  every admitted schema, or global topic; otherwise
  `TopicOrderingProvidesNoPrecedence`, `TopicKeyMappingMissing`, or
  `KeyIdentityUnestablished`.
- **Lane.** `by_topic_key` (keyed topic only; `ByTopicKeyWithoutKeyDomain`
  on a global one) or `single_lane`; otherwise
  `RoutingDoesNotPreserveOrder`. Lane concurrency `bounded(1)`;
  otherwise `LaneConcurrencyNotSerial`.
- **Redelivery.** `at_most_once`, or a proven idempotency requirement
  keyed from the input; otherwise `RedeliveryNotCollapsed`.

A proof (`LaneOrder`) cites the precedence source with its per-schema
key facts, the lane fact, and the duplicate handling; every citation is
a declared fact or a proven requirement.

---

## 6. Worked outcomes on the fixtures

Flash checkout (`tests/fixtures/flash_checkout.yaml`): `order_events`
is keyed by `order_id` for every schema; the three subscribers route
`by_topic_key` at lane concurrency one with `at_least_once` delivery,
and each declares its ordering key as `order_id` of its subscription.

- **`apply_payment`**: keyed precedence, keyed lane, and its
  idempotency requirement keyed from `captured` is proven.
  **Proven.**
- **`reserve_inventory`** and **`charge_payment`**: the same
  precedence and lane facts, but their idempotency requirements are
  unproven (a read-dependent reservation; an undeduplicated card
  charge), so a redelivered earlier message is not established to do
  no work. **Unproven** on exactly that obstacle — the ordering
  verdict inherits the idempotency gap rather than hiding it.

Video streaming (`tests/fixtures/video_streaming.yaml`):
`transcode_video` and `publish_video` order by `video_id` on a topic
keyed by `video_id`, route `by_topic_key` at lane concurrency one, and
both idempotency requirements are proven. **Both proven.**

---

## 7. What V1 deliberately does not infer

1. **Producer-side precedence.** No analysis shows a topic's
   publication order reflects a business order; the keyed topic is
   the assertion.
2. **Request ordering.** No arrival-order fact exists for requests,
   and a modeled caller with ordered dispatch is not a V1 concept.
3. **Cross-input and cross-operation precedence.** Ordering among
   invocations triggered through different inputs, or a causal order
   across operations, has no source in the DSL.
4. **Order-preserving redelivery.** Some transports redeliver an
   ordered suffix rather than a single message; the DSL declares no
   such fact, so redelivery is collapsed through idempotency or not at
   all.

---

## 8. Reconciliation

Executed 2026-08-22:

1. **Main document §8.2**: a logical lane dispatches its deliveries in
   the order they entered it.
2. **Main document §9** (`OrderingRequirement`): the V1 analysis
   summary — precedence source, mechanism, redelivery.
3. **Implementation**: `analyzer::verification::ordering`, reusing the
   serialization verifier's key identity and reading the idempotency
   verdicts.
