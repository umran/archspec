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
   in the order they entered it, and it does not advance past an
   incomplete delivery: a failed attempt is re-dispatched at the head
   of the lane before any later delivery. §8.2 now states both
   explicitly; they are what make a lane a lane rather than a pool,
   and the §8.2 proof pattern was always read this way. A transport
   whose lane skips a failed delivery and redelivers it later does
   not conform to the declaration.
3. **No overtaking.** Lane concurrency `bounded(1)` means invocations
   in one lane cannot overlap, so a later one cannot overtake an
   earlier one through its execution. Any larger or absent bound
   admits overtaking.

---

## 4. Redelivery and duplicates

Serialization only asks that same-key invocations not overlap;
ordering asks that they take effect in order, so `at_least_once`
delivery raises a question serialization never meets. Two different
things hide under "redelivery", and they are answered differently.

**Failure-driven redelivery.** An attempt at `m1` fails before taking
effect and `m1` is delivered again. If the lane had advanced to `m2`
in the meantime, the redelivered attempt — `m1`'s *first effective*
execution — would take effect after `m2`, and no idempotency fact
could repair that: there is no duplicate work to collapse, only an
inversion. What prevents it is the lane semantic of §3: the lane does
not advance past an incomplete delivery, so the retry precedes `m2`.
Lane concurrency one keeps the retried attempt from overlapping what
follows; it is the head-of-line rule that keeps it in place. This is
why the §8.2 composition is sufficient on its own.

**Duplicates of a completed delivery.** A transport may deliver `m1`
again after its invocation completed and `m2` was processed. That
attempt belongs to a logical invocation that already took effect in
its place; the precedence between logical invocations is intact. What
the repeated attempt *does* is the idempotency requirement's
obligation (§9 keeps the two families separate, as it does for
recoverability), so the ordering proof does not depend on it. The
proof records which idempotency requirement keyed from the input
answers for the duplicate and whether it is proven, or that none does
— in which case the model-wide note on unchecked duplicate deliveries
already points at the gap.

`at_most_once` delivery admits neither case. `unspecified` delivery
is treated like `at_least_once`: a lost failed attempt takes effect
never, which inverts nothing, and a redelivered one retries in place.

An earlier revision of this document required a proven idempotency
requirement for `at_least_once` subscriptions. That was both too
strong — idempotency is not what keeps a retry in place — and
unsound for the failure-driven case, where a proven requirement
collapses nothing; the head-of-line rule replaces it.

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
- **Duplicates.** Recorded, never an obstacle: `at_most_once`
  (`SingleDelivery`), or head-of-line retry with the idempotency
  requirement keyed from the input that answers for duplicates, and
  its verdict, when one is declared.

A proof (`LaneOrder`) cites the precedence source with its per-schema
key facts, the lane fact, and the duplicate handling; every citation is
a declared fact.

---

## 6. Worked outcomes on the fixtures

Flash checkout (`tests/fixtures/flash_checkout.yaml`): `order_events`
is keyed by `order_id` for every schema; the three subscribers route
`by_topic_key` at lane concurrency one with `at_least_once` delivery,
and each declares its ordering key as `order_id` of its subscription.

- **`apply_payment`**, **`reserve_inventory`**, **`charge_payment`**:
  keyed precedence, keyed lane at concurrency one, head-of-line retry.
  **All proven.** The proofs differ only in what they record about
  duplicates: `apply_payment`'s idempotency requirement is proven;
  the other two name their requirements as unproven, which is where
  the read-dependent reservation and the undeduplicated card charge
  are reported — under idempotency, not smuggled into ordering.

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
4. **Lane conformance.** Head-of-line retry is a stated lane
   semantic, not an inferred one; a transport whose ordered lane can
   skip a failed delivery is outside the declaration, and no fact in
   the DSL distinguishes it.

---

## 8. Reconciliation

Executed 2026-08-22:

1. **Main document §8.2**: a logical lane dispatches its deliveries in
   the order they entered it.
2. **Main document §9** (`OrderingRequirement`): the V1 analysis
   summary — precedence source, mechanism, redelivery.
3. **Implementation**: `analyzer::verification::ordering`, reusing the
   serialization verifier's key identity; the idempotency verdicts are
   read only to record duplicate coverage.
