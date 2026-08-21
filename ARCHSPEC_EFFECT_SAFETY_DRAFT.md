# Archspec Duplicate Effect Execution Safety (V1)
## Resolution Draft for Operation Idempotency

**Status:** Accepted 2026-08-21. Defines the V1 judgment for when a duplicate effect execution is not externally distinguishable duplicate logical work, completing the rule set the operation idempotency requirement (§9) needs. Reconciled into `ARCHSPEC_DSL_SEMANTICS.md` (§9, §13) and implemented (`analyzer::verification::idempotency`) the same day.
**Date:** 2026-08-21
**Scope:** Duplicate-execution safety per effect kind; the single-delivery vacuous route; cross-operation request discharge and its fixpoint; what V1 does not infer.

---

## 1. The remaining gap

An idempotency requirement means (§9):

> Repeated attempts representing the same logical invocation must not
> cause externally distinguishable duplicate logical work beyond what
> the declared idempotency contract permits.

The replay rules (§18) settle the state-side of this: a transaction
whose commits are keyed by a stable key commits once per class, and a
naturally replayable transaction reproduces the same logical state on
re-execution. What remained undefined is the effect side. A retry can
re-reach any effect-executing step — §14 is explicit that even a
recovered intent may be executed again when a crash hides a prior
success — so duplicate execution must be considered possible at every
`execute_effect` and `execute_effect_intent` site, and the proof must
show the duplicate is not *distinguishable duplicate work*. This
document defines that judgment per effect kind.

Throughout, the analysis is relative to a governing key and population
(§12), and an effect **instance is class-fixed** when every attempt in
the class constructs the same logical instance:

- a direct `execute_effect` instance is class-fixed when its declared
  derivation is replay-deterministic — deterministic over roots stable
  under the §18 rules, judged in the artifact context at that step;
- an intent-mediated instance is class-fixed when the intent is
  replay-available by route A or route B of §17: a recovered intent is
  the exact original, and a reconstructed one is derived from
  class-fixed roots.

---

## 2. External effects: the boundary's own guarantee

An external boundary's deduplication is already declared (§13.3). The
V1 rule:

> A duplicate execution of an external effect is safe iff the effect
> declares `deduplicated_by { key }` and every component of that key
> is replay-stable relative to the governing key.

Key stability makes every attempt execute under the same evaluated
key; the boundary's guarantee then collapses the executions. The
guarantee is scoped to key equality alone, so no instance condition is
needed: a second execution sharing the key is deduplicated whatever
its payload.

`not_deduplicated` is the explicit negative: a duplicate execution is
distinguishable duplicate work at that boundary, and the requirement
is not established — the §13.3 "potentially observably unsafe" case.
`unspecified` leaves the same conclusion for the epistemic reason. As
everywhere, the verdict is unproven, not violated: model facts bound
what may happen, not what does.

---

## 3. Publications: same logical message

A duplicate publication is discharged by message identity:

> A duplicate execution of a publication effect is safe iff the topic
> declares a keyed message identity mapping the published schema, and
> the published instance is class-fixed.

*Soundness.* A class-fixed instance makes every attempt publish
payload-equal messages, hence equal identity tuples; by the declared
guarantee they are the **same logical message**. The duplicate
therefore creates no new logical work — at most it raises delivery
multiplicity, and delivery multiplicity is already an admitted degree
of freedom of the topic's delivery semantics, which every consumer's
own obligations must handle regardless (`at_least_once` admits
redelivery with or without duplicate publication; `at_most_once`
bounds deliveries of one logical message however often it is
published).

This does not quietly turn identity into a mechanism. The §24
distinction stands: the identity fixes *what* the repeated
publications are — one message — and the admitted delivery semantics
govern how often anything downstream observes it. Without the identity
declaration, or with an instance that is not class-fixed, two
publications are not established to be one message, and the duplicate
is unproven-safe.

Idempotency-key propagation plays no role here: propagation is
lineage for the *consumer's* analysis (§12) and deduplicates nothing
on the publishing side.

---

## 4. Requests: the target must deduplicate

A duplicate request invokes the target operation again, so its
discharge is the one place the analysis crosses operations:

> A duplicate execution of a request effect is safe iff the instance
> is class-fixed, the effect's schema is the targeted input's schema,
> and the target operation declares an idempotency requirement, keyed
> from the targeted input, that is itself proven.

*Soundness.* A class-fixed instance makes every attempt send
payload-equal requests to the same input; equal payloads evaluate the
target requirement's key equally, so the duplicate invocations fall
into one class of the target's requirement; that requirement being
proven means the class produces the work of a single logical
invocation.

The asymmetry with §3 is deliberate. A request identity on the target
input fixes payload consistency, but the target is still *invoked*
per attempt — invocation multiplicity is collapsed only by the
target's own proven obligation, a mechanism. A publication needs no
consumer-side proof because delivery multiplicity is already admitted
by the topic contract; a request's "delivery multiplicity" is
invocation multiplicity, which nothing admits by default.

### 4.1 The fixpoint

Request discharge makes idempotency verdicts mutually dependent, and
request cycles between operations are legal models. V1 computes the
verdicts as a **least fixpoint**: starting from no proven targets,
requirements are re-checked as their request targets become proven,
until nothing changes. The iteration is monotone — discharge
conditions only improve as targets prove — and terminates within one
pass per requirement. A cyclic dependency therefore settles as
unproven on the request legs, which is the conservative answer: V1
asserts nothing about whether some cycles are coinductively safe.

---

## 5. Transition side effects

A transition side effect is executed only through its implicitly
established intent (§22), so it takes the intent-mediated form of the
rules above: the intent must be replay-available — in practice route
B, since every transition transaction is explicitly keyed — and the
publication or request condition of §3/§4 applies to its contract.

---

## 6. The state leg, restated

For completeness, the V1 idempotency analysis over each admitted flow
(the same admitted-flow scoping as recoverability: flows with no
response, or with the triggering input's response) is:

1. **every** transaction step is retry-safe — keyed commit over a
   stable key, or naturally replayable. Unlike recoverability, there
   is no final-step exemption: a duplicate delivery re-drives the
   whole flow even after terminal completion, so every committed
   transaction may be re-encountered;
2. every effect-executing step is duplicate-safe per §2–§5;
3. an executed intent must be established by an earlier step at all.

Response consistency is the separate response-replay obligation and is
not re-checked here. Serialization facts are not needed: keyed commits
exclude concurrent same-key commits by contract (§17), natural replay
writes class-fixed values to class-fixed targets whatever the
interleaving, and the boundary guarantees of §2–§4 are stated over
populations, not schedules.

---

## 7. Vacuous routes

- **Empty population**: the triggering subscription admits no message
  schemas.
- **No admitted behavior**: no admitted flow exists for the triggering
  input, so an attempt performs no modeled work and there is nothing
  to duplicate. (Recoverability treats the same shape as an obstacle:
  progress is impossible; safety is trivial. The asymmetry is
  correct.)
- **Single delivery**: the triggering input is a subscription with
  `at_most_once` delivery and the whole payload is identity-pinned by
  the governing key (§18 rule 3). Same-class messages are then one
  logical message, delivered no more than once, so a class contains at
  most one attempt and repeated attempts cannot exist. Requests have
  no analogous route: nothing bounds an unmodeled caller's retries.

---

## 8. Worked outcomes on the flash-checkout fixture

- **`create_order`**: keyed commit over the governing key; the
  publication intent is recovered and `OrderCreated` is
  identity-mapped on the topic. **Proven.**
- **`apply_payment`**: keyed commit over `event_id`; the transition
  intent is recovered; `OrderPaid` is identity-mapped. **Proven.**
- **`charge_payment`**: the capture publication is safe — its direct
  derivation is replay-deterministic under the message identity pinned
  by `event_id`, and `PaymentCaptured` is identity-mapped — but the
  card charge is an external effect explicitly `not_deduplicated`.
  **Unproven**, with exactly that obstacle: the model admits charging
  the card twice.
- **`reserve_inventory`**: the reservation transaction is
  `not_deduplicated` with a read-dependent write, and the publication
  intent it establishes is replay-available by neither route.
  **Unproven** on both legs, matching its recoverability verdict.

---

## 9. What V1 deliberately does not infer

1. **Coinductive request cycles** (§4.1): settled unproven.
2. **Partial-payload publication identity**: a direct instance whose
   derivation is unspecified or unstable is never class-fixed, even if
   its identity fields alone might be; the DSL declares instance
   provenance at whole-instance granularity.
3. **Compensation or permitted-duplicate contracts**: "beyond what the
   declared idempotency contract permits" currently has no DSL surface
   for declaring permitted duplicates; V1 treats every unproven
   duplicate as unpermitted.
4. **Delivery-driven attempt bounding for requests**: no fact bounds
   caller retries, so the single-delivery route is subscription-only.

---

## 10. Reconciliation

Executed 2026-08-21:

1. **Main document §9** (`IdempotencyRequirement`): the V1 analysis
   summary — state leg, effect leg, vacuous routes, and the
   separation from response replay.
2. **Main document §13.1/§13.2/§13.3**: the per-kind
   duplicate-execution rules of §2–§4, including the
   publication/request asymmetry.
3. **Implementation**: `analyzer::verification::idempotency`, reusing
   the replay engine, with the request fixpoint of §4.1.
