# Archspec Operation Execution Revision Draft
## Revision 2 — Control Flow, Typed Results, and Transaction Outputs

**Status:** Implemented 2026-09-04. The §46 decisions, the V1 analysis as built, the validation, the report format, the fixture outcomes, and the follow-ups the implementation exposed are recorded in §48. The proposals themselves (§1–§47) stand as written.  
**Baseline:** current `master` as of 2026-09-02  
**Scope:** Next-iteration operation execution semantics and the transformation of the current `InvocationFlow`, `Response`, and `InvocationResult` surface.

---

# 1. Design goals

This revision makes four coordinated changes:

1. Replace alternative complete `InvocationFlow`s with one explicit operation control structure.
2. Introduce a first-class Rust-like `Result<Ok, Err>` tagged sum for synchronous request/effect outcomes.
3. Replace response-specific `InvocationResult` with a generic `TransactionOutput` artifact.
4. Retain existing transaction-level keyed idempotency and effect-intent recovery semantics.

The target decomposition is:

```text
Operation control
    = causal execution structure

Result<Ok, Err>
    = mutually exclusive typed outcome

TransactionOutput
    = typed information deliberately exported
      from a transaction into the enclosing operation

EffectIntent
    = captured logical effect instance
      intended for later execution

Transaction DeduplicatedBy(K)
    = durable at-most-once logical transaction commit
      plus recovery of that commit's exact artifacts
```

Each primitive should represent one architectural fact.

---

# 2. Explicit non-goals

Do not introduce core primitives for:

```text
workflow history
checkpoint
activity
signal
update
durable program counter
generic workflow resumption
```

Those are runtime/framework implementation mechanisms.

Do not introduce general loops in this iteration. The first operation-control representation is intentionally acyclic.

---

# 3. Defer object-history requirements

The next iteration should phase out the current object-history requirement surface.

Current `DataObject` includes:

```rust
pub struct DataObject {
    pub schema: Id,
    pub identity: Vec<FieldPath>,
    pub requirements: ObjectRequirements,
}

pub struct ObjectRequirements {
    pub history: BTreeSet<ObjectHistoryRequirement>,
}

pub enum ObjectHistoryRequirement {
    Linearizable,
}
```

For the next iteration, remove:

```text
DataObject.requirements
ObjectRequirements
ObjectHistoryRequirement
ObjectHistoryRequirement::Linearizable
```

unless another non-history object requirement is introduced before implementation freeze.

`DataObject.identity` is retained unchanged. Object identity remains necessary for:

```text
selector precision
insert uniqueness
alias/interference analysis
locking
state-machine subject identity
transaction reasoning
```

Its usefulness is independent of object-history requirements.

## Why defer this surface

Linearizability is a meaningful correctness property beyond replicated systems: even a single logical store may need real-time-consistent object histories under concurrent access.

However, object-history consistency becomes especially important when the architecture models distributed storage concerns such as:

```text
replication
leader/follower or multi-leader topology
replica read/write routing
consensus/quorum behavior
failover
propagation lag
partition behavior
availability guarantees
read freshness / session consistency
```

Archspec does not yet model those concerns.

At the current stage the DSL is primarily reasoning about:

```text
application operations
transaction atomicity/isolation
locks
message ordering
execution serialization
idempotency
recoverability
effect behavior
control flow
```

Keeping a first-class linearizability requirement now would therefore introduce a substantial object-history proof domain before the model exposes the distributed storage facts that will make that domain most useful.

The next iteration should deliberately narrow scope rather than retain a requirement merely because it is theoretically meaningful.

## What is not being weakened

Removing the object-history requirement does **not** change existing transaction semantics.

Continue to model and reason about:

```text
TransactionIsolation
serializable transactions
explicit locks
lock ordering
object identity
selector overlap
transaction conflicts
operation serialization
operation ordering
```

In particular:

```text
serializable
```

continues to mean transaction serializability according to the existing transaction contract.

The DSL simply stops asking the separate object-history question:

```text
Do all observations/mutations of object instance O admit a legal
sequential history respecting real-time precedence?
```

for now.

Do not reinterpret serializability as linearizability after this removal.

## Solver consequence

The next-iteration solver should not emit:

```text
Proven / Violated / Unknown
```

outcomes for object linearizability.

No attempt should be made to infer linearizability implicitly from:

```text
serializable isolation
locks
operation serialization
topic ordering
```

Those facts retain only their own declared semantics.

## Validation consequence

Remove validation specific to:

```text
ObjectRequirements.history
ObjectHistoryRequirement
linearizable
```

Existing `DataObject` validation remains responsible for:

```text
schema resolution
non-empty complete identity
identity field-path validity
```

and any other non-history structural rules.

Existing model fixtures containing:

```yaml
requirements:
  history:
    - linearizable
```

must be migrated by removing that declaration.

## Documentation consequence

Remove current normative language that presents object linearizability as an active requirement.

References that say object identity participates in a "linearizability domain" should be revised so identity is justified only by currently retained analyses.

Historical/design documentation may note that object-history requirements are intentionally deferred.

Do not erase the semantic distinction between linearizability and serializability from design notes if it is useful for later reintroduction; simply remove it from the active DSL contract.

## Reintroduction boundary

Object-history requirements should be reconsidered when Archspec begins modeling distributed persistence and availability semantics.

Before reintroducing `linearizable` or adding other history models, the design should first establish the facts from which such requirements can actually be proved, potentially including:

```text
replica membership/topology
authoritative write location
read routing
write/read quorum semantics
consensus or leader guarantees
replication propagation semantics
partition/failure assumptions
availability guarantees
real-time observation boundaries
```

At that point, object history may return as a coherent family of requirements rather than as an isolated `linearizable` flag.

Potential future history properties such as:

```text
linearizability
sequential consistency
causal consistency
eventual convergence
```

must not be predeclared now. Their exact vocabulary should be derived from the future replication/availability model.

## Scope rule

For this iteration:

> Archspec models transaction and operation correctness without declaring end-to-end persistent-object history consistency requirements.

This is an explicit scope decision, not a claim that object-history properties are unimportant.

---

# 4. Current surface being transformed

Current operation execution is based on:

```rust
Operation {
    ...
    invocation_results: BTreeMap<Id, InvocationResult>,
    responses: BTreeMap<Id, Response>,
    transactions: BTreeMap<Id, Transaction>,
    flows: BTreeMap<Id, InvocationFlow>,
    ...
}

InvocationFlow {
    steps: Vec<FlowStep>,
    response: Option<Id>,
}

FlowStep {
    Transaction { transaction: Id },

    ExecuteEffect {
        effect: Id,
        values: Derivation,
    },

    ExecuteEffectIntent {
        intent: Id,
    },
}
```

The next iteration removes:

```text
Operation.flows
InvocationFlow
FlowStep

Operation.invocation_results
InvocationResult
EstablishInvocationResult
ValueSource::InvocationResult

Operation.responses
Response
ResponseSource
flow.response
```

Their useful semantics are redistributed into more general primitives below.

---

# 5. Retain explicit transaction idempotency unchanged

For:

```text
Transaction T
    idempotency = DeduplicatedBy(K)
```

the environment provides a durable logical commit:

```text
Commit(T,K)
```

The first successful execution atomically commits:

```text
application state
+
Commit(T,K)
+
transaction artifacts
```

A later encounter with the same `(T,K)`:

```text
does not commit the transaction body again
+
resolves the prior Commit(T,K)
+
restores the exact transaction artifacts retained by that commit
```

This remains transaction-level durability only.

It does not imply operation checkpointing or durable workflow execution.

---

# 6. Transaction artifact taxonomy

After this revision the two principal framework transaction artifacts are:

```text
TransactionOutput
EffectIntent
```

They have deliberately different jobs.

## `TransactionOutput`

A typed logical value deliberately exported from a transaction into subsequent operation control.

It represents data.

## `EffectIntent`

A captured logical effect instance intended for later execution.

It represents pending logical work.

## Rule

Do not use an `EffectIntent` merely to transport arbitrary transaction data.

Do not use a `TransactionOutput` to represent work awaiting execution.

Both may be established by the same transaction and both may be retained by `Commit(T,K)`, but they are not interchangeable.

---

# 7. Replace `InvocationResult` with `TransactionOutput`

Introduce:

```rust
pub struct TransactionOutput {
    pub schema: Id,
}
```

Operation surface:

```rust
pub transaction_outputs: BTreeMap<Id, TransactionOutput>,
```

Replace:

```rust
TransactionStep::EstablishInvocationResult
```

with:

```rust
TransactionStep::EstablishTransactionOutput
```

Recommended shape:

```rust
pub struct EstablishTransactionOutput {
    pub output: Id,
    pub values: Derivation,
}
```

Replace:

```rust
ValueSource::InvocationResult(Id)
```

with:

```rust
ValueSource::TransactionOutput(Id)
```

---

# 8. `TransactionOutput` semantics

For:

```text
EstablishTransactionOutput(O, D)
```

the transaction:

1. constructs a value shaped by `O.schema`;
2. declares its provenance through `D`;
3. establishes `O` atomically with the transaction commit;
4. makes `O` available to subsequent operation control after successful execution or commit recovery.

A `TransactionOutput` does **not** imply:

```text
operation response
success/failure
effect execution
idempotency
database storage
memoization
```

Its single meaning is:

> This transaction deliberately exposes this typed logical value to the enclosing operation.

---

# 9. Transaction-output replay

For a naturally replayable transaction:

```text
T
    EstablishTransactionOutput(O, D)
```

the same `O` may be reconstructed if `D` is replay-deterministic.

For:

```text
T.idempotency = DeduplicatedBy(K)
```

the first successful `Commit(T,K)` retains the exact output value.

A retry:

```text
resolves Commit(T,K)
restores O
does not recompute D
```

The same recovery rule applies to `EffectIntent`.

Conceptually:

```text
Commit(T,K)
    |
    +-- TransactionOutput O
    |
    +-- EffectIntent I
```

---

# 10. Transaction encapsulation

`ValueSource::TransactionRead` remains transaction-local.

It may only be referenced by later steps in the same transaction execution.

If information observed or computed inside a transaction must influence later operation control, it must be explicitly exported through a `TransactionOutput`.

The intended boundary is:

```text
transaction-local observation
        |
        | EstablishTransactionOutput
        v
operation-visible value
```

Effect intents cross the same transaction boundary for a different reason: they carry captured executable work.

---

# 11. First-class `Result<Ok, Err>`

Introduce:

```rust
pub struct ResultType {
    pub ok: Id,
    pub err: Id,
}
```

where the IDs name schemas.

Semantically:

```text
Result<OkSchema, ErrSchema>
```

is a tagged sum containing exactly one of:

```text
Ok(ok_payload)
Err(err_payload)
```

Mutual exclusivity is structural.

Archspec models the algebraic outcome, not Rust's API or method set.

---

# 12. `Err` is not an interrupted execution

A crucial distinction:

```text
Err(E)
```

means a synchronous interaction completed and returned a modeled logical failure outcome.

It does not automatically represent:

```text
process crash
timeout
lost connection
remote completion uncertainty
worker failure
```

For example:

```text
payment request -> Err(CardDeclined)
```

is a conclusive logical result.

But:

```text
payment request sent
connection lost
remote completion unknown
```

is an idempotency/recoverability problem, not automatically an `Err`.

Do not conflate logical result failure with execution failure.

---

# 13. Request results belong to `RequestInput`

A request boundary should declare the result contract returned by that request.

Recommended:

```rust
pub struct RequestInput {
    pub schema: Id,
    pub identity: RequestIdentity,
    pub result: ResultType,
}
```

This is preferable to one global `Operation.result`, because an operation may expose multiple request inputs and a `RequestEffect` already targets a specific:

```text
operation + input
```

Subscription inputs have no synchronous result contract.

---

# 14. Remove `Response` / `ResponseSource`

Remove:

```text
Operation.responses
Response
ResponseSource
```

A request invocation terminates directly with a typed result.

There is no longer a privileged:

```text
ResponseSource::InvocationResult
```

relationship.

Terminal result replay consistency is proven through ordinary provenance plus replay semantics of the values used to construct the result.

---

# 15. Request and non-request terminals

Recommended terminal primitives:

```rust
OperationStep::Return {
    request: Id,
    outcome: ResultOutcome,
}

OperationStep::Complete
```

with:

```rust
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResultOutcome {
    Ok {
        values: Derivation,
    },

    Err {
        values: Derivation,
    },
}
```

`Return` terminates a request-driven execution and constructs the declared request result.

`Complete` terminates a non-request execution without a returned value, as is natural for subscription-driven operations.

For:

```text
request R
R.result = Result<OkSchema, ErrSchema>
```

a:

```text
Return Ok(D)
```

constructs an `OkSchema` payload from `D`.

A:

```text
Return Err(D)
```

constructs an `ErrSchema` payload from `D`.

---

# 16. Result replay requirement

Transform:

```rust
IdempotencyRequirement {
    key: IdempotencyKey,
    response: ResponseReplayRequirement,
}
```

toward:

```rust
IdempotencyRequirement {
    key: IdempotencyKey,
    result: ResultReplayRequirement,
}
```

with:

```rust
pub enum ResultReplayRequirement {
    Unspecified,
    ReplayConsistent,
}
```

`ReplayConsistent` means:

> Repeated admitted attempts in the same logical idempotency domain that return a request result must return the same result variant and replay-equivalent payload.

The checker proves this from:

```text
control-path replay
terminal variant
terminal Derivation
TransactionOutput replay
effect-result replay
input replay stability
```

No special response artifact is needed.

---

# 17. Synchronous effect result contracts

Synchronous effects may produce first-class `Result<Ok, Err>` outcomes.

## Publication effects

`PublicationEffect` has no synchronous result and cannot bind one.

## Request effects

A `RequestEffect` inherits its result contract from its target `RequestInput`.

Given:

```rust
RequestTarget {
    operation: O,
    input: I,
}
```

if `I` declares:

```text
Result<OkSchema, ErrSchema>
```

then executing that request effect yields the same result type.

Do not redeclare the result schemas on `RequestEffect`.

## External effects

Because Archspec cannot inspect beyond an external boundary, allow:

```rust
pub struct ExternalEffect {
    pub name: String,
    pub idempotency: IdempotencyGuarantee,
    pub result: Option<ResultType>,
}
```

`None` means no synchronous result is modeled.

---

# 18. Effect result bindings

Extend execution steps:

```rust
ExecuteEffect {
    effect: Id,
    values: Derivation,
    result: Option<Id>,
}
```

and:

```rust
ExecuteEffectIntent {
    intent: Id,
    result: Option<Id>,
}
```

The result schema is inferred from the underlying effect contract.

A result-bearing effect may be executed without binding its result if the result is intentionally ignored.

A non-result-bearing effect must not declare a result binding.

`ExecuteEffectIntent` still has no outgoing-value derivation: the effect instance values were fixed when the intent was established.

---

# 19. Variant-specific effect-result values

Add:

```rust
ValueSource::EffectResultOk(Id)
ValueSource::EffectResultErr(Id)
```

where the ID names the result binding of an effect-execution step.

Schemas are inferred:

```text
EffectResultOk(r)
    -> r's Ok schema

EffectResultErr(r)
    -> r's Err schema
```

These are operation-local observations.

They are not transaction artifacts and are not inherently durable.

---

# 20. Explicit result matching

Success/failure control should not be encoded as a comparison against a conventional status field.

Introduce:

```rust
OperationStep::MatchResult {
    result: Id,
    ok: OperationBlock,
    err: OperationBlock,
}
```

Semantics:

```text
result = Ok(v)
    -> execute ok block

result = Err(e)
    -> execute err block
```

Inside `ok`:

```text
EffectResultOk(result)
```

is available and `EffectResultErr(result)` is not.

Inside `err`:

```text
EffectResultErr(result)
```

is available and `EffectResultOk(result)` is not.

The match is exhaustive and mutually exclusive by construction.

---

# 21. Generic branching remains separate

Keep a separate generic branch primitive:

```rust
Branch {
    condition: Condition,
    then: OperationBlock,
    otherwise: Option<OperationBlock>,
}
```

Its purpose is ordinary control decisions over modeled values.

Examples:

```text
region == "CA"
amount > threshold
flag == true
```

`MatchResult` destructures a `Result`.

`Branch` evaluates an ordinary predicate.

Do not overload either primitive.

The initial `Condition` language should remain deliberately small and structurally expose its `ValueRef` dependencies.

---

# 22. Replace alternative flows with one operation program

Remove:

```text
Operation.flows
InvocationFlow
FlowStep
```

Introduce one explicit operation control program:

```rust
pub struct Operation {
    pub service: Id,
    pub description: Option<String>,

    pub inputs: BTreeMap<Id, Input>,

    pub effects: BTreeMap<Id, Effect>,
    pub effect_intents: BTreeMap<Id, EffectIntent>,
    pub transaction_outputs: BTreeMap<Id, TransactionOutput>,

    pub transactions: BTreeMap<Id, Transaction>,

    pub program: OperationBlock,

    pub requirements: OperationRequirements,
    pub execution: ExecutionSemantics,
}
```

Initial structured representation:

```rust
pub struct OperationBlock {
    pub steps: Vec<OperationStep>,
}
```

The analyzer may lower this structure to a CFG.

---

# 23. Proposed `OperationStep`

Conceptually:

```rust
pub enum OperationStep {
    Transaction {
        transaction: Id,
    },

    ExecuteEffect {
        effect: Id,
        values: Derivation,
        result: Option<Id>,
    },

    ExecuteEffectIntent {
        intent: Id,
        result: Option<Id>,
    },

    MatchResult {
        result: Id,
        ok: OperationBlock,
        err: OperationBlock,
    },

    Branch {
        condition: Condition,
        then: OperationBlock,
        otherwise: Option<OperationBlock>,
    },

    Return {
        request: Id,
        outcome: ResultOutcome,
    },

    Complete,
}
```

Exact naming may change during implementation.

The semantic responsibilities should not.

---

# 24. Retain effect-instance provenance

Retain:

```rust
ExecuteEffect {
    effect: Id,
    values: Derivation,
    ...
}
```

`values` describes the provenance of the complete outgoing logical effect instance.

Natural replay safety still requires separate proof of:

```text
effect-instance replay stability
+
effect duplicate-execution safety
```

The returned `Result` is a separate semantic object from the outgoing effect payload.

---

# 25. Retain explicit effect-intent establishment

Keep:

```rust
EstablishEffectIntent {
    intent: Id,
    values: Derivation,
}
```

The transaction constructs and captures the logical effect instance.

The intent becomes a transaction artifact.

Later:

```text
ExecuteEffectIntent(I)
```

executes that exact captured instance.

It may bind a synchronous result if the underlying effect contract is result-bearing.

---

# 26. Retain transition-established intents

A successful state-machine `Transition` transaction step still implicitly establishes an effect intent for each declared transition side effect.

Retain:

```rust
StateTransition {
    ...
    effect_values: BTreeMap<Id, Derivation>,
}
```

with exact coverage of the referenced transition's side effects.

A successful transition atomically:

1. applies the state transition;
2. constructs the side-effect instances from `effect_values`;
3. establishes the corresponding intents;
4. commits the artifacts with transaction state.

Transition side effects are not executed inside the transaction.

---

# 27. Retain V1 transition replay rule

A transaction containing a state-machine transition continues to require explicit keyed transaction idempotency in V1. *(Superseded 2026-09-05: `CONSEQO_REVISION_V3_AMENDMENT_A_TRANSITION_DEDUP_RELAXATION.md` removes the structural requirement; the blocked natural-replay route and keyed artifact recovery are unchanged.)*

The first successful `Commit(T,K)` retains exact transition-established intents.

A retry restores those intents rather than re-evaluating the transition or its effect derivations.

---

# 28. Control-flow-scoped transaction artifact availability

The existing invocation-artifact-context semantics become a forward definite-availability analysis over operation control.

For transaction artifacts:

```text
AvailableArtifacts(entry) = {}
```

After transaction `T`:

```text
AvailableArtifacts(after T)
=
AvailableArtifacts(before T)
UNION
ArtifactsEstablishedOrRecoveredBy(T)
```

At a join:

```text
AvailableArtifacts(join)
=
INTERSECTION(
    AvailableArtifacts(each predecessor)
)
```

This applies to:

```text
TransactionOutput
EffectIntent
```

Therefore:

```text
ExecuteEffectIntent(I)
```

is valid only where `I` is definitely available.

Likewise:

```text
ValueSource::TransactionOutput(O)
```

is valid only where `O` is definitely available.

---

# 29. Operation-local effect-result scope

Effect result bindings use separate definite-assignment facts.

For:

```text
ExecuteEffect E -> r
```

the binding `r` is available after that step in the current operation attempt.

`MatchResult(r)` refines the path:

```text
ok arm:
    EffectResultOk(r) available

err arm:
    EffectResultErr(r) available
```

Variant payloads are arm-local.

Do not automatically allow them to leak past the match join.

If data must survive a branch and become generally available later, structure control accordingly or deliberately export durable data through a transaction artifact.

---

# 30. Branch replay semantics

The new model removes unexplained "flow selection."

A retry traverses declared control.

For a generic branch:

```text
condition dependencies replay-stable
+
predicate deterministic
    ->
same branch
```

For a result match:

```text
effect result replay-stable
    ->
same Ok/Err arm
```

If the controlling observation changes on retry, a different path may be legitimate.

The checker must then reason about compatibility of the resulting execution histories.

The DSL now exposes the cause of path divergence.

---

# 31. Effect-result replay is separate from effect-payload replay

Stable outgoing effect values do not by themselves prove stable returned results.

For a result-bearing effect, the checker must separately determine whether repeated equivalent executions produce replay-equivalent:

```text
Result variant
+
variant payload
```

For a request effect, a downstream target operation's proven result replay consistency may provide this evidence.

For an external effect, only explicitly modeled external guarantees may do so.

---

# 32. Request effect result lineage

A `RequestEffect` targets:

```text
operation O
request input I
```

Its logical result type is `I.result`.

If the target operation proves:

```text
ResultReplayRequirement::ReplayConsistent
```

under the relevant propagated logical request identity/idempotency domain, the caller may use that proof when reasoning about repeated observations of the request effect result.

Idempotency-key propagation remains lineage only.

It does not itself prove duplicate safety or result replay consistency.

---

# 33. Operation terminal replay

A request result is now constructed directly from values available at the terminal.

Example:

```text
Transaction T
    establishes TransactionOutput O
        |
        v
Return Ok
    values = Deterministic(from O)
```

If `O` is replay-stable and retry reaches the same terminal variant, the result payload may be proven replay-consistent.

No `InvocationResult` intermediary is required.

---

# 34. `TransactionOutput` is intentionally generic

A transaction may need to expose information that is not an operation result:

```text
reservation_id
selected_account
remaining_stock
routing decision
authorization metadata
normalized version
```

Later operation control may use it for:

```text
branching
effect construction
another transaction
terminal result construction
```

`TransactionOutput` directly models this boundary.

---

# 35. `TransactionOutput` is not inherently a `Result`

Do not force transaction outputs into `Result<Ok, Err>`.

These are orthogonal concepts.

Typical transaction outputs remain:

```text
TransactionOutput<Reservation>
TransactionOutput<PricingDecision>
TransactionOutput<NormalizedOrder>
```

The initial first-class `Result` semantics are required for:

```text
request operation results
synchronous effect results
```

If later architectures require generic result-typed transaction outputs, extend the type model deliberately rather than coupling that concern into `TransactionOutput` now.

---

# 36. Revised value taxonomy

Conceptually:

```rust
pub enum ValueSource {
    Input(Id),

    // Existing effect payload semantics.
    Effect(Id),

    // Generic transaction artifact exported to operation control.
    TransactionOutput(Id),

    StateMachineSubject(Id),

    // Same-transaction observation only.
    TransactionRead(Id),

    // Operation-local synchronous effect outcome payloads.
    EffectResultOk(Id),
    EffectResultErr(Id),
}
```

Remove:

```rust
InvocationResult(Id)
```

Update shorthand parsing, diagnostics, namespace documentation, and path validation accordingly.

---

# 37. Recoverability requirement transformation

Remove references to terminal execution of a declared flow.

The requirement becomes:

> The logical invocation identified by the recoverability key can reach a valid terminal in the operation program after any modeled interruption covered by the requirement.

Valid terminals are:

```text
Return
Complete
```

For `Resumable`:

> Every modeled failing execution prefix admits a valid retry/continuation history that reaches a terminal, with required transaction artifacts and replayed observations available as needed.

For `Guaranteed`:

> In addition to resumability, the architecture contains a modeled retry driver that guarantees re-driving until a terminal is reached.

Retain:

```text
idempotency = safety
recoverability = progress
```

---

# 38. Validation changes

The validator moves from per-flow validation to structured control-flow and dataflow validation.

## Request result contracts

Validate:

- `RequestInput.result.ok` schema exists;
- `RequestInput.result.err` schema exists;
- `Return.request` names an operation-owned request input;
- `Return::Ok` values validate against the `ok` schema;
- `Return::Err` values validate against the `err` schema;
- a subscription input cannot be a `Return` target.

## Effect results

Validate:

- publication effects cannot bind a result;
- request effect result contract resolves through the target request input;
- external effect may bind a result only if `ExternalEffect.result` is present;
- result IDs are unique in program scope;
- results cannot be referenced before binding.

## Result matching

Validate:

- `MatchResult.result` is definitely available;
- both `ok` and `err` arms exist;
- `EffectResultOk(r)` is legal only in the `ok` arm;
- `EffectResultErr(r)` is legal only in the `err` arm;
- field paths use the correct variant schema.

## Transaction outputs

Validate:

- output schema exists;
- establishment references an operation-owned output;
- establishment derivation is valid in transaction scope;
- transaction reads obey read-before-use;
- output use requires definite artifact availability.

## Effect intents

Retain existing establishment validation and add result-binding validation based on the underlying effect contract.

## Program structure

Validate:

- referenced transactions/effects/intents belong to the operation;
- all value references respect scope;
- all reachable paths reach an appropriate terminal;
- artifact/result availability is path-sensitive.

Do not perform replay proofs in structural validation.

## Object-history requirements

Remove validation and parsing support specific to:

```text
DataObject.requirements
ObjectRequirements.history
ObjectHistoryRequirement::Linearizable
```

Do not replace these checks with implicit linearizability inference.

---

# 39. Analyzer internal form

The structured program may lower to a CFG with nodes:

```text
Transaction
ExecuteEffect
ExecuteEffectIntent
MatchResult
Branch
Return
Complete
```

Edges represent:

```text
sequential successor
generic branch arms
Ok/Err match arms
joins
terminals
```

This supports:

```text
reachability
dominance
definite artifact availability
effect-result availability
variant refinement
crash-prefix exploration
branch replay
terminal result consistency
```

---

# 40. No loops in this iteration

Loops remain deferred because they require semantics for:

```text
iteration identity
repeated transaction artifacts
repeated effect-result bindings
effect multiplicity
termination
ordering between iterations
recovery from partial iteration progress
potentially unbounded analyzer state
```

The current expressivity problem is branching and intermediate observations, not iteration.

---

# 41. Surface transformation summary

| Current surface | Next iteration |
|---|---|
| `Operation.flows` | `Operation.program` |
| `InvocationFlow` | removed |
| `FlowStep` | `OperationStep` |
| `flow.response` | `Return` / `Complete` terminals |
| `Operation.responses` | removed |
| `Response` | removed |
| `ResponseSource` | removed |
| `InvocationResult` | `TransactionOutput` |
| `Operation.invocation_results` | `Operation.transaction_outputs` |
| `EstablishInvocationResult` | `EstablishTransactionOutput` |
| `ValueSource::InvocationResult` | `ValueSource::TransactionOutput` |
| conventional success/failure schema fields | `Result<Ok, Err>` |
| status-based synchronous result branching | `MatchResult` |
| no synchronous effect result source | result binding + `EffectResultOk/Err` |
| alternative complete flows | explicit causal operation control |
| `DataObject.requirements` | removed for now |
| `ObjectRequirements` | removed / deferred |
| `ObjectHistoryRequirement::Linearizable` | removed / deferred |

---

# 42. Example: request result and result matching

```yaml
inputs:
  input.checkout:
    kind: request
    schema: schema.checkout
    identity:
      kind: keyed
      fields: [request_id]
    result:
      ok: schema.checkout_success
      err: schema.checkout_error

program:
  steps:

    - kind: execute_effect
      effect: effect.authorize_payment
      values:
        kind: deterministic
        from:
          - source: input:input.checkout
            path: payment
      result: result.payment

    - kind: match_result
      result: result.payment

      ok:
        steps:
          - kind: transaction
            transaction: tx.mark_paid

          - kind: return
            request: input.checkout
            outcome:
              kind: ok
              values:
                kind: deterministic
                from:
                  - source: effect_result_ok:result.payment
                    path: authorization_id

      err:
        steps:
          - kind: transaction
            transaction: tx.mark_failed

          - kind: return
            request: input.checkout
            outcome:
              kind: err
              values:
                kind: deterministic
                from:
                  - source: effect_result_err:result.payment
                    path: reason
```

The causal structure is explicit:

```text
effect execution
    ->
Result<Ok, Err>
    ->
MatchResult
    ->
operation Return Ok/Err
```

---

# 43. Example: generic transaction output

```yaml
transaction_outputs:

  output.reservation:
    schema: schema.reservation

transactions:

  tx.reserve:
    ...
    steps:

      - kind: read
        result: read.inventory
        ...

      - kind: write
        ...

      - kind: establish_transaction_output
        output: output.reservation
        values:
          kind: deterministic
          from:
            - source: transaction_read:read.inventory
              path: reservation_id

program:
  steps:

    - kind: transaction
      transaction: tx.reserve

    - kind: execute_effect
      effect: effect.notify_warehouse
      values:
        kind: deterministic
        from:
          - source: transaction_output:output.reservation
            path: reservation_id

    - kind: complete
```

No fake effect intent and no response-specific transaction artifact are required.

---

# 44. Example: keyed transaction artifact recovery

```text
Transaction T
    idempotency = DeduplicatedBy(K)

    establishes:
        TransactionOutput O
        EffectIntent I
```

First successful attempt:

```text
execute T
    ->
commit state
    ->
retain O
    ->
retain I
    ->
Commit(T,K)
```

Retry:

```text
encounter T(K)
    ->
resolve Commit(T,K)
    ->
do not execute body
    ->
restore exact O
    ->
restore exact I
```

Downstream operation control sees the same artifact-availability postcondition.

---

# 45. Migration order

Recommended implementation order:

1. Add `ResultType`.
2. Add `RequestInput.result`.
3. Add optional `ExternalEffect.result`.
4. Replace `InvocationResult` with `TransactionOutput`.
5. Replace `EstablishInvocationResult` with `EstablishTransactionOutput`.
6. Replace `ValueSource::InvocationResult`.
7. Remove `Response` / `ResponseSource`.
8. Rename response replay requirement to result replay requirement.
9. Introduce `OperationBlock` / `OperationStep`.
10. Replace `Operation.flows` with `Operation.program`.
11. Add `Return` / `Complete`.
12. Add effect result bindings.
13. Add `EffectResultOk` / `EffectResultErr`.
14. Add `MatchResult`.
15. Add generic `Branch`.
16. Add transaction-artifact definite-availability validation.
17. Add effect-result definite-assignment and variant refinement.
18. Rewrite idempotency/recoverability analysis over the operation CFG.
19. Remove `DataObject.requirements`, `ObjectRequirements`, and `ObjectHistoryRequirement`.
20. Remove object-history validation, fixtures, diagnostics, and active normative linearizability semantics.
21. Remove stale flow/response/invocation-result terminology from documentation, diagnostics, fixtures, and tests.

Do not combine this migration with loops or operation durability primitives.

---

# 46. Open decisions before implementation freeze

The central semantics above are proposed for retention.

The following representation details still deserve review:

1. **Top-level execution field name**  
   `program`, `body`, or another term.

2. **Terminal names**  
   `Return` / `Complete` versus alternative precise names.

3. **Result-source naming**  
   `EffectResultOk` / `EffectResultErr` versus a more general variant-qualified result reference.

4. **Condition vocabulary**  
   Keep the initial generic branch language narrow.

5. **Input-specific path admission**  
   The current operation model may expose multiple inputs without explicitly associating control entry with a triggering input. Removing flows may reveal a need for an explicit entry/path-admission concept. Do not silently guess this relationship.

6. **Future result-typed transaction outputs**  
   Keep `TransactionOutput` schema-shaped initially. Generalize only if concrete architectures require algebraic types beyond request/effect outcomes.

---

# 47. Normative summary

> An operation has one explicit causal control structure rather than a collection of unexplained alternative complete flows.

> A request boundary declares a first-class `Result<Ok, Err>` contract.

> A synchronous result-bearing effect yields the same kind of mutually exclusive typed result.

> `Err` is a logical returned outcome, not an execution interruption.

> Request effects inherit their result contract from the targeted request input.

> External effects may explicitly declare a result contract because Archspec cannot inspect beyond that boundary.

> Publications do not produce synchronous results.

> `MatchResult` explicitly destructures result outcomes; generic `Branch` remains a separate ordinary-control primitive.

> `InvocationResult` and the privileged `ResponseSource::InvocationResult` relationship are removed.

> `TransactionOutput` is the generic typed artifact by which a transaction deliberately exports data to subsequent operation control.

> `EffectIntent` remains exclusively a captured logical effect instance intended for later execution.

> Transaction-local reads remain transaction-local.

> Explicit transaction `DeduplicatedBy { key }` remains a durable keyed logical-commit mechanism. Re-encountering a prior commit restores its exact transaction outputs and effect intents without recommitting the body.

> Transaction artifacts may be consumed only where their establishment or recovery is definitely guaranteed by control flow.

> Terminal request result replay consistency is proven from path/variant stability and ordinary value provenance rather than through a privileged invocation-result artifact.

> `RecoverabilityRequirement` remains an operation-level progress obligation terminating at explicit operation terminals.

> Archspec does not add generic workflow durability primitives in this revision.

> General loops remain deferred.

> Object-history requirements, including `linearizable`, are intentionally removed from the active next-iteration DSL while replication and availability semantics remain out of scope.

> Transaction serializability, locking, operation serialization, ordering, and object identity retain their existing meanings and must not be promoted into implicit object-history guarantees.

> Object-history consistency should be reconsidered only alongside a future explicit model of replication, replica observation, failure, and availability semantics.

The guiding design rule is:

> Control flow describes causality; `Result` describes mutually exclusive outcome; `TransactionOutput` exports data; `EffectIntent` captures work; keyed transaction idempotency recovers committed transaction artifacts. None should be made to stand in for another.

---

# 48. Reconciliation

Executed 2026-09-04. The proposals of §1–§47 are implemented on the
`operation-execution-revision` branch; this section records what the
implementation decided where the draft left room, what the V1 analysis
over the new surface actually is, and what it exposed. The main
document (`ARCHSPEC_DSL_SEMANTICS.md`) has not yet been reconciled; the
accepted drafts (`ARCHSPEC_EFFECT_SAFETY_DRAFT.md`,
`ARCHSPEC_FLOW_RESUMPTION_DRAFT.md`, `ARCHSPEC_ORDERING_DRAFT.md`,
`ARCHSPEC_REPLAY_STABILITY_DRAFT.md`,
`ARCHSPEC_SEMANTICS_REVISION_DRAFT.md`) carry a dated terminology note
mapping the retired vocabulary onto this one.

## 48.1 The §46 decisions

1. **Top-level execution field name**: `program`
   (`Operation.program: OperationBlock`).
2. **Terminal names**: `return` / `complete`
   (`OperationStep::Return { request, outcome }`,
   `OperationStep::Complete`).
3. **Result-source naming**: `effect_result_ok` / `effect_result_err`
   (`ValueSource::EffectResultOk(Id)` / `EffectResultErr(Id)`), with
   the `kind:id` YAML shorthand every other source has —
   `effect_result_ok:result.payment`.
4. **Condition vocabulary**: `unspecified | eq | and | not`. `eq`
   compares a `ValueRef` against a `SelectorValue` — a literal or
   another reference, reusing the selector-value shorthand — so every
   condition but `unspecified` is a deterministic function of the
   references it structurally exposes (`Condition::roots()`).
   `unspecified` states that the model provides no fact about how the
   decision is made.
5. **Input-specific path admission**: no explicit entry or
   path-admission concept was added. A path is admitted for
   triggering input `i` iff it ends at `complete` or at `return` for
   `i` — the direct generalization of the retired admitted-flow rule
   (a flow with no response, or with `i`'s response). A path returning
   another request input's result is not one an `i`-invocation
   completes. This is the V1 stance, not a resolution; see §48.7.
6. **Result-typed transaction outputs**: `TransactionOutput` stays
   schema-shaped (`TransactionOutput { schema }`).

Two further representation details, taken without a §46 item:
`StepLocation` names a program step by its one-based position in each
enclosing block with the arm taken — `3.ok.1` is the first step of the
`ok` arm of the third top-level step — and is how validation errors,
obstacles, proofs, and the visualization address steps; and result
bindings are ids owned by the operation, declared by the step that
binds them and globally unique within it.

## 48.2 The V1 analysis as implemented

The analyzer does not lower the program to a CFG (§39). It
**enumerates the paths** of the acyclic program
(`analyzer::verification::paths`): every invocation traverses one path,
taking one arm at each decision and ending at one terminal, so a path
is exactly the linear step sequence the retired flows were, plus the
decisions that selected it. The replay engine's single forward pass
(`analyzer::verification::replay`) applies unchanged path by path over
a `PathContext` of established artifacts (each with its replay route)
and bound results (each with its replay judgment). A path is named in
proofs and obstacles by the arms it took — `ok(result.x) › then(step
3)` — or as "the program" when the program has no decisions.

**Decision replay (§30).** A `match_result` replays iff its result is
replay-stable; a `branch` iff its condition is deterministic over
replay-stable roots (`unspecified` never). A bound result is stable
only when the effect is a **request** whose instance is class-fixed,
whose schema is the targeted input's, and whose target holds a proven
`result: replay_consistent` requirement keyed from that input
(`trigger::returns_consistently`).

**External results are never replay-stable** (`ResultGap::
ExternalResultUndeclared`). §31 says only explicitly modeled external
guarantees may establish result replay, and this draft names no such
guarantee, so no declared fact makes an external boundary's returned
result replay-consistent. `deduplicated_by` does not imply it: that
guarantee collapses the *work* of same-key executions and says nothing
about what each execution is *answered* with — a boundary may answer
the duplicate with a distinct "already processed" error, which is a
different variant. A decision on an external result is therefore never
established to replay in V1, and a value derived from one is never
replay-stable. Publications have no result (`ResultGap::
NoResultContract`).

**Result replay** (`analyzer::verification::result_replay`, replacing
the response-replay checker): per admitted path that ends at `return`
for the triggering input, every decision must replay
(`PathDecisionUnstable`) — so a class follows one path to one terminal,
which fixes the variant — and the terminal derivation must be
replay-deterministic in the end context of the path
(`ResultDerivationUnspecified`, `ResultDerivationRootUnstable`). No
returning path is the vacuous case (`NoReturnedResult`). Because a
request effect's result is stable only through another operation's
verdict, the checks are computed as a **greatest fixpoint** over the
replay-consistent requirements, exactly as idempotency's are; a cycle
of requests whose members each pass their local checks proves and is
marked `coinductive`. Verification runs result replay first, and its
proven `(operation, input)` set is what idempotency and recoverability
consult.

**Idempotency** (`analyzer::verification::idempotency`): per admitted
path, the state leg and effect leg of `ARCHSPEC_EFFECT_SAFETY_DRAFT.md`
as before, plus a **control leg**: every decision on the path must
replay, else `PathDecisionUnstable` — a retry that may take a different
arm may do different work, and V1 has no compatibility argument for
the two histories. No admitted path is vacuous (`NoAdmittedPaths`).

**Recoverability** (`analyzer::verification::recoverability`): per
admitted path, same-path continuation. A decision is **never** an
obstacle: progress holds path by path, whichever admitted path a retry
follows is analyzed on its own, and the difference in work is
idempotency's concern. A transaction that is the final step of a
`complete`-terminated path is exempt from re-encounter resolution
(`Resolution::TerminalStep`); a `return` follows its last transaction,
so every transaction on a returning path needs resolution. Consumed
artifacts are transaction outputs referenced by later transaction
bodies (same-transaction references exempt), by effect derivations,
and by the `return` outcome, plus every executed intent. An
unterminated path is `PathNotTerminated`; no admitted path is
`NoAdmittedPath`.

**Variant payloads are arm-local** (§29), in verification as in
validation: `effect_result_ok(r)` is in scope only within the `ok` arm
of the match on `r`, and neither payload survives the join after the
match, even when the other arm terminates.

**Obstacles are deduplicated by site.** Paths sharing a prefix share
its steps, so the same fact would otherwise be reported once per path;
an obstacle is reported once per site, with its path (and, for a
decision, the arm) forgotten for the comparison.

## 48.3 Validation, against §38

Implemented in `analyzer::validation` as described: request result
contracts (`RequestInput.result` schemas resolve; `Return.request`
names an operation-owned request input — `InvalidInputKind` for a
subscription; `Return` outcome derivations validated in operation
scope, their roots' field paths resolved against the sources they
name — a `Derivation` declares provenance, not a field mapping, so the
variant schema constrains what the payload is shaped by, not what the
derivation lists); effect results (publications and externals without a
declared contract cannot bind — `EffectHasNoResult`; request results
resolve through the target input; bindings are unique ids declared by
their binding step); transaction outputs (schema resolution,
operation-owned establishment, transaction-scope derivations, output
use requires availability); and the **forward definite-availability
pass** over each program (`ProgramValidator`): artifacts are available
after a transaction on every path, intersected at joins; result
bindings after the binding step; variant payloads arm-local. Commit
keys and transaction bodies are checked at each execution site
(same-transaction references satisfied by step order), and effect
declaration roots — an external `deduplicated_by` key, propagation
keys — at each execution site. Errors: `ProgramNotTerminated`,
`UnreachableProgramStep` (the first dead step per block),
`TransactionArtifactNotAvailable { location, artifact, consumer }`,
`EffectResultNotBound`, `EffectResultVariantOutOfScope`,
`EffectHasNoResult`. Validation performs no replay proof.

The object-history surface was removed as §3 and §38 require:
`DataObject { schema, identity }`, no `requirements`, no
`ObjectHistoryRequirement`, no linearizability verdicts, no validation.

## 48.4 Report format 2

`analyzer::report::FORMAT` is `2`: the response-replay property became
`result_replay` (obligation ids `oblig.<op>.result_replay.<i>`),
object-history obligations and the flow subject are gone, and proofs
cite decisions and paths. The visualization's TypeScript mirrors
(`viz/src/types/`) follow.

## 48.5 Fixture outcomes

`tests/fixtures/flash_checkout.yaml` — 14 obligations, **10 proven, 4
unknown**. `charge_payment` binds the card result
(`result.charge_payment.card`) and matches on it: `ok` publishes
`PaymentCaptured`; `err` publishes `PaymentFailed`, reading `reason`
from `effect_result_err:result.charge_payment.card`. Its idempotency
is unknown with three obstacles: the card charge is explicitly
`not_deduplicated`; the match is not established to replay (external
result, §48.2); and the `PaymentFailed` instance depends on the
unstable `reason` root. `create_order` returns `Ok` from
`transaction_output:output.create_order`, recovered from the keyed
commit — result replay and recoverability proven; its idempotency
remains unknown through the `reserve_inventory` cascade, and
`reserve_inventory` remains unknown on both legs, exactly as before.
`tests/fixtures/flash_checkout.report.json` is regenerated.

`tests/fixtures/video_streaming.yaml` — 15 obligations, **13 proven, 2
unknown**. `transcode_video` matches on the external engine's result
(`result.transcode_video.render`): `ok` completes the transcode and
executes the completion intent; `err` records the failure. Its
idempotency is unknown with one obstacle — the match on an external
result is not established to replay — and its recoverability is
proven, decisions being no obstacle to progress. `complete_upload`'s
idempotency is unknown through the cascade: it publishes
`VideoUploaded`, which `transcode_video` consumes under a requirement
that is not proven.

## 48.6 Follow-ups exposed

1. **An explicit external result-replay guarantee.** Nothing in the
   DSL can make an external result replay-stable, so every program
   that branches on one is unproven for idempotency and result replay,
   however the boundary actually behaves. A declaration on
   `ExternalEffect` stating that same-key (or same-instance) executions
   are answered with the same result variant and a replay-equivalent
   payload would discharge it — and must be separate from
   `deduplicated_by`, for the reason in §48.2. Recorded as open
   question 11 of `ARCHSPEC_SEMANTICS_REVISION_DRAFT.md` §27.
2. **Input-specific path admission (§46.5).** The terminal-based rule
   of §48.1 relates a path to an input only through its terminal. A
   path that completes with no `return` is admitted for every input,
   including request inputs whose invocations then return nothing; a
   program with several request inputs has no way to say which steps
   an invocation through each may reach. Recorded as open question 10
   of `ARCHSPEC_SEMANTICS_REVISION_DRAFT.md` §27.
3. **Loops** remain deferred (§40). Path enumeration presupposes an
   acyclic program; iteration needs the semantics §40 lists before the
   representation can admit it.
