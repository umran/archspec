# Archspec Semantic Revision Draft
## Transaction Replay, Deterministic Derivation, and Transaction Artifacts

**Status:** Draft for discussion  
**Date:** 2026-08-19  
**Scope:** Proposed revision to operation transaction, flow, invocation-result, effect-intent, and value-provenance semantics.

**Terminology note (2026-09-04).** `ARCHSPEC_OPERATION_EXECUTION_REVISION_DRAFT_V3.md` (implemented 2026-09-04, see its §48) replaced the operation-execution surface this draft was written against. Read its retired terms as follows: an *invocation flow* is a *path of the operation program* — `Operation.program` is one structured, acyclic block, and a path takes one arm at each `match_result` or `branch` and ends at a terminal; `FlowStep` is `OperationStep`; `InvocationResult`, `EstablishInvocationResult`, and `ValueSource::invocation_result` are `TransactionOutput`, `EstablishTransactionOutput`, and `transaction_output`, with every artifact rule here (§3.3, §7, §12, §14–§16, §23) applying to transaction outputs verbatim; a `Response` / `ResponseSource` / `flow.response` is the `return` terminal constructing the `RequestInput.result` contract (`Result<Ok, Err>`), and a flow with no response is a path ending at `complete`; *response replay* (§18) is *result replay*; `ObjectHistoryRequirement::linearizable` is removed and deferred (V3 §3). The V1 analyses this draft specifies — natural replay (§22), artifact replay (§23), operation idempotency (§24) — carry over unchanged **per admitted path** (a path ending at `complete` or at `return` for the triggering input), with each decision judged where it is taken. Sections §18, §21, and the flow shapes in §25 and §28 are superseded and annotated in place; the remainder stands.

This draft is intentionally narrower than a complete rewrite of the Archspec semantic contract. It records the model reached during the transaction/idempotency discussion and is intended to be reconciled into the main semantics document after implementation details are finalized.

---

## 1. Goals

This revision has five goals:

1. Keep **transaction idempotency** separate from **invocation-result durability**.
2. Allow Archspec to prove **natural replayability** from transaction semantics where sufficient provenance exists.
3. Provide an explicit **durable keyed transaction-deduplication mechanism** when natural replayability cannot be proven or is not desired.
4. Treat `InvocationResult` and `EffectIntent` as **logical transaction artifacts**, rather than as objects that are inherently durable in every architecture.
5. Preserve transaction-produced artifacts across later flow steps and retries without introducing explicit `Recover*` flow steps.

The revision deliberately does **not** attempt to make Archspec a general expression language.

---

## 2. Existing Semantics Being Revised

The current implementation has, among other things:

- `Transaction { data_model, isolation, steps }`
- `TransactionStep::AcquireUniqueClaim`
- `TransactionStep::EstablishEffectIntent`
- `TransactionStep::EstablishInvocationResult`
- `TransactionStep::ReadInvocationResult`
- `InvocationResult { key, schema }`
- `EffectIntent { effect, execution }`
- `IntentExecutionSemantics::{Unspecified, Recoverable}`
- `FlowStep::{Transaction, ExecuteEffect, ExecuteEffectIntent}`
- no transaction-read value source
- no mutation-value provenance declaration

This draft changes those semantics while preserving the small ordered-flow model.

---

# 3. Core Semantic Separation

Archspec SHALL distinguish the following concepts.

## 3.1 Natural transaction replayability

A transaction is naturally replayable when the analyzer can prove from the transaction's declared semantics that re-executing it for the same logical invocation can reproduce the same logical committed outcome and any artifacts required by the remainder of the flow, without relying on recovery of a prior durable commit.

Natural replayability is **derived**. It is not asserted by a boolean such as:

```yaml
idempotent: true
```

Natural replayability may follow from facts such as:

- stable mutation targets;
- deterministic mutation contents;
- uniqueness inherent in `DataObject.identity`;
- absence of persistent mutations.

For V1, a transaction containing any `Transition` is **not** eligible for natural replay. A successful transition changes the state against which that same transition was evaluated, so the transaction cannot be assumed to reproduce the same execution on a later attempt. Transition-containing transactions use the explicit durable keyed-idempotency route defined in this revision.

A guard that merely prevents a second commit is not sufficient for natural replayability. If a retry cannot successfully reproduce the transaction's logical outcome and required artifacts, the transaction is not naturally replayable even when duplicate state mutation is impossible.

If the declared semantics are insufficient, natural replayability is `Unknown`.

## 3.2 Explicit keyed transaction deduplication

A transaction may separately declare an explicit durable idempotency mechanism:

```text
DeduplicatedBy(K)
```

This does not assert that the transaction body is naturally idempotent.

Instead it guarantees:

> For a transaction declaration `T` and evaluated idempotency key `K`, at most one logical execution of `T(K)` may successfully commit.

A subsequent encounter with the same successfully committed `T(K)` SHALL resolve the prior logical commit rather than commit the transaction body again.

## 3.3 Transaction artifacts

`InvocationResult` and `EffectIntent` are logical artifacts established by transaction execution.

They are not themselves the mechanism that deduplicates the enclosing transaction.

Their replay availability follows one of two routes:

1. **Deterministic reconstruction** by safely re-executing the establishing transaction; or
2. **Durable recovery** from a previously committed transaction that is explicitly deduplicated by key.

---

# 4. Transaction Atomicity

The existing atomic transaction interpretation is retained.

A `Transaction` represents one logical atomic commit over all its steps.

`data_model` identifies the application `DataModel` participating in that transaction. Framework-managed transaction state may participate in the same logical atomic boundary without belonging to the application `DataModel`.

Therefore:

```text
Write(application object)
EstablishInvocationResult(R)
EstablishEffectIntent(E)
```

within one transaction either logically commit together or do not commit.

---

# 5. Proposed Transaction Shape

A transaction SHOULD gain an explicit idempotency guarantee:

```rust
pub struct Transaction {
    pub data_model: Option<Id>,
    pub isolation: TransactionIsolation,
    pub idempotency: IdempotencyGuarantee,
    pub steps: Vec<TransactionStep>,
}
```

The existing `IdempotencyGuarantee` vocabulary can be reused:

```rust
pub enum IdempotencyGuarantee {
    Unspecified,
    NotDeduplicated,
    DeduplicatedBy { key: IdempotencyKey },
}
```

For transactions:

### `Unspecified`

No explicit transaction-commit deduplication fact is available.

The analyzer MAY still prove natural replayability from the transaction body.

### `NotDeduplicated`

The architecture explicitly declares that the execution environment does not provide keyed transaction-commit deduplication.

The analyzer MAY still prove natural replayability.

### `DeduplicatedBy { key }`

The execution environment guarantees durable keyed logical-commit deduplication as defined in §6.

---

# 6. Durable Keyed Transaction Commit Semantics

For a transaction `T` with:

```text
idempotency = DeduplicatedBy(K)
```

the framework defines a logical committed execution:

```text
Commit(T, K)
```

where `K` is evaluated for the current invocation.

## 6.1 First successful execution

If no committed `Commit(T,K)` exists:

1. the transaction body executes;
2. application mutations and transaction artifacts are produced;
3. the application state, retained artifacts, and `Commit(T,K)` are committed atomically.

Conceptually:

```text
                    atomic commit
                         │
          ┌──────────────┼──────────────┐
          ▼              ▼              ▼
 application state   Commit(T,K)   transaction artifacts
```

## 6.2 Replay after a successful commit

If `Commit(T,K)` already exists:

- the transaction body SHALL NOT be committed again;
- the prior committed execution SHALL be resolved;
- transaction artifacts retained by that committed execution SHALL be restored to the invocation artifact context.

The flow may therefore encounter the transaction step again, but this is **commit replay**, not a second execution of the transaction body.

## 6.3 Crash before commit

If execution fails before the atomic commit containing `Commit(T,K)` succeeds, no successful logical commit exists.

A subsequent attempt MAY execute the transaction body.

## 6.4 Concurrent attempts

Concurrent attempts with the same `(T,K)` SHALL NOT both successfully commit.

This is an implementation/conformance obligation of `DeduplicatedBy`.

---

# 7. Artifact Retention by a Keyed Commit

A successful keyed commit retains the logical artifacts established by that exact execution.

Conceptually:

```text
Commit(T,K)
    ├── InvocationResult R
    ├── EffectIntent E1
    └── EffectIntent E2
```

The association belongs to the keyed transaction-commit semantics.

It does **not** mean that `InvocationResult` or `EffectIntent` independently deduplicates the transaction.

On replay of `T(K)`, these same artifacts are recovered into the current invocation's artifact context.

A keyed replay SHALL NOT derive replacement artifacts from a newly executed transaction body, because the body is not re-executed after a successful commit.

---

# 8. Deterministic Derivation

Archspec SHOULD introduce a small provenance declaration for opaque value computation.

A candidate shape is:

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Derivation {
    Unspecified,

    Deterministic {
        from: Vec<ValueRef>,
    },
}
```

`Deterministic { from }` means:

> The produced values are a deterministic function solely of the declared source values.

It does **not** mean that those source values are necessarily stable across retries.

The analyzer separately determines replay stability of the provenance roots.

Therefore:

```text
deterministic derivation
        +
replay-stable provenance
        ↓
replay-deterministic produced value
```

---

# 9. Mutation Value Provenance

`Write` and `Insert` SHOULD declare the derivation of the values they produce.

A candidate shape is:

```rust
pub struct Write {
    pub target: ObjectSelector,
    pub fields: BTreeSet<FieldPath>,
    pub values: Derivation,
}

pub struct Insert {
    pub object: Id,
    pub values: Derivation,
}
```

## 9.1 `Write`

For a write to participate in a natural replay-safety proof, the analyzer must establish, at minimum:

1. replay stability of the selected logical target;
2. replay determinism of the resulting field values.

A `Write` with `values: Unspecified` normally prevents a natural replay-safety proof for that mutation.

## 9.2 `Insert`

`Insert` SHALL NOT redeclare object identity.

`DataObject.identity` remains the canonical non-empty identity of every logical object instance.

If the inserted object's complete contents are replay-deterministic, then its identity fields are correspondingly replay-deterministic. Repeated insertion therefore attempts to create the same logical object identity.

Uniqueness is intrinsic to `DataObject.identity`; a second distinct instance with the same identity is not a valid successful insertion.

This makes `AcquireUniqueClaim` redundant.

---

# 10. Transaction Read Results and Provenance

The DSL SHOULD model transaction-read results now, even though the V1 idempotency solver will not use them to prove natural replayability.

A candidate extension is:

```rust
pub struct Read {
    pub result: Id,
    pub target: ObjectSelector,
    pub fields: FieldSelection,
}
```

with:

```rust
pub enum ValueSource {
    Input(Id),
    Effect(Id),
    InvocationResult(Id),
    StateMachineSubject(Id),
    TransactionRead(Id),
}
```

A `TransactionRead(id)` value source denotes a field observed by the named read within the current transaction execution.

## 10.1 Scope

A transaction-read result is transaction-local.

It MAY be referenced by later steps in the same transaction.

It SHALL NOT implicitly become a durable cross-transaction artifact.

Validation SHOULD require that:

- the referenced read belongs to the same transaction;
- the read precedes its use in transaction step order;
- the referenced field was included in the read's `FieldSelection`.

## 10.2 Selector provenance

Because `ObjectSelector` already structurally represents predicates and `SelectorPredicate::Eq` already refers to literals or `ValueRef`s, selector provenance SHOULD be derived structurally.

No separate `predicate.deterministic` flag is required.

Selectors MAY eventually depend on preceding transaction-read values through `ValueSource::TransactionRead`.

---

# 11. V1 Rule for Read-Dependent Replay Analysis

V1 SHALL be deliberately conservative.

For natural transaction replayability analysis:

> If the provenance closure of a persistent mutation's target or produced values reaches a `TransactionRead`, V1 SHALL NOT prove natural replayability from that path.

The result SHOULD be `Unknown`, with a reason indicating dependence on transaction-observed mutable state.

Example:

```text
Read A -> r
Write B.values = deterministic_from(r.value)
```

V1:

```text
natural replayability = UNKNOWN
reason = mutation depends on transaction read
```

This does not mean the transaction is necessarily non-idempotent.

A future solver may establish replay stability of the read by analyzing, for example:

- all modeled writers of the observed field;
- whether the transaction itself mutates the object or field from which the read result was derived;
- object immutability;
- lifecycle constraints;
- serialization/concurrency guarantees;
- absence of intervening writers;
- implementation conformance regarding writers outside the model.

### 11.1 Self-modifying read dependencies

Deterministic derivation from a transaction read does **not** imply replay idempotency, even when no other process can modify the observed state. The transaction itself may change the value that a later retry will read.

For example:

```text
Read A.counter -> r
Write A.counter = deterministic_from(r.counter)
```

An implementation may deterministically compute `r.counter + 1`. Starting from `5`, the first execution writes `6`; a retry then reads `6` and writes `7`. The derivation is deterministic, but the transaction is not replay-idempotent.

Accordingly, future read-stability analysis must prove more than the absence of concurrent or external writers. If `R(S)` denotes the value observed by a relevant read in state `S`, and a successful transaction execution produces state `T(S)`, replay stability requires the observed value to be invariant under the transaction's own committed transformation whenever that read participates in a replay proof. At minimum, the solver must be able to establish the relevant form of:

```text
R(S) = R(T(S))
```

and must additionally account for any other admitted state transitions between attempts.

For V1, Archspec does not attempt this fixed-point/invariance analysis. **Any persistent mutation or artifact derivation whose provenance transitively reaches a `TransactionRead` prevents natural replay from being proven.**

The DSL records the provenance now so a future solver can perform this stronger analysis without a fundamental representation change.

---

# 12. InvocationResult Semantics

*(Since 2026-09-04 this artifact is `TransactionOutput` — a typed value a transaction deliberately exports to the enclosing operation program, with no response meaning of its own (V3 §7–§9). Every rule in this section applies to it unchanged.)*

`InvocationResult` is a logical result artifact produced by transaction execution.

It SHALL remain semantically separate from transaction idempotency.

In particular:

> Establishing an `InvocationResult` does not, by itself, prevent the enclosing transaction from executing or committing again.

## 12.1 Result value derivation

The establishment site SHOULD declare result-value provenance:

```rust
pub struct EstablishInvocationResult {
    pub result: Id,
    pub values: Derivation,
}
```

This describes how the logical result produced by that execution is derived.

## 12.2 Naturally replayed transaction

If:

- the establishing transaction is naturally replayable; and
- the result derivation is replay-deterministic;

then a retry MAY safely reconstruct the same logical `InvocationResult`.

No semantic requirement exists to durably store that result merely for replay.

## 12.3 Explicitly deduplicated transaction

If the establishing transaction uses `DeduplicatedBy(K)`:

- the successful transaction body is committed at most once;
- the exact `InvocationResult` produced by that committed execution SHALL be retained with `Commit(T,K)`;
- replay of `T(K)` SHALL recover that same result rather than recompute it.

## 12.4 Unsafe/unknown case

If:

- the establishing transaction is not proven naturally replayable;
- there is no explicit keyed transaction deduplication;
- and replay requires the result;

then replay consistency is not proven.

Archspec SHALL NOT pair an arbitrary previously memoized result with a newly committed non-idempotent execution.

---

# 13. EffectIntent Semantics

`EffectIntent` is a logical artifact describing an intended effect execution.

It SHALL NOT itself imply an invisible independent executor.

`ExecuteEffectIntent` remains the modeled authority that executes an established intent within an invocation flow.

The current `IntentExecutionSemantics::{Unspecified, Recoverable}` SHOULD therefore be removed or replaced rather than retained with its current meaning.

A minimal declaration may become:

```rust
pub struct EffectIntent {
    pub effect: Id,
}
```

with provenance declared at establishment:

```rust
pub struct EstablishEffectIntent {
    pub intent: Id,
    pub values: Derivation,
}
```

## 13.1 Naturally replayed transaction

If:

- the establishing transaction is naturally replayable; and
- the intent derivation is replay-deterministic;

then retrying the transaction MAY reconstruct the same logical effect intent.

This proves sameness of the intended effect, not safety of repeating the external effect.

## 13.2 Explicitly deduplicated transaction

If the establishing transaction uses `DeduplicatedBy(K)`:

- the exact effect intent established by the successful commit SHALL be retained with `Commit(T,K)`;
- replay of the transaction step recovers the retained intent rather than recomputing it.

## 13.3 Effect execution retry remains separate

Even a deterministically reconstructed effect intent does not establish whether the external effect has already occurred.

For example:

```text
ExecuteEffectIntent(E)
external effect succeeds
process crashes before local completion is known
```

may cause the effect execution to be attempted again.

Safety of this case depends on the modeled effect's own idempotency/retry semantics.

Artifact reconstruction and effect-execution idempotency are distinct concerns.

## 13.4 Transition-produced effect intents

A `Transition` may declare transition side effects. Under this revision, those side effects SHALL be interpreted as implicitly established logical `EffectIntent` artifacts, established atomically by the successful transition transaction. They follow the same artifact-retention rules as explicitly established effect intents.

The transition declares the effect contract; the applying `StateTransition` step declares the instance provenance:

```rust
pub struct StateTransition {
    pub machine: Id,
    pub transition: Id,
    pub subject: ObjectSelector,
    pub effect_values: BTreeMap<Id, Derivation>,
}
```

`effect_values` supplies one `Derivation` per side effect declared by the applied transition, keyed by side effect. The keys SHALL exactly match the transition's declared side effects; a transition without side effects uses an empty map. Each derivation is evaluated in the enclosing transaction context at the transition step, so it may reference preceding transaction reads under the usual read-before-use and field-selection rules.

Because every transition-containing transaction declares `DeduplicatedBy(K)`, these derivations are evaluated only during the first successful keyed execution; a later encounter recovers the exact original intents from `Commit(T,K)` without evaluating the derivations again. This permits transition effect values to depend on transaction-local reads even though those reads may not be replay-stable.

For V1, a transaction containing a `Transition` is not naturally replayable. Therefore transition-produced effect intents SHALL NOT be considered reconstructible merely because their payload derivation is deterministic. A retry cannot partially re-run the artifact-producing portion of a transaction while bypassing the transition that established it.

This creates an important crash boundary:

```text
Transaction T
    Transition pending -> paid
        establishes EffectIntent E
COMMIT

<crash>

ExecuteEffectIntent E
```

After `T` commits, a retry cannot naturally replay `T` to reproduce `E`, because the transition has already changed the subject state. Without durable transaction replay, the subsequent `ExecuteEffectIntent E` has no recoverable artifact to consume.

Accordingly, **V1 SHALL require every transaction containing a `Transition` to declare explicit durable keyed idempotency using `DeduplicatedBy(K)`.** The successful keyed commit SHALL retain transition-produced effect intents and any other transaction artifacts. A later encounter with the same `T(K)` resolves the prior commit, restores those artifacts, and does not execute the transition body again. *(Superseded 2026-09-05 by `CONSEQO_REVISION_V3_AMENDMENT_A_TRANSITION_DEDUP_RELAXATION.md`: the natural route remains unavailable, but the keyed guarantee is no longer structurally required — an unkeyed transition transaction is valid, and the obligations over it settle unproven where no recovery fact exists.)*

This rule provides crash recovery for flows that continue after a transition transaction, including flows whose next step executes an effect intent or consumes an invocation result established by the transition transaction.

---

# 14. Cross-Transaction Artifact Visibility

Invocation flows are ordered compositions of transactions and effect executions. *(Since 2026-09-04 the composition is the operation program, and "subsequent steps" means the steps on every path that the transaction dominates: availability is a forward definite-availability fact, intersected at joins — V3 §28.)*

A logical transaction artifact established by a successful transaction SHALL remain available to subsequent flow steps in the same invocation.

For example:

```text
Transaction T1
    EstablishInvocationResult(R)

Transaction T2
    Write(... derived from R ...)
```

is semantically valid provided `T1` successfully makes `R` available before `T2` executes.

A subsequent transaction may therefore reference an available `InvocationResult` through `ValueSource::InvocationResult`.

Artifact availability may arise from:

1. production by the transaction earlier in the current invocation;
2. deterministic reconstruction during a natural replay of that transaction;
3. recovery from the prior `Commit(T,K)` when the transaction is explicitly deduplicated.

No explicit `RecoverInvocationResult` or `RecoverEffectIntent` flow step is required.

Transaction-read results are excluded from this rule: they remain local to their transaction execution.

---

# 15. Invocation Artifact Context

For semantic analysis, an invocation may be understood as carrying an abstract artifact context.

This need not be represented in the DSL.

Conceptually:

```text
ArtifactContext
    InvocationResult R -> logical result value
    EffectIntent E     -> logical effect-intent value/state
```

After a transaction step:

- newly established artifacts enter the context;
- a natural replay may reconstruct the same artifacts;
- a keyed transaction replay restores artifacts retained by the prior committed execution.

Subsequent flow steps consume logical artifacts from this context.

This is semantic bookkeeping, not a new user-visible workflow abstraction.

---

# 16. InvocationResult and Durable Storage

Under this revision, an `InvocationResult` is not inherently synonymous with a durable database record.

Its logical availability on retry can be established through deterministic reconstruction.

Durable retention is mandatory when the result is part of an explicitly keyed transaction commit whose body will not execute again on replay.

This suggests that the current artifact-level `InvocationResult.key` MAY be redundant under the revised model, because keyed durable identity can instead be inherited from the committed transaction that retains the result.

This draft therefore proposes reconsidering:

```rust
pub struct InvocationResult {
    pub key: IdempotencyKey,
    pub schema: Id,
}
```

in favor of a declaration closer to:

```rust
pub struct InvocationResult {
    pub schema: Id,
}
```

**Draft status:** removal of the artifact-level key should be finalized only after validating all cross-transaction and response-replay use cases.

---

# 17. EffectIntent and Durable Storage

The same principle applies to effect-intent payload durability.

A deterministic intent associated with a naturally replayable transaction may be reconstructed.

A retained intent associated with `Commit(T,K)` must remain recoverable because the keyed transaction body will not execute again.

This does not remove the need for durable effect-execution state where an architecture requires it. In particular, tracking whether an external effect has completed is a separate problem from reconstructing the intent's contents.

---

# 18. Response Replay

*(Superseded 2026-09-04 by V3 §14–§16 and §33. `Response` and `ResponseSource` are removed; a request invocation terminates at a `return` that constructs the `RequestInput.result` contract from an ordinary derivation, and the requirement is `result: replay_consistent`. The two routes below survive as the ways a `transaction_output` the terminal derives from becomes replay-stable — reconstruction (R5) or recovery (R4) — to which the implemented checker adds that every decision on the returning path must replay, so a class reaches one terminal and hence one variant. The section is kept as the rationale.)*

`ResponseSource::InvocationResult` SHALL mean:

> The response is obtained from the logical `InvocationResult` available to the current invocation.

It should no longer imply, merely from the source variant, that the result was independently stored as an intrinsically durable artifact.

For an operation requiring:

```text
ResponseReplayRequirement::ReplayConsistent
```

the solver must prove a safe route to the same logical result, such as:

1. naturally replayable transaction + replay-deterministic result derivation; or
2. keyed transaction deduplication + recovery of the result retained by the prior committed execution.

---

# 19. Removal of UniqueClaim

`AcquireUniqueClaim` and `UniqueClaim` SHOULD be removed.

`DataObject.identity` already defines the identity of one logical persistent instance.

`Insert` operates on a `DataObject` whose identity is therefore necessarily unique.

Natural replay analysis of an insert should combine:

- deterministic/replay-stable inserted contents; and
- intrinsic uniqueness of the object's declared identity.

A separate uniqueness-claim transaction primitive is unnecessary.

---

# 20. Removal of ReadInvocationResult

`TransactionStep::ReadInvocationResult` SHOULD be removed unless a separate concrete semantic requirement emerges.

An invocation result is already an artifact that may be referenced through `ValueSource::InvocationResult`.

Cross-transaction availability should be governed by the artifact-availability rules in this revision rather than by an explicit recovery/read transaction step.

---

# 21. Flow Semantics

*(Superseded 2026-09-04 by V3 §22–§23. `Operation.flows` and `FlowStep` are replaced by one `Operation.program` of `OperationStep`s — the three step kinds below, plus `match_result`, `branch`, `return`, and `complete`; `execute_effect` and `execute_effect_intent` may bind a synchronous result. What this section says about `ExecuteEffect.values` and about `ExecuteEffectIntent` declaring no derivation remains true, and no recovery wrapper was introduced.)*

The existing small flow vocabulary SHOULD remain, with direct execution declaring instance provenance:

```rust
pub enum FlowStep {
    Transaction { transaction: Id },
    ExecuteEffect { effect: Id, values: Derivation },
    ExecuteEffectIntent { intent: Id },
}
```

`ExecuteEffect.values` declares the provenance of the complete logical effect instance constructed and executed by the step. It is evaluated in the operation-level value context and may not reference transaction reads, because no transaction is in scope at flow level. Natural replay of a direct execution requires the derivation to be replay-deterministic.

`ExecuteEffectIntent` deliberately declares no derivation: it consumes an effect instance whose values were fixed when the intent was established, and must never recompute or replace them.

No `RecoverInvocationResult`, `RecoverEffectIntent`, `TransactionExecution`, or other recovery wrapper is introduced.

Recovery is a semantic consequence of re-encountering an explicitly keyed transaction step whose logical commit already exists.

---

# 22. V1 Transaction Replay Analysis

V1 SHOULD compute transaction replayability using two independent proof routes.

## 22.1 Natural route

Analyze the transaction body.

A natural proof may use:

- mutation target stability;
- deterministic mutation derivation;
- replay-stable provenance;
- unique object identity;
- absence of persistent mutation.

If required provenance is unspecified, natural replayability is `Unknown`.

If mutation target/value or artifact provenance reaches a `TransactionRead`, V1 returns `Unknown` for the natural proof route. This includes self-modifying dependencies where a transaction reads state that it later changes; deterministic computation from the read does not make the retry deterministic because the retry may observe the state produced by the first execution.

If the transaction contains any `Transition`, the V1 natural route is unavailable. Such a transaction MUST use `DeduplicatedBy(K)` so that the successful commit and its artifacts can be recovered without re-executing the transition. *(Superseded 2026-09-05 by `CONSEQO_REVISION_V3_AMENDMENT_A_TRANSITION_DEDUP_RELAXATION.md`: the natural route remains unavailable, but the keyed guarantee is no longer structurally required — an unkeyed transition transaction is valid, and the obligations over it settle unproven where no recovery fact exists.)*

## 22.2 Explicit route

If:

```text
Transaction.idempotency = DeduplicatedBy(K)
```

then transaction re-commit safety is established by the durable keyed-commit guarantee.

The solver need not prove that the transaction body would naturally produce the same state if executed twice, because a second successful body commit is forbidden.

---

# 23. V1 Artifact Replay Analysis

For each artifact needed after a retry, the solver SHOULD establish one of:

```text
A. reconstruction
   establishing transaction naturally replayable under the V1 natural route
   +
   artifact derivation replay-deterministic

OR

B. recovery
   establishing transaction DeduplicatedBy(K)
   +
   artifact retained by Commit(T,K)
```

Otherwise artifact replay availability/consistency is `Unknown`.

A transaction containing a `Transition` cannot use reconstruction route A in V1. Its artifacts are replay-available only through the durable keyed-commit recovery route.

---

# 24. V1 Operation Idempotency

An operation-level idempotency requirement is not satisfied merely because:

- an `InvocationResult` exists;
- an `EffectIntent` exists;
- a transaction contains an `Insert`;
- an effect is retryable.

The solver must compose the relevant proofs across the admitted invocation flow — since 2026-09-04, across each admitted path of the operation program, judging every decision on the path where it is taken (V3 §30, §48.2).

At minimum it may need to reason about:

- transaction replayability / durable commit recovery;
- artifact replay availability;
- response (now result) replay consistency;
- decision replay — whether a retry takes the same arm at each `match_result` and `branch`;
- effect execution idempotency;
- idempotency-key propagation;
- ordering/serialization requirements where relevant.

---

# 25. Candidate Structural Diff

The following is a **draft shape**, not a mandate to implement these exact field names before review.

*(Annotated 2026-09-04. As implemented and then revised by V3: `EstablishInvocationResult { result, values }` is `EstablishTransactionOutput { output, values }`; `InvocationResult { schema }` is `TransactionOutput { schema }`; `ValueSource::InvocationResult(Id)` is `ValueSource::TransactionOutput(Id)`, and the enum gained `EffectResultOk(Id)` / `EffectResultErr(Id)`; the `FlowStep` enum at the end of this section is superseded by `OperationStep` (V3 §23), whose `ExecuteEffect` and `ExecuteEffectIntent` carry `result: Option<Id>`. The transaction, derivation, read, write, insert, effect-intent, and state-transition shapes are as implemented.)*

```rust
pub struct Transaction {
    pub data_model: Option<Id>,
    pub isolation: TransactionIsolation,
    pub idempotency: IdempotencyGuarantee,
    pub steps: Vec<TransactionStep>,
}

pub enum Derivation {
    Unspecified,
    Deterministic { from: Vec<ValueRef> },
}

pub struct Read {
    pub result: Id,
    pub target: ObjectSelector,
    pub fields: FieldSelection,
}

pub struct Write {
    pub target: ObjectSelector,
    pub fields: BTreeSet<FieldPath>,
    pub values: Derivation,
}

pub struct Insert {
    pub object: Id,
    pub values: Derivation,
}

pub struct EstablishInvocationResult {
    pub result: Id,
    pub values: Derivation,
}

pub struct EstablishEffectIntent {
    pub intent: Id,
    pub values: Derivation,
}

pub struct StateTransition {
    pub machine: Id,
    pub transition: Id,
    pub subject: ObjectSelector,
    pub effect_values: BTreeMap<Id, Derivation>,
}

pub enum ValueSource {
    Input(Id),
    Effect(Id),
    InvocationResult(Id),
    StateMachineSubject(Id),
    TransactionRead(Id),
}

pub struct InvocationResult {
    pub schema: Id,
}

pub struct EffectIntent {
    pub effect: Id,
}
```

Candidate removals:

```text
TransactionStep::AcquireUniqueClaim
UniqueClaim

TransactionStep::ReadInvocationResult
ReadInvocationResult

IntentExecutionSemantics
EffectIntent.execution

InvocationResult.key    // pending final review
```

Flow shape, with direct execution declaring instance provenance:

```rust
pub enum FlowStep {
    Transaction { transaction: Id },
    ExecuteEffect { effect: Id, values: Derivation },
    ExecuteEffectIntent { intent: Id },
}
```

---

# 26. Examples

## 26.1 Naturally replayable write and result

```text
input.request : request
  identity: keyed [request_id]

operation idempotency key: [input.request_id]

Transaction T
  idempotency: unspecified

  Write File.contents
    target: File[id = input.file_id]
    values: deterministic_from(input.contents)

  EstablishInvocationResult R
    values: deterministic_from(input.file_id)
```

The declared request identity, pinned by the operation's idempotency
key, makes `input.file_id` and `input.contents` replay-stable for the
relevant logical invocation (`ARCHSPEC_REPLAY_STABILITY_DRAFT.md`;
main document §18). Then:

```text
retry T
    → same target
    → same file contents
    → same R
```

No durable transaction commit record is required for replay correctness.

---

## 26.2 Read-dependent mutation in V1

```text
Transaction T
  Read Account -> account
  Write Invoice
    values: deterministic_from(account.tier)
```

V1:

```text
natural replayability = UNKNOWN
```

because the mutation depends on transaction-observed state.

The DSL retains enough provenance for a future solver to attempt to prove that `Account.tier` cannot change between retries.

---

## 26.3 Read-dependent mutation with explicit keyed deduplication

```text
Transaction T
  idempotency: DeduplicatedBy(request_id)

  Read Account -> account
  Write Invoice
    values: deterministic_from(account.tier)

  EstablishInvocationResult R
```

First successful execution:

```text
execute T
commit application changes
retain R
persist Commit(T, request_id)
```

Retry:

```text
resolve Commit(T, request_id)
do not execute transaction body
recover R
```

V1 does not need to prove replay stability of `account.tier`.

---

## 26.4 Effect intent under natural replay

```text
Transaction T
  naturally replayable
  EstablishEffectIntent E
    deterministic_from(input.order_id, input.payload)

ExecuteEffectIntent E
```

A retry may reconstruct the same logical intent.

However, if the external effect may already have occurred, repeated `ExecuteEffectIntent` still requires an appropriate effect-level idempotency/retry proof.

---

## 26.5 Effect intent under keyed transaction deduplication

```text
Transaction T
  idempotency: DeduplicatedBy(request_id)
  EstablishEffectIntent E

ExecuteEffectIntent E
```

After `T` commits, a retry of the flow:

```text
Transaction T
    → resolve prior Commit(T, request_id)
    → recover retained E

ExecuteEffectIntent E
```

No `RecoverEffectIntent` flow step is necessary.

---

## 26.6 Transition transaction and crash recovery

```text
Transaction T
  idempotency: DeduplicatedBy(request_id)

  Read Order -> order

  Transition Order: pending -> paid
    side effect: PaymentCaptured
    effect_values:
      PaymentCaptured = deterministic_from(order.order_id, input.amount)

  EstablishInvocationResult R

ExecuteEffectIntent PaymentCaptured
Response R
```

The transition transaction is not eligible for V1 natural replay. On the first successful execution, the transition side-effect instance is constructed from its `effect_values` derivation — which may reference the preceding transaction read — and `Commit(T, request_id)` atomically retains the transition-produced effect intent and `R`. If the invocation crashes after the transaction commits but before `ExecuteEffectIntent`, retrying the flow resolves the prior commit, restores those artifacts, and continues without attempting the state transition again. The effect derivation is not evaluated again.

Without `DeduplicatedBy(request_id)`, V1 rejects this transaction shape as a recoverable/idempotent replay boundary because the committed transition cannot be naturally replayed to reproduce its artifacts.

---

# 27. Open Questions Before Finalization

The semantic direction is coherent, but the following should be explicitly resolved before calling the revision final:

1. **Artifact-level keys**  
   Confirm whether `InvocationResult.key` should be removed entirely, and whether `EffectIntent` needs any independent logical identity outside a keyed transaction commit.

2. **Artifact derivation granularity**  
   Confirm whether `Derivation` on `EstablishEffectIntent` describes the entire intended effect payload, and whether additional effect-payload lineage is needed.

3. **Replay-stable provenance roots** — *Resolved, 2026-08-20.*  
   The exact V1 rules are defined in `ARCHSPEC_REPLAY_STABILITY_DRAFT.md` and reconciled into the main document (§6 `message_identity`, §8.1 `RequestInput.identity`, §12 governing keys, §18 rules). Stability is definitional (governing-key components), declared (a request or message identity pinned by the governing key), or derived (keyed-commit recovery, natural-replay reconstruction, congruence); everything else is `Unknown`.

4. **Insert failure semantics**  
   Normatively define the result of attempting to insert an already-existing `DataObject.identity` and how that affects the enclosing transaction and flow applicability.

5. **Future transition replay analysis**  
   V1 requires explicit keyed idempotency for every transition-containing transaction. A later solver may investigate whether restricted transition patterns admit safe replay without durable commit recovery, but no such inference is permitted in V1.

6. **Effect execution completion state**  
   Keep intent reconstruction separate from durable tracking of effect execution/completion, and decide what minimum execution-state semantics V1 requires.

7. **Alternative flow applicability** — *Open; V1 stance adopted 2026-08-21, restated per path 2026-09-04.*  
   Formalize how candidate flows whose required transaction/artifact preconditions cannot be satisfied are treated by the analyzer. This remains unresolved: `ARCHSPEC_FLOW_RESUMPTION_DRAFT.md` adopts same-flow continuation as a sufficient recoverability route that neither uses nor forbids alternative-flow continuations, so a future resolution may add routes but cannot invalidate V1 proofs. Since the operation-execution revision (V3 §48.2) the stance applies **per path of the operation program**: recoverability is established by same-path continuation for every admitted path, a decision a retry may take differently is no obstacle to progress (each arm is its own path and is analyzed on its own), and the question of which *other* paths a resumed attempt may follow after a partial execution is exactly as open as before.

8. **Locking expressivity** — *Open; earmarked for implementation 2026-08-21.*  
   A `Lock` is one selector, a mode, and an acquisition order within that selector (§21). That surface cannot state the facts a deadlock argument needs, and the gaps are these:

   - *Instance sets.* `SelectorPredicate` admits only `all`, `eq`, and `and` — no disjunction, no set membership — so one lock step cannot name several specific instances of an object. `tx.transfer_stock` in `tests/fixtures/flash_checkout.yaml` locks its source and destination `stock` rows as two steps for exactly this reason. Needs `or`/`in` over a field (with §19's provenance rules extended to them), or a lock step with several targets.
   - *Cross-step and cross-object order.* `LockOrder::by` orders acquisition only inside one step; between steps the only fact is program order, which is data-relative for a transfer (source before destination), and nothing orders locks on different objects ("`order` before `stock`"). Needs a transaction-level lock order — a sequence of objects, each with a field order — or a data-model-level ordering convention that transactions cite.
   - *Order-domain compatibility.* §21 permits deadlock reasoning only between "compatible order domains" without defining them. Needs a definition: same object, one declared order a common prefix of the other, same directions; or one declared total order per object.
   - *Preconditions and distinctness.* Selectors reference input values, so whether two of them address one instance or two depends on the inputs, and nothing can state `source_warehouse_id ≠ destination_warehouse_id`. Needs a decision on input preconditions; without them the degenerate same-instance case of a transfer is unstatable and its two writes conflict.
   - *Upgrades.* A `shared` then `exclusive` lock on one target within a transaction is the classic deadlock (two holders both upgrading); the DSL allows writing it and gives it no semantics. Needs one: conversion or a second lock.
   - *Implicit locks.* Engines take row locks on `Write`/`Delete` and gap locks on `Insert` under the stronger isolation levels; the DSL models only explicit `Lock` steps. Needs either implicit-lock facts per isolation level (a `Write` acquires `exclusive` on its selector at its program point) or a stated assumption that only declared locks count — which makes every proof conditional on the engine's conformance to that assumption.
   - *Predicate versus instance locks.* Whether a lock on `all` or on a partial identity covers instances inserted later (a predicate lock) or only current ones is unspecified; serialization and deadlock reasoning both depend on it.
   - *Wait policy.* No lock-wait timeout, `nowait`, or `skip locked` fact; these decide whether a circular wait deadlocks or aborts. Absent one, a checker must treat every cycle as a deadlock.

   No V1 proof credits a lock — the serialization checker deliberately declines the lock route — so each of these can only add what can be stated and proven, never invalidate a verdict.

9. **Deadlock checker against the entire model** — *Open; earmarked for implementation 2026-08-21; depends on 8 to be useful.*  
   A model-wide analysis, not a per-operation requirement: locks live in transactions, and a deadlock is a property of every transaction the model admits concurrently on one data model. The analysis, per data model:

   1. *Collect* every explicit lock step of every transaction in every operation — object, selector, mode, declared order, program position — and, once question 8 settles it, the implicit locks its isolation level implies.
   2. *Abstract* each lock to a class: the object and the shape of its selector (full identity, partial identity, `all`), with selector values carried as canonical paths (`value_identity.rs`), so that two classes are *disjoint* only when provably so (distinct literals, or identities pinned to different canonical values) and otherwise *may overlap*. Two classes *conflict* when they may overlap and are not both `shared`.
   3. *Order* the classes: within a transaction, program order between steps and the `by` order within a step give a per-transaction acquisition order over conflicting classes; a multi-instance step with `order: unspecified` contributes no order among its own instances.
   4. *Admit* concurrency: two transactions can overlap unless a declared fact says otherwise — `bounded(1)` on an operation, a proven serialization requirement for same-key invocations, lane concurrency. The serialization verdicts already compute most of this.
   5. *Decide.* The union of the admitted transactions' acquisition orders over conflicting classes is acyclic: **proven**, citing the global order it found. A cycle whose every edge is a declared fact, whose transactions are admitted concurrently, and whose classes may overlap: **disproven**, with a counterexample trace in the report's existing `counterexample` shape — "T1 holds A, requests B; T2 holds B, requests A" — the checker's first disproven verdict, consistent with §1.2 because it is built from declarations, not from their absence. Anything else — an `unspecified` order on a multi-instance class, an unspecified concurrency bound, overlap that cannot be decided — is **unknown**, with the lock steps it hinges on as evidence.

   To settle alongside: whether deadlock freedom is declared (a `DataModel.requirements.deadlock_freedom`, on the `ObjectHistoryRequirement` precedent — since removed, V3 §3, so the precedent is now historical — keeping the rule that requirements are obligations) or standing; a `data_model` subject kind for the report; the viz rendering of a disproven obligation with its trace; and the wait-policy assumption of question 8. Until question 8 lands, the analysis is implementable but would return unknown for nearly every real model, the transfer pattern included — which is still the honest answer.

10. **Input-specific path admission** — *Open; V1 stance adopted 2026-09-04 (V3 §46.5, §48.1).*  
    An operation may declare several inputs, and its one program does not say which input an invocation entered through. V1 relates a path to an input only through its terminal: a path is *admitted* for triggering input `i` iff it ends at `complete` or at `return` for `i` — the direct generalization of the retired admitted-flow rule (a flow with no response, or with `i`'s response). This is sufficient for the fixtures but weaker than the model knows: a `complete`-terminated path is admitted for every input, including a request input whose invocations then return nothing; a program with two request inputs cannot state that a step is reachable only through one of them, so both populations are analyzed over it; and a `return` for another input excludes a path without saying what an `i`-invocation does instead. An explicit entry concept — a per-input entry block, an `entry` step naming the inputs that may reach the steps it dominates, or a validation rule that every request input has at least one `return` — would let validation reject a program that returns nothing for a request input, let each population analyze only the steps it can reach, and give path admission a declared rather than inferred basis. It was deliberately not guessed at (V3 §46.5); any resolution refines admission and so can only remove paths from an analysis, never add work to a proven one.

11. **External effect result replay** — *Open; exposed 2026-09-04 (V3 §31, §48.2, §48.6).*  
    An `ExternalEffect` may declare a `result: Result<Ok, Err>` contract and a program may bind and match on it, but no declared fact makes an external boundary's returned result replay-consistent, so a decision on an external result is never established to replay, and a value derived from `effect_result_ok` / `effect_result_err` of one is never replay-stable. Every program branching on a provider's answer is therefore unproven for idempotency and result replay (`charge_payment` in flash checkout, `transcode_video` in video streaming), however the boundary actually behaves. `deduplicated_by { key }` is insufficient and must not be read as implying it: that guarantee collapses the *work* of same-key executions and says nothing about what each execution is *answered* with — a boundary may legitimately answer a duplicate with a distinct "already processed" error, a different variant from the original `Ok`. A guarantee that discharges the gap would have to state, at the boundary and as a separate declaration, that executions sharing the deduplication key (or a class-fixed instance) are answered with the same result variant and a replay-equivalent payload — the external counterpart of a target operation's proven `result: replay_consistent`, and like it an implementation guarantee every proof using it is conditional on (§1.3 of the main document). Whether it should be scoped to key equality alone, as `deduplicated_by` is, or to instance equality, and whether a partial guarantee (same variant, unspecified payload) is worth a vocabulary of its own, are the design questions.

These questions do not require introducing recovery-specific flow steps.

---

# 28. Normative Summary

The intended model can be summarized as follows:

> A transaction may be replayable either because its declared operations can be safely and deterministically re-executed or because the execution environment durably deduplicates successful commits by an explicit idempotency key and recovers the prior committed execution.

> Mutation and artifact value computations may declare deterministic provenance without exposing their implementation expressions.

> Transaction-read results are first-class provenance sources, but V1 does not use any read-dependent mutation or artifact derivation to prove natural replay. Deterministic computation from a read is insufficient because the transaction itself may change the state observed by a retry.

> `InvocationResult` and `EffectIntent` are logical transaction artifacts. They are not inherently transaction-idempotency mechanisms.

> Naturally replayable transactions may reconstruct replay-deterministic artifacts. Transactions containing state transitions are not naturally replayable in V1 and must declare explicit durable keyed idempotency. Explicitly keyed transactions commit at most once and durably retain the exact artifacts of the successful logical commit for recovery on later encounters.

> Invocation flows remain simple ordered sequences of transaction and effect-execution steps. Recovery is not represented as a separate flow action. *(Superseded 2026-09-04: an operation has one structured, acyclic program with explicit `match_result` / `branch` decisions and `return` / `complete` terminals — V3 §22 — and each of its paths is such a sequence. Recovery is still not a step.)*

> Durable result memoization must never be used to conceal a later inconsistent non-idempotent transaction execution. If neither natural replayability nor explicit keyed commit deduplication can establish a coherent replay path, the analyzer reports the relevant requirement as unproven.
