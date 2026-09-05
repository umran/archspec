# Archspec Flow Resumption (V1)
## A Same-Flow Continuation Stance for Recoverability

**Status:** Accepted 2026-08-21. States the V1 analysis for `RecoverabilityRequirement` and the stance it takes toward open question 7 of `ARCHSPEC_SEMANTICS_REVISION_DRAFT.md` §27, which it deliberately does **not** resolve. Reconciled into `ARCHSPEC_DSL_SEMANTICS.md` §9 and implemented (`analyzer::verification::recoverability`) the same day.
**Date:** 2026-08-21
**Scope:** How V1 discharges `completion: resumable` and `completion: guaranteed`; the crash-prefix analysis; retry drivers; what remains open.

**Terminology note (2026-09-04).** `ARCHSPEC_OPERATION_EXECUTION_REVISION_DRAFT_V3.md` replaced the surface this document was written against. Read its retired terms as follows: an *invocation flow* is a *path of the operation program* (one arm taken at each `match_result` or `branch`, ending at a terminal), so *same-flow continuation* is *same-path continuation*; `FlowStep` is `OperationStep`; `InvocationResult`, `EstablishInvocationResult`, and `ValueSource::invocation_result` are `TransactionOutput`, `EstablishTransactionOutput`, and `transaction_output`; a `Response` / `ResponseSource` / `flow.response` is the `return` terminal constructing the `RequestInput.result` contract, and a flow with `response: null` is a path ending at `complete`; *response replay* is *result replay*; `ObjectHistoryRequirement::linearizable` is removed and deferred. The V1 rules below carry over unchanged **per admitted path** — a path ending at `complete` or at `return` for the triggering input — with each decision judged where it is taken; for this progress obligation a decision is never an obstacle (§3.3). See V3 §48.

---

## 1. The question and this document's scope

A recoverability requirement obliges the logical invocation identified
by its key to reach terminal execution of a declared flow (§9) — since
2026-09-04, a valid terminal of the operation program, `return` or
`complete` (V3 §37). The requirement deliberately does not name a
flow, and which flows (now: paths) remain admissible for a resumed
attempt after a partial execution is open question 7 of the revision
draft.

This document does not answer question 7. It adopts a **sufficient**
V1 route that does not prejudge it:

> **Same-flow continuation.** V1 proves resumability by establishing
> that, for every prefix at which an attempt may fail, re-driving the
> **same flow from its first step** reaches the flow's terminal
> completion.

If the same flow admits a continuation from every prefix, the
obligation is discharged under *any* eventual resolution of question 7
that leaves the interrupted flow admissible to its own resumption —
and the model's recovery semantics are already written in exactly
those terms: a re-encountered keyed transaction "resolves the prior
commit" (§16), and the §22 crash examples resume by "retrying the
flow". A future resolution of question 7 may add alternative-flow
continuations as further proof routes; it cannot invalidate this one.

---

## 2. Population and admitted flows

The requirement's key is a governing key in the §12 sense: V1 analysis
proceeds only when its components name one triggering input, and the
population is the invocations triggered by that input. A population
empty by declaration (a subscription admitting no message schemas)
discharges the obligation vacuously.

The flows analyzed for a population triggered by input `i` are the
flows an `i`-invocation can complete — since 2026-09-04, the *paths*
of the operation program an `i`-invocation can complete:

- flows with `response: null` — now paths ending at `complete`, and
- flows whose response is declared for `i` itself — now paths ending
  at `return` for `i`.

A flow terminating with another request input's response — a path
ending at `return` for another input — is not a path an
`i`-invocation can complete, and is outside the analysis. If no
admitted path exists, the obligation cannot be discharged: the
population has no terminal path at all (`NoAdmittedPath`).

---

## 3. The crash-prefix analysis

Fix an admitted flow. An attempt may fail before any step, between any
two steps, or mid-step. Transactions are atomic, so a mid-transaction
failure commits nothing and a resumed attempt simply executes the
transaction fresh; a failed effect execution can be attempted again.
The prefixes that need argument are those where some transactions have
committed and the terminal completion has not been reached.

Re-driving the same flow from its first step reaches terminal
completion exactly when the following hold.

### 3.1 Committed transactions resolve on re-encounter

For every transaction step that can be followed by a failing prefix —
every transaction step, except one that is the flow's final step in a
flow with no declared response (now: the final step of a path ending
at `complete`) — the resumed attempt re-encounters a
transaction the prior attempt may already have committed. Per §9, the
re-encounter must resolve:

- **by keyed commit** — `deduplicated_by` with a commit key that is
  replay-stable relative to the governing key, so every attempt in the
  class addresses the same `Commit(T,K)`, which resolves without
  re-executing the body and restores its artifacts; or
- **by natural replay** — the transaction is naturally replayable
  under the V1 rules, so re-executing the body safely reproduces the
  same logical outcome.

Re-executing a committed transaction that is neither is not a
continuation of the same logical invocation — the re-execution may do
different or duplicate work — so V1 records the transaction as
unresolvable and the obligation as unproven. This is a progress
obligation refusing to be discharged by a safety violation.

The final-step exemption is exact: a transaction that is the last step
of a response-less flow (a path ending at `complete`) has no failing
prefix after it. If the attempt crashed after that commit, the
invocation already reached terminal execution; there is nothing to
resume. When a response is declared — when the path ends at `return`
— the result's construction and return follow the last step, so every
transaction on the path needs re-encounter resolution.

### 3.2 Consumed artifacts are replay-available

Every artifact a later step consumes must be replay-available by route
A or route B of §17, judged by the replay engine: there is always a
prefix that fails after the establishing transaction commits and
before the consumer runs, and the resumed attempt must recover or
reconstruct the exact artifact. Mere re-establishment with different
contents is not availability; a resumed run that continues with a
different artifact is not continuing the same invocation.

Consumption means:

- an `execute_effect_intent` step — the intent must also have been
  established at all, by a step earlier on the path; a path that
  executes an intent nothing establishes cannot proceed on any
  attempt;
- a declared response sourced from an invocation result — now a
  `return` whose outcome derivation references a `transaction_output`;
- any `invocation_result` (now `transaction_output`) reference inside
  a later transaction's body — selectors, mutation derivations,
  artifact derivations, transition effect values — or inside a
  program-level `execute_effect` derivation.

References to an artifact established earlier **in the same
transaction** impose nothing: atomicity means a resumed attempt either
re-executes the whole body fresh or resolves the whole prior commit.
A transaction's own commit key is judged by the re-encounter analysis
of §3.1 and is not double-counted here.

A response whose source is `unspecified` — a `return` whose outcome
derivation is `unspecified` — imposes no artifact condition:
recoverability is progress, and the model declares no consumption for
such a result. Its replay *stability* is a separate obligation (result
replay) with its own analysis.

### 3.3 Nothing else blocks

The third §9 bullet — no step left in a state from which the flow
cannot proceed — is discharged by construction in V1: transactions are
atomic, effect executions can be re-attempted (their duplicate-safety
is idempotency's concern, deliberately separate) and re-observed, and
consuming an artifact does not remove it from the invocation's context.

*(Added 2026-09-04.)* A decision — a `match_result` or `branch` — is
likewise never an obstacle to progress. A retry that is not
established to take the same arm follows some other admitted path,
which is analyzed on its own; that the two paths do different work is
idempotency's concern (V3 §30, §48.2), not recoverability's. This is
why `transcode_video` in the video-streaming fixture is recoverable
while its idempotency is unproven: the match on the external engine's
result may go either way on retry, but each arm resumes.

---

## 4. `completion: guaranteed` — retry drivers

`guaranteed` adds to resumability the fact that the invocation **is**
re-driven. V1 accepts exactly the modeled drivers §9 names, resolved
against the triggering input:

1. the triggering input is a subscription declaring
   `delivery: at_least_once`; or
2. the triggering input is a request, and some modeled caller declares
   a `RequestEffect` targeting that operation and input with
   `retry: may_repeat` — whether declared among an operation's effects
   or as a state-machine transition side effect, which is a
   `RequestEffect` under §22.

Both driver facts re-drive the *same logical invocation*: a redelivery
is another delivery of one logical message, and `may_repeat` repeats
one logical request, so the re-driven attempt carries the same payload
and hence the same governing-key value.

Both cautions of §9 stand. The driver facts are duplicate-delivery
facts, not bounded-liveness facts, so a `guaranteed` proof is
conditional (§1.3) on the abstraction genuinely re-driving until
success; and a request input with no modeled `may_repeat` caller
supplies no driver, so `guaranteed` on such an operation is
undischargeable, with the missing driver recorded.

A driver makes retries *expected*. Recoverability says nothing about
their safety (§6.4), so when the operation also declares no
idempotency requirement keyed from the triggering input, the proof
carries a warning: the retries that guarantee completion have
undeclared, unverified safety. The verdict is unchanged — progress is
progress — but a reader would look for the safety half exactly there,
and the model has not supplied it.

---

## 5. Worked outcomes on the flash-checkout fixture

- **`create_order`** (`resumable`): its only path (the program has no
  decisions) re-encounters `tx.create_order.new`, which resolves by
  keyed commit — the commit key is the governing key itself. The
  publication intent and the transaction output `output.create_order`
  are recovered from the same commit, and the `return` derives its
  `Ok` payload from the recovered output. **Proven.**
- **`apply_payment`** (`guaranteed`): the transition transaction
  resolves by keyed commit over the stable `event_id`; the
  transition-established intent is recovered; the triggering
  subscription declares `at_least_once`, supplying the driver.
  **Proven.**
- **`reserve_inventory`** (`guaranteed`): the driver exists, but
  `tx.reserve_inventory` is `not_deduplicated` and its stock write
  derives from a transaction read, so the committed transaction
  resolves by neither route, and the publication intent it establishes
  is replay-available by neither route. **Unproven**, with both gaps
  recorded. The fixture keeps this shape deliberately: a crash between
  the reservation commit and the publication has no legitimate
  continuation, and the checker now says so.

---

## 6. What V1 deliberately does not infer

1. **Alternative-flow continuation.** Question 7 remains open. V1
   neither uses another flow (now: path) as a continuation nor
   forbids a future solver from doing so.
2. **Bounded liveness.** No retry count, backoff, or eventual-success
   fact exists in the DSL; `guaranteed` proofs are conditional on the
   driver abstraction.
3. **External effect success.** Recoverability establishes that the
   flow can be driven through its steps; that an external boundary
   eventually accepts an effect is outside the model.
4. **Safety.** Driving a flow to termination leaves the §14
   effect-level uncertainty and every idempotency question exactly
   where they were; recoverability discharges progress only.

---

## 7. Reconciliation

Executed 2026-08-21:

1. **Main document §9**: the `completion: resumable` section gains the
   V1 same-flow analysis (admitted flows, re-encounter resolution with
   the final-step exemption, consumed-artifact availability); the
   `completion: guaranteed` section gains the driver search, including
   transition-side-effect callers.
2. **Revision draft §27, question 7**: annotated — not resolved — with
   the V1 sufficient-route stance and a pointer here.
3. **Implementation**: `analyzer::verification::recoverability`,
   reusing the replay engine.

Revised 2026-09-04: the operation-execution revision
(`ARCHSPEC_OPERATION_EXECUTION_REVISION_DRAFT_V3.md`, §48) replaced
flows with one operation program. The same-flow route is now
same-path continuation over the paths of the program admitted for the
triggering input; the final-step exemption applies to a
`complete`-terminated path, a `return` needs every transaction
resolved, consumed artifacts include the `return` outcome's
transaction outputs, and decisions are never an obstacle (§3.3). The
terminology note after the status block maps the retired vocabulary.
Verdicts on the flash-checkout fixture are unchanged (§5).
