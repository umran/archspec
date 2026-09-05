# Archspec Duplicate Effect Execution Safety (V1)
## Resolution Draft for Operation Idempotency

**Status:** Accepted 2026-08-21. Defines the V1 judgment for when a duplicate effect execution is not externally distinguishable duplicate logical work, completing the rule set the operation idempotency requirement (§9) needs. Reconciled into `ARCHSPEC_DSL_SEMANTICS.md` (§9, §13) and implemented (`analyzer::verification::idempotency`) the same day.
**Date:** 2026-08-21
**Scope:** Duplicate-execution safety per effect kind; the single-delivery vacuous route; cross-operation request discharge and its fixpoint; what V1 does not infer.

**Terminology note (2026-09-04).** `ARCHSPEC_OPERATION_EXECUTION_REVISION_DRAFT_V3.md` replaced the surface this document was written against. Read its retired terms as follows: an *invocation flow* is a *path of the operation program* (one arm taken at each `match_result` or `branch`, ending at a terminal); `FlowStep` is `OperationStep`; `InvocationResult`, `EstablishInvocationResult`, and `ValueSource::invocation_result` are `TransactionOutput`, `EstablishTransactionOutput`, and `transaction_output`; a `Response` / `ResponseSource` / `flow.response` is the `return` terminal constructing the `RequestInput.result` contract; *response replay* is *result replay*; `ObjectHistoryRequirement::linearizable` is removed and deferred. The V1 rules below carry over unchanged **per admitted path** — a path ending at `complete` or at `return` for the triggering input — with each decision judged where it is taken: a path's decisions must replay (V3 §30) for the path's retry to be the same work, which is the control leg §6 now records. See V3 §48.

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

## 3. Publications: same logical message, collapsed by every consumer

A duplicate publication is discharged at the topic by message
identity, and downstream by the consumers of that message:

> A duplicate execution of a publication effect is safe iff the topic
> declares a keyed message identity mapping the published schema, the
> published instance is class-fixed, **and** every modeled consumer of
> the message collapses duplicate deliveries of it: an operation
> subscribing to the topic with a message selection admitting the
> schema either holds a proven idempotency requirement keyed from that
> subscription, or receives it with `at_most_once` delivery.

*Soundness at the topic.* A class-fixed instance makes every attempt
publish payload-equal messages, hence equal identity tuples; by the
declared guarantee they are the **same logical message**. The
duplicate therefore creates no new logical work *there* — at most it
raises delivery multiplicity.

*Soundness downstream.* Delivery multiplicity is an admitted degree
of freedom of the topic's delivery semantics, but the work a
redelivery causes in a consumer is still work the upstream attempt
caused, and the requirement's "must not cause" is transitive. Each
consumer collapses it by one of two facts. Payload-equal deliveries
evaluate the consumer's key equally, so they fall into one class of a
requirement keyed from the subscription; that requirement being proven
means the class performs the work of one logical invocation — and,
recursively, that its own cascade collapses. Or `at_most_once`
delivery bounds the one logical message to at most one delivery,
however often it is published, so the consumer never sees a second.

An earlier version of this document stopped at the topic, on the
argument that consumers must handle redelivery anyway. That argument
explains why the producer is not *to blame* for a consumer's defect;
it does not make the producer idempotent. On the flash-checkout
fixture it proved `create_order` while a retried `create_order` could
charge a card twice through `reserve_inventory` and `charge_payment`.
The consumer leg closes exactly that gap, and — because a consumer
that declares no requirement is checked nowhere else — it is also
where such a consumer first becomes visible.

This does not quietly turn identity into a mechanism. The §24
distinction stands: the identity fixes *what* the repeated
publications are — one message — and the consumers' mechanisms fix
what that one message can do. Without the identity declaration, or
with an instance that is not class-fixed, two publications are not
established to be one message, and the duplicate is unproven-safe. A
consumer the model does not contain is outside the proof, which is
conditional on the model's closed world of consumers (§1.3).

Idempotency-key propagation plays no role here: a class-fixed
instance already makes every duplicate payload-equal, so a consumer's
key evaluates equally across them whichever fields it reads.
Propagation remains lineage for the *consumer's* analysis (§12) and
deduplicates nothing on the publishing side.

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

Request and consumer discharge make idempotency verdicts mutually
dependent, and cycles — request cycles between operations, or
publication cycles through topics — are legal models. V1 computes the
verdicts as a **greatest fixpoint**: every requirement with an
admissible governing key is assumed, and whatever fails under that
assumption is dropped until nothing more fails. The iteration is
monotone — fewer assumptions never prove more — and terminates within
one pass per requirement. A cycle whose members each pass their local
checks under the mutual assumption is therefore proven, and the proof
is marked *coinductive*.

*Soundness.* Suppose some member of a self-consistent set `P` were
violated: some execution in which duplicate attempts at one of its
logical invocations cause distinguishable duplicate work. Among all
such violations across `P`, take one whose duplicate work lies at the
shortest causal distance from the duplicated attempts. That work is
not in the invocation's own flow — the state leg and the external and
publication legs are discharged by local facts that assume nothing
about `P`. So it lies downstream of a request or publication, and the
local check establishes that the duplicates reached the target or
consumer as payload-equal inputs, falling into one class of its
requirement, which is in `P`. The duplicate work is then caused by
duplicate attempts of *that* invocation, at a strictly shorter
distance — contradicting the choice of the violation. Every causal
chain in a real execution is finite, so no violation exists. The
argument uses the downstream requirement only for what its local check
provides — a key over the input that collapses payload-equal attempts
— never for its verdict, which is why assuming the verdict is
harmless. The least fixpoint is computed alongside, only to mark which
proofs rest on a cycle.

---

## 5. Transition side effects

A transition side effect is executed only through its implicitly
established intent (§22), so it takes the intent-mediated form of the
rules above: the intent must be replay-available — route B when the
transaction is keyed; since Amendment A
(`CONSEQO_REVISION_V3_AMENDMENT_A_TRANSITION_DEDUP_RELAXATION.md`) a
transition transaction may also be unkeyed, and its intents are then
replay-available by no route, so the site settles unproven — and the
publication or request condition of §3/§4 applies to its contract.

---

## 6. The state leg, restated

For completeness, the V1 idempotency analysis over each admitted path
of the operation program (the same scoping as recoverability: paths
ending at `complete`, or at `return` for the triggering input) is:

1. **every** transaction step is retry-safe — keyed commit over a
   stable key, or naturally replayable. Unlike recoverability, there
   is no final-step exemption: a duplicate delivery re-drives the
   whole program even after terminal completion, so every committed
   transaction may be re-encountered;
2. every effect-executing step is duplicate-safe per §2–§5;
3. an executed intent must be established by an earlier step at all;
4. *(added 2026-09-04, the control leg)* every decision on the path
   replays — a `match_result` whose result is replay-stable, or a
   `branch` whose condition is deterministic over replay-stable roots
   (V3 §30) — so a retry traverses the same path. A decision that may
   go the other way lets the retry do different work, and V1 has no
   compatibility argument for the two histories; it is recorded as an
   obstacle (`PathDecisionUnstable`). An external effect's result is
   never replay-stable in V1 (V3 §48.2), so a match on one is always
   such an obstacle.

Result consistency is the separate result-replay obligation and is
not re-checked here; its verdicts feed in only where a decision rests
on a request effect's result. Serialization facts are not needed: keyed commits
exclude concurrent same-key commits by contract (§17), natural replay
writes class-fixed values to class-fixed targets whatever the
interleaving, and the boundary guarantees of §2–§4 are stated over
populations, not schedules.

---

## 7. Vacuous routes

- **Empty population**: the triggering subscription admits no message
  schemas.
- **No admitted behavior**: no admitted path exists for the triggering
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
  identity-mapped on the topic — but `reserve_inventory` consumes
  `OrderCreated` and is itself unproven, so the cascade does not
  collapse. **Unproven**, with exactly that obstacle: a retried
  `create_order` may reserve inventory twice and, through
  `charge_payment`, charge the card twice.
- **`apply_payment`**: keyed commit over `event_id`; the transition
  intent is recovered; `OrderPaid` is identity-mapped. **Proven.**
- **`charge_payment`**: the capture publication is safe — its direct
  derivation is replay-deterministic under the message identity pinned
  by `event_id`, and `PaymentCaptured` is identity-mapped — but the
  card charge is an external effect explicitly `not_deduplicated`.
  **Unproven**, with that obstacle: the model admits charging the card
  twice. *(Since 2026-09-04 the fixture binds the charge's result and
  matches on it — `ok` publishes `PaymentCaptured`, `err` publishes
  `PaymentFailed` with the decline `reason` — and two further obstacles
  join the first: the match on the external result is not established
  to replay, so a retry may take the other arm; and the `PaymentFailed`
  instance depends on the `reason` root, which is not replay-stable
  because no declared fact makes an external result
  replay-consistent. The verdict is unchanged.)*
- **`reserve_inventory`**: the reservation transaction is
  `not_deduplicated` with a read-dependent write, and the publication
  intent it establishes is replay-available by neither route.
  **Unproven** on both legs, matching its recoverability verdict.

---

## 9. What V1 deliberately does not infer

1. **Nothing about cycles beyond §4.1**: a cycle proves only when
   every member passes its local checks; a member failing for any
   other reason fails the cycle with it.
2. **Consumers outside the model**: the cascade is followed through
   the modeled subscriptions only; the proof is conditional on that
   closed world.
3. **Partial-payload publication identity**: a direct instance whose
   derivation is unspecified or unstable is never class-fixed, even if
   its identity fields alone might be; the DSL declares instance
   provenance at whole-instance granularity.
4. **Compensation or permitted-duplicate contracts**: "beyond what the
   declared idempotency contract permits" currently has no DSL surface
   for declaring permitted duplicates; V1 treats every unproven
   duplicate as unpermitted.
5. **Delivery-driven attempt bounding for requests**: no fact bounds
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
   the replay engine, with the fixpoint of §4.1.

Revised 2026-08-21: §3 gained the consumer leg, §4.1 the publication
edges of the fixpoint, and §8 the corrected `create_order` outcome;
the main document's §9 and §13.1 and the implementation (with the
trigger graph in `analyzer::verification::trigger`) were reconciled
the same day.

Revised 2026-08-25: the 2026-08-21 pass left least-fixpoint prose
behind in the main document's §9 and §13.1 — the latter still
describing publication cycles as settling unproven — and in the
implementation's module and obstacle documentation. The behavior was
and remains the greatest fixpoint of §4.1 over both legs, as
`cyclic_publication_dependencies_prove_coinductively` asserts; the
prose was corrected to match, and no behavior changed.

Revised 2026-09-04: the operation-execution revision
(`ARCHSPEC_OPERATION_EXECUTION_REVISION_DRAFT_V3.md`, §48) replaced
flows with one operation program. The analysis now runs per admitted
path, §6 gained the control leg, and §8's `charge_payment` outcome
records the two obstacles the fixture's new match on the card result
adds; the terminology note after the status block maps the retired
vocabulary. The per-kind rules of §2–§5 and the fixpoint of §4.1 are
unchanged.
