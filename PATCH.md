# Patch Spec: Effect Instance Value Derivation

## Goal

Ensure that every point where Archspec creates a new logical effect instance declares the provenance of the values used to construct that instance.

Effect declarations remain contracts describing what kind of effect occurs. They do **not** own value derivation.

Value derivation belongs at the site where a particular effect instance is created.

There are three effect-instantiation paths:

1. direct `ExecuteEffect`;
2. explicit `EstablishEffectIntent`;
3. effects implicitly established by a `StateTransition`.

`ExecuteEffectIntent` consumes an already-established effect instance and therefore must **not** declare another derivation.

---

## 1. DSL Changes

### 1.1 `FlowStep::ExecuteEffect`

Change:

```rust
ExecuteEffect {
    effect: Id,
}
```

to:

```rust
ExecuteEffect {
    effect: Id,
    values: Derivation,
}
```

`values` describes the provenance of the complete logical effect instance constructed and executed by this flow step.

This applies to all effect kinds.

Do not add `Derivation` to the underlying `Effect` declaration.

---

### 1.2 `StateTransition`

Change:

```rust
pub struct StateTransition {
    pub machine: Id,
    pub transition: Id,
    pub subject: ObjectSelector,
}
```

to:

```rust
pub struct StateTransition {
    pub machine: Id,
    pub transition: Id,
    pub subject: ObjectSelector,
    pub effect_values: BTreeMap<Id, Derivation>,
}
```

Each key in `effect_values` identifies a side effect declared by the referenced state-machine transition.

For every side effect declared on the transition there must be exactly one corresponding entry in `effect_values`.

Therefore:

```text
transition.side_effects.keys()
==
state_transition.effect_values.keys()
```

A transition with no side effects must use an empty map.

The derivations are evaluated in the transaction context at the point where the `StateTransition` step occurs.

They may therefore reference transaction-local values that are in scope, including preceding `TransactionRead` results.

Existing read-before-use and field-selection rules continue to apply.

---

### 1.3 `EstablishEffectIntent`

Keep the existing shape unchanged:

```rust
pub struct EstablishEffectIntent {
    pub intent: Id,
    pub values: Derivation,
}
```

This is already the correct location for provenance when an operation explicitly establishes an effect intent.

---

### 1.4 `ExecuteEffectIntent`

Keep unchanged:

```rust
ExecuteEffectIntent {
    intent: Id,
}
```

Do not add `values`.

The effect instance was already constructed when the intent was established.

---

## 2. Effect Semantics

An `Effect` declaration defines the contract of logical work.

Depending on effect kind, this includes information such as:

- destination or target;
- schema;
- retry semantics;
- idempotency-key propagation;
- external idempotency guarantees.

It does not define how the values of a particular effect instance are computed.

An effect instance is constructed at an execution or establishment site.

---

## 3. Direct Effect Execution

For:

```text
ExecuteEffect(E, D)
```

the semantics are:

1. construct one logical instance of effect `E`;
2. obtain its values according to derivation `D`;
3. execute that effect instance.

For natural replay idempotency, the analyzer must prove that `D` is replay-deterministic.

Effect payload stability and duplicate-execution safety remain separate proof obligations.

---

## 4. Explicit Effect Intent Establishment

For:

```text
EstablishEffectIntent(I, D)
```

where intent `I` refers to effect `E`:

1. construct one logical instance of `E`;
2. obtain its values according to `D`;
3. establish `I` as a transaction artifact representing that exact effect instance.

A later:

```text
ExecuteEffectIntent(I)
```

executes that already-established instance.

`ExecuteEffectIntent` must never recompute or replace its values.

---

## 5. Transition Side Effects

State-machine transitions declare effect contracts independently of any particular transaction execution.

The transaction-level `StateTransition.effect_values` supplies the value provenance for the concrete instances created when that transition is applied.

For example:

```text
State machine transition:
    pending -> approved
    side effect E: publish ApprovalCreated

Transaction:
    Read Account -> account

    StateTransition:
        transition = approve
        effect_values:
            E = Deterministic(account.tier)
```

A successful transition transaction logically performs the following atomically:

1. evaluate the state-transition guard;
2. apply the state transition;
3. construct each transition side-effect instance using its corresponding `effect_values` derivation;
4. implicitly establish the corresponding effect-intent artifacts;
5. commit the transition state and established artifacts together.

Transition side effects are not executed inside the transaction merely because they are established there.

They are subsequently executed through the existing effect-intent execution semantics.

---

## 6. Transition Retry Semantics

Existing V1 transition semantics remain unchanged:

> A transaction containing a `Transition` is not naturally replayable and must use explicit durable keyed transaction idempotency where crash recovery is required.

Therefore transition-effect derivations are evaluated only during the first successful keyed transaction execution.

For:

```text
Transaction T
    idempotency = DeduplicatedBy(K)

    Read ...
    Transition ...
        implicitly establishes E
```

the first successful execution:

```text
evaluate transition
evaluate effect derivation
construct E
commit application state
retain E
retain Commit(T, K)
```

A retry with the same transaction idempotency identity:

```text
resolve Commit(T, K)
do not execute transaction body again
recover the exact original E
```

The transition effect derivation is not evaluated again.

This permits transition effects to depend on transaction-local reads even though those reads may not be replay-stable.

---

## 7. Natural Effect Replay

For an effect instance created through natural replay, the analyzer must prove its value derivation replay-stable.

### Direct effect

```text
ExecuteEffect(E, D)
```

requires:

```text
D is deterministic
+
all provenance roots of D are replay-stable
```

to prove that retry constructs the same logical effect instance.

### Explicit intent

For an intent established inside a naturally replayable transaction:

```text
EstablishEffectIntent(I, D)
```

natural reconstruction of `I` requires `D` to be replay-deterministic.

### Transition intent

Transition-containing transactions do not use natural reconstruction in V1.

Their exact effect intents are recovered from the durable keyed transaction commit.

---

## 8. Validation Changes

### 8.1 Direct `ExecuteEffect`

Preserve the existing validation that the referenced effect belongs to the operation.

Additionally validate `values`.

Because `ExecuteEffect` occurs at the flow level rather than inside a transaction, its derivation must use an operation-level value context.

It must not reference `TransactionRead`.

Validation must include both:

- reference validation;
- `ValueRef` field-path validation.

Do not attempt to prove replay stability here. That remains solver responsibility.

---

### 8.2 Transition `effect_values` Coverage

For every `TransactionStep::Transition`:

1. resolve the referenced state machine;
2. resolve the referenced transition;
3. preserve the existing check that the transition belongs to the machine;
4. obtain the IDs in `transition.side_effects`;
5. obtain the IDs in `state_transition.effect_values`;
6. require the two sets to be exactly equal.

Reject:

- missing derivations for transition side effects;
- extra derivations for effects not declared by the transition;
- derivations keyed by effects belonging to another transition.

Prefer a single structural diagnostic containing both missing and unexpected IDs, following existing diagnostic conventions.

Example conceptual diagnostic:

```rust
TransitionEffectValuesMismatch {
    transaction: Id,
    transition: Id,
    missing: Vec<Id>,
    unexpected: Vec<Id>,
}
```

The exact error type/name should follow the current validation style.

---

### 8.3 Transition Effect Derivation Scope

Each derivation in `StateTransition.effect_values` is evaluated in the context of the enclosing transaction.

Reuse the same transaction-aware `ValueContext` used for other transaction-step derivations.

This means a transition effect derivation may reference:

- valid operation-level values;
- available invocation results;
- preceding `TransactionRead` results.

Existing transaction-read validation must still enforce:

- the read belongs to the same transaction;
- the read occurs before the transition;
- the referenced field is included in that read's `FieldSelection`;
- the referenced `FieldPath` is valid.

A transition effect derivation must not be validated using a static state-machine-transition context because its values belong to a concrete transaction application of the transition.

---

### 8.4 Existing Transition Side-Effect Validation

Keep existing validation of the static transition side-effect declarations.

This includes existing checks for things such as:

- topic references;
- publication schemas;
- request targets;
- request inputs;
- idempotency-key propagation.

The state-machine transition continues to own the effect contract.

The `StateTransition` transaction step only provides the concrete value derivation.

---

### 8.5 `EstablishEffectIntent`

No shape or semantic change.

Preserve existing validation for:

- intent ownership;
- effect ownership;
- invalid explicit establishment of transition-owned intents, if currently prohibited;
- derivation references;
- derivation field paths;
- transaction-local read scope/order.

---

## 9. Expected Validator Touchpoints

The implementation will likely require changes in:

```text
src/spec/operation/flow.rs
src/spec/operation/transaction.rs

src/analyzer/validation/mod.rs
src/analyzer/validation/error.rs

ARCHSPEC_DSL_SEMANTICS.md
ARCHSPEC_SEMANTICS_REVISION_DRAFT.md

tests/
tests/fixtures/
```

Do not introduce additional abstractions unless required by the existing validation structure.

---

## 10. Flow Reference Validation

Where flow references are validated, update the `ExecuteEffect` branch from conceptually:

```rust
FlowStep::ExecuteEffect { effect } => {
    validate_effect_reference(effect);
}
```

to:

```rust
FlowStep::ExecuteEffect { effect, values } => {
    validate_effect_reference(effect);

    validate_derivation_references(
        operation_value_context,
        values,
    );
}
```

Use the existing validation helper patterns and diagnostic infrastructure rather than introducing a parallel derivation validator.

---

## 11. Flow Field-Path Validation

The existing field-path validation must additionally walk flow-step derivations.

For:

```rust
FlowStep::ExecuteEffect { values, .. }
```

validate all referenced `ValueRef` paths using the operation-level context.

A `TransactionRead` source must fail because no transaction context exists.

---

## 12. Transaction Reference Validation

Within validation of:

```rust
TransactionStep::Transition(state_transition)
```

after normal machine/transition/subject validation:

1. resolve the transition declaration;
2. compare transition-side-effect IDs against `effect_values` IDs;
3. report missing/unexpected mappings;
4. validate every derivation using the current transaction value context.

Do not infer or synthesize missing `Unspecified` derivations.

If provenance is unknown, the DSL must explicitly contain:

```yaml
kind: unspecified
```

This preserves the distinction between:

```text
provenance intentionally unspecified
```

and:

```text
provenance declaration accidentally missing
```

---

## 13. Transaction Field-Path Validation

For every derivation in:

```rust
StateTransition.effect_values
```

run the existing derivation field-path validation using the enclosing transaction context.

This must enforce transaction-read field selection and read-before-use exactly as for existing transaction derivations.

---

## 14. YAML Shape

### Direct effect execution

```yaml
- kind: execute_effect
  effect: effect.publish_order
  values:
      kind: deterministic
      from:
          - source:
                kind: input
                id: input.create_order
            path:
                - order_id
```

Unknown provenance must still be explicit:

```yaml
- kind: execute_effect
  effect: effect.publish_order
  values:
      kind: unspecified
```

---

### Transition effect derivation

```yaml
- kind: transition
  machine: machine.order
  transition: transition.capture
  subject:
      object: data.order
      predicate:
          kind: eq
          field:
              - order_id
          value:
              kind: value
              value:
                  source:
                      kind: input
                      id: input.capture
                  path:
                      - order_id

  effect_values:
      effect.payment_captured:
          kind: deterministic
          from:
              - source:
                    kind: transaction_read
                    id: read.payment
                path:
                    - amount
```

For a transition with no effects:

```yaml
effect_values: {}
```

---

## 15. Required Tests

Add or update tests covering at least:

1. `ExecuteEffect` parses and serializes with `values`.
2. `ExecuteEffect` accepts valid operation-scoped deterministic provenance.
3. `ExecuteEffect` accepts explicit `Unspecified`.
4. `ExecuteEffect` rejects `TransactionRead`.
5. `ExecuteEffect` rejects invalid `ValueRef` field paths.
6. A transition with side effects requires matching `effect_values`.
7. A missing transition-effect derivation is rejected.
8. An extra transition-effect derivation is rejected.
9. An effect belonging to another transition cannot appear in `effect_values`.
10. A transition effect derivation may reference a preceding read in the same transaction.
11. A transition effect derivation may not reference a later read.
12. A transition effect derivation may not reference a field excluded by the read's `FieldSelection`.
13. Invalid field paths inside transition effect derivations are rejected.
14. Transitions without effects accept an empty `effect_values` map.
15. Existing `EstablishEffectIntent.values` behavior remains valid.
16. `ExecuteEffectIntent` still has no `values` field.
17. Parser round-trip tests and canonical YAML fixtures are updated accordingly.

---

## 16. Do Not Change

Do not:

- add `Derivation` to `Effect`;
- add `Derivation` to `PublicationEffect`;
- add `Derivation` to `RequestEffect`;
- add `Derivation` to `ExternalEffect`;
- add `Derivation` to `EffectIntent`;
- add `values` to `ExecuteEffectIntent`;
- introduce a new `EffectInstance` DSL entity;
- introduce `RecoverEffectIntent`;
- introduce `RecoverInvocationResult`;
- move transition side effects out of state-machine definitions;
- weaken the existing V1 requirement that transition-containing transactions use durable keyed transaction idempotency.

---

## 17. Acceptance Criteria

After this patch, every location capable of creating a new logical effect instance has exactly one provenance declaration:

```text
ExecuteEffect
    -> FlowStep::ExecuteEffect.values

EstablishEffectIntent
    -> EstablishEffectIntent.values

StateTransition side effect
    -> StateTransition.effect_values[side_effect_id]
```

A location that merely executes an already-established instance has no provenance declaration:

```text
ExecuteEffectIntent
    -> consumes existing intent
```

The validator guarantees:

- every required effect-instance derivation exists;
- every derivation refers only to values available in its execution scope;
- transaction-read provenance obeys transaction ownership and ordering;
- referenced fields exist and are available;
- transition-effect mappings exactly match the transition's declared effects.

The validator does **not** determine whether those derivations are replay-stable.

Replay stability and operation idempotency remain model-checker responsibilities.
