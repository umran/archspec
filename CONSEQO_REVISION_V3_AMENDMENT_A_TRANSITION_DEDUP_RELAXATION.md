# Conseqo Revision V3 — Amendment A
## Relax Mandatory `DeduplicatedBy` for Transition-Containing Transactions

**Status:** Implemented 2026-09-05 (see §32)  
**Scope:** State-transition transaction replay semantics, structural validation, and requirement proving only  
**Dependency:** Apply after Revision V3 is implemented as written  
**Baseline behavior being amended:** V1/current semantics require every transaction containing a state-machine `Transition` to declare `IdempotencyGuarantee::DeduplicatedBy { key }`

---

# 1. Purpose

Revision V3 intentionally retained the V1 rule that every transaction containing a state-machine transition must declare explicit durable keyed transaction idempotency.

This amendment removes that blanket structural requirement.

The underlying V1 observation remains valid:

> A transaction containing a state-machine transition is not, in the current proof model, naturally replayable merely from the transition semantics.

What changes is the consequence drawn from that observation.

The current model effectively treats:

```text
transition prevents natural transaction replay proof
```

as equivalent to:

```text
transaction MUST use DeduplicatedBy
```

This amendment separates those concepts.

After this amendment:

```text
Transition
    -> blocks the current natural-transaction-replay proof route

Transition
    -/-> structural requirement for DeduplicatedBy
```

Explicit keyed transaction idempotency remains available as a strong durable recovery mechanism, but it is required only when a declared requirement actually needs the guarantees it provides.

---

# 2. Design principle

Conseqo must distinguish:

```text
STRUCTURAL VALIDITY
```

from:

```text
REQUIREMENT SATISFACTION
```

A transaction is not structurally invalid merely because it lacks enough guarantees to prove some hypothetical retry property.

`DeduplicatedBy { key }` is an implementation guarantee.

`IdempotencyRequirement` and `RecoverabilityRequirement` are obligations.

The solver determines whether the guarantee is needed for a particular obligation.

---

# 3. Existing rule being removed

Remove the current rule:

```text
Every transaction containing a Transition
MUST declare DeduplicatedBy { key }.
```

After this amendment, all of the following are structurally valid:

```text
Transaction T1
    idempotency = Unspecified
    Transition(...)
```

```text
Transaction T2
    idempotency = NotDeduplicated
    Transition(...)
```

```text
Transaction T3
    idempotency = DeduplicatedBy(K)
    Transition(...)
```

The declarations expose different replay/recovery facts to requirement solvers, but none is invalid solely because it contains a transition.

---

# 4. Retained semantic fact: transition transactions are not naturally replayable in V1

Retain the conservative proof rule:

> A transaction containing any state-machine `Transition` is not eligible for the current natural transaction replayability proof route.

Reason:

A committed transition changes the state against which its own transition precondition was evaluated.

For example:

```text
pending -> paid
```

after successful commit leaves:

```text
state = paid
```

A later execution cannot generally be assumed to reproduce the same transition execution, transaction outcome, or transaction artifacts merely by re-running the transaction.

A transition that rejects a second application may prove some form of at-most-once state mutation, but that does not establish natural replayability.

---

# 5. Revised transaction replay proof model

Conceptually:

```text
prove_transaction_retry_semantics(T)
    =
    any_of(
        prove_natural_transaction_replay(T),
        prove_explicit_keyed_transaction_recovery(T),
        other future sound proof routes
    )
```

For a transition-containing transaction:

```text
prove_natural_transaction_replay(T)
    ->
    Unknown(TransitionNaturalReplayUnsupported)
```

or equivalent.

This must NOT itself produce a validation error and must NOT imply that the enclosing operation requirement necessarily fails.

The operation-level solver may still establish correctness through explicit keyed commit recovery, different retry control paths, already-persisted application state, or future modeled guarantees.

If no sufficient proof route exists, the relevant requirement returns `Unknown` or `Violated` according to ordinary solver rules.

---

# 6. Explicit keyed transaction recovery remains unchanged

Retain:

```text
Transaction T
    idempotency = DeduplicatedBy(K)
```

First successful execution:

```text
execute body
+
apply transition
+
establish transaction artifacts
+
atomically retain Commit(T,K)
```

Later same-key encounter:

```text
do not reapply the transition
+
resolve Commit(T,K)
+
restore exact retained transaction artifacts
```

The amendment does not weaken or alter `DeduplicatedBy`.

It only stops requiring that mechanism unconditionally.

---

# 7. Transition-established `EffectIntent` semantics remain unchanged

A state-machine transition may declare side effects.

When such a transition successfully commits:

```text
state transition
+
transition side-effect instances
+
corresponding EffectIntent artifacts
```

are established atomically according to existing transaction-artifact semantics.

This amendment does not change transition side-effect declaration, `StateTransition.effect_values`, implicit `EffectIntent` establishment, intent uniqueness/establishability, or `ExecuteEffectIntent`.

Only the retry/recovery proof changes.

---

# 8. Transition `effect_values` semantics remain unchanged

Retain exact coverage:

```text
transition.side_effects.keys()
==
state_transition.effect_values.keys()
```

The derivations remain evaluated in the enclosing transaction context and may use valid preceding `TransactionRead` values.

If the transaction is `DeduplicatedBy(K)`, later same-key encounters recover the exact original intents without re-evaluating `effect_values`.

If the transaction is not explicitly deduplicated, no such recovery fact may be inferred.

This affects requirement proving only.

---

# 9. `TransactionOutput` semantics under Revision V3

Revision V3 replaces `InvocationResult` with `TransactionOutput`.

The same amended rule applies to transition-containing transactions that establish outputs.

Example:

```text
Transaction T
    Transition(...)
    EstablishTransactionOutput O
```

On first successful execution:

```text
transition commits
+
O is established
```

If:

```text
T.idempotency = DeduplicatedBy(K)
```

then `Commit(T,K)` retains the exact `O`, and retry may recover it.

Without keyed deduplication, the model remains structurally valid, but the solver may not assume that retry can naturally reproduce or recover `O` merely by replaying `T`.

---

# 10. Structural validation changes

Remove the validator rule:

```text
Transition
    AND transaction.idempotency != DeduplicatedBy
    ->
    validation error
```

Specifically remove the current transition-specific check equivalent to:

```rust
if !deduplicated {
    errors.push(
        ValidationError::TransitionTransactionNotDeduplicated {
            ...
        }
    );
}
```

Do not replace it with a warning.

The absence of a proof-oriented guarantee is not a structural-validation warning condition.

---

# 11. Remove obsolete validation diagnostic

Remove:

```rust
ValidationError::TransitionTransactionNotDeduplicated
```

and the corresponding validation code if it exists exclusively for this rule.

Also remove its diagnostic rendering, evidence generation, tests, and documentation.

No successor structural diagnostic is introduced.

---

# 12. Structural transition rules that remain

Continue validating:

```text
state machine exists
transition exists
transition belongs to machine
subject object matches machine subject
subject selector is valid
transaction object/data-model ownership is valid
effect_values exactly cover declared transition side effects
effect-value derivations are valid in transaction scope
transition-owned side-effect intents are unambiguous
transition-owned intents are establishable
transition side effects are not explicitly re-established
```

This amendment concerns only mandatory transaction idempotency.

---

# 13. Requirement solver changes

The solver must no longer rely on structural validation to guarantee that every transition-containing transaction is `DeduplicatedBy`.

Whenever replay/recovery analysis encounters one, inspect its actual idempotency declaration.

Conceptually:

```text
if transaction.contains_transition():

    natural =
        Unknown(TransitionNaturalReplayUnsupported)

    explicit =
        match transaction.idempotency:
            DeduplicatedBy(key):
                prove_keyed_commit_recovery(key)

            NotDeduplicated:
                Unknown(NoExplicitTransactionRecovery)

            Unspecified:
                Unknown(NoUsableExplicitTransactionRecoveryFact)
```

The enclosing requirement solver composes these outcomes with all other available proof routes.

Do not automatically turn the absence of keyed deduplication into `Violated`.

`Unknown` is normally appropriate unless the declarations provide an actual counterexample.

---

# 14. Idempotency requirement behavior

Example:

```text
Operation O

Transaction T
    Transition pending -> paid

Requirement:
    Idempotency(K)
```

The transaction is valid.

The transition blocks the ordinary natural-transaction-replay proof.

If there is no other sufficient proof route:

```text
Outcome = Unknown
```

Diagnostics should explain that the analyzer cannot establish replay/recovery from the declared facts.

They should not say that the transaction itself is invalid.

---

# 15. Recoverability requirement behavior

Example:

```text
Transaction T
    Transition pending -> paid
        establishes EffectIntent I

later:
    ExecuteEffectIntent I

Requirement:
    Recoverability(K)
```

Suppose:

```text
T commits
I is established
CRASH
before ExecuteEffectIntent I
```

With:

```text
T.idempotency = DeduplicatedBy(Kt)
```

retry may resolve `Commit(T,Kt)`, restore `I`, and continue.

Without explicit keyed deduplication, `T` remains structurally valid, but the current analyzer cannot assume replay will reconstruct `I`.

If no alternate control path or other modeled guarantee restores sufficient state:

```text
RecoverabilityRequirement -> Unknown
```

---

# 16. Transition without downstream replay obligations

Permit:

```text
Transaction T
    Transition pending -> cancelled
```

with no explicit keyed idempotency when no declared requirement requires transition replay/recovery.

There is no semantic reason for structural validation to demand an idempotency key merely because the transaction contains a transition.

---

# 17. Alternate retry paths

Revision V3 introduces explicit causal operation control.

A retry may legitimately observe durable state written by the first attempt and follow a different control path.

Example:

```text
first attempt:
    state == pending
    -> Transition pending -> paid
    -> CRASH

retry:
    state == paid
    -> already-completed/recovery path
```

Such an operation may eventually be provable without naturally replaying the transition transaction and without keyed transaction recovery.

Therefore:

> `DeduplicatedBy` is a sufficient durable-recovery mechanism for transition transactions, not the only admissible architecture.

---

# 18. Distinguish state-mutation at-most-once from transaction replay

Retain:

```text
transition cannot successfully apply twice
```

does not imply:

```text
transaction is naturally replayable
```

and does not imply:

```text
transaction artifacts are recoverable
```

A non-reentrant transition may suppress a second state mutation without reproducing `TransactionOutput`, `EffectIntent`, or other artifacts from the original execution.

Do not promote transition non-reentrancy into general transaction replayability.

---

# 19. No automatic requirement insertion

Do not synthesize `IdempotencyRequirement`, `RecoverabilityRequirement`, or `DeduplicatedBy` merely because a transaction contains a transition.

Requirements remain explicitly declared obligations.

Guarantees remain explicitly declared facts.

The amendment changes only how those declared facts are interpreted.

---

# 20. Preserve `Unspecified` vs `NotDeduplicated`

Do not collapse:

```text
IdempotencyGuarantee::Unspecified
```

and:

```text
IdempotencyGuarantee::NotDeduplicated
```

`Unspecified` gives no usable explicit deduplication fact.

`NotDeduplicated` explicitly denies durable keyed commit deduplication.

Neither is a structural error for a transition-containing transaction.

---

# 21. Normative documentation changes

Replace the existing MUST rule with:

> A transaction containing a `Transition` is not naturally replayable under the current replay proof rules. This does not make such a transaction structurally invalid when it lacks explicit keyed transaction idempotency. `DeduplicatedBy { key }` provides a durable recovery route by resolving the prior logical transaction commit and restoring its exact transaction artifacts. Where a declared idempotency or recoverability requirement depends on replay/recovery of a transition-containing transaction, the analyzer must prove an adequate route from the actual declarations; otherwise the relevant requirement remains unproven.

Also update nearby wording that assumes:

```text
because every transition transaction is DeduplicatedBy...
```

Transition `effect_values` replay wording must become conditional:

```text
If the transaction is DeduplicatedBy,
the original artifacts are recovered on same-key replay.
```

---

# 22. Revision V3 reconciliation

Revision V3 currently retains the old blanket transition rule.

After V3 implementation, amend only that relationship.

Retain:

```text
TransactionOutput
EffectIntent
Result<Ok, Err>
operation control program
artifact definite availability
transition effect_values
keyed transaction artifact recovery
```

Remove only:

```text
Transition -> mandatory DeduplicatedBy
```

---

# 23. Analyzer diagnostic guidance

Requirement diagnostics should be obligation-oriented.

Preferred:

```text
Cannot prove recoverability of operation `checkout`.

Transaction `tx.capture` contains a state transition.
The current analyzer does not prove natural replayability for
transition-containing transactions.

No explicit keyed transaction recovery or alternate proven recovery
path establishes availability of effect intent `intent.receipt`
after a post-commit interruption.
```

Avoid:

```text
Transaction `tx.capture` must declare deduplicated_by.
```

The latter prescribes one implementation strategy rather than reporting the missing proof fact.

---

# 24. Tests to remove or rewrite

Remove tests whose only expected behavior is:

```text
transition-containing transaction without DeduplicatedBy
-> validation error
```

Rewrite them to assert structural validation succeeds when all other transition declarations are valid.

Remove tests for the obsolete `TransitionTransactionNotDeduplicated` diagnostic.

---

# 25. New structural validation tests

Add at least:

1. transition transaction with `Unspecified` idempotency is structurally valid;
2. transition transaction with `NotDeduplicated` is structurally valid;
3. transition transaction with `DeduplicatedBy` remains structurally valid;
4. transition side-effect `effect_values` coverage remains required regardless of idempotency;
5. invalid subject/machine/object relationships still fail validation;
6. explicit establishment of a transition-owned effect intent still fails;
7. transition-owned intent uniqueness/establishability remains enforced.

---

# 26. New solver tests

Add focused tests for:

## Natural replay

```text
transition-containing transaction
+
no explicit keyed recovery
```

must not be reported as naturally replayable.

## Explicit recovery

```text
transition-containing transaction
+
DeduplicatedBy(K)
```

may use existing keyed commit recovery.

## Recoverability with retained intent

If a transition establishes `I` and recovery after post-commit interruption requires `I`:

```text
without sufficient recovery -> Unknown
```

unless another modeled path proves recovery.

With valid keyed recovery, the artifact-recovery sub-obligation may be proven.

## Transaction output

Under V3:

```text
Transition
+
EstablishTransactionOutput(O)
```

without keyed recovery is structurally valid.

If retry requires `O` and no alternative proof exists:

```text
replay/recoverability -> Unknown
```

## No requirement

A valid transition transaction with no relevant retry requirement remains accepted without a key.

---

# 27. Expected implementation touchpoints

After V3 lands:

```text
src/analyzer/validation/mod.rs
src/analyzer/validation/error.rs
validation diagnostic-code definitions
validation tests

idempotency/replay solver
recoverability solver
solver evidence/unknown-reason types if needed

ARCHSPEC_DSL_SEMANTICS.md or renamed Conseqo equivalent
fixtures and tests containing the old mandatory rule
```

---

# 28. Primitives not redesigned by this amendment

No redesign is required for:

```text
StateMachine
Transition
StateTransition
StateTransition.effect_values

Transaction
TransactionStep
TransactionOutput

EffectIntent
EstablishEffectIntent
ExecuteEffectIntent

IdempotencyGuarantee::DeduplicatedBy
IdempotencyGuarantee::NotDeduplicated
IdempotencyGuarantee::Unspecified

IdempotencyRequirement
RecoverabilityRequirement

Result<Ok, Err>
OperationBlock / OperationStep
```

Only the semantic coupling between:

```text
TransactionStep::Transition
```

and:

```text
IdempotencyGuarantee::DeduplicatedBy
```

is changed.

---

# 29. Acceptance criteria

The amendment is complete when:

## Structural model

This is accepted:

```text
Transaction T
    idempotency = Unspecified
    Transition(...)
```

provided the transition is otherwise valid.

## Natural replay

The analyzer does not infer that `T` is naturally replayable.

## Explicit recovery

If `T.idempotency = DeduplicatedBy(K)`, existing durable commit/artifact-recovery semantics remain available.

## Requirement proving

When retry correctness depends on replay/recovery of `T`, the solver derives the result from actual declared facts.

Absence of keyed recovery may produce:

```text
Unknown
```

rather than structural failure.

## Diagnostics

No validation diagnostic says transition-containing transactions must declare an idempotency key.

Proof diagnostics may identify explicit keyed recovery as one missing sufficient fact without presenting it as the only legal architecture.

---

# 30. Normative amendment text

> **Transition transaction replay.** A transaction containing a state-machine `Transition` is not naturally replayable under the current replay proof rules. A successful transition changes the state against which the transition was evaluated, so re-executing the transaction cannot generally be assumed to reproduce the same transaction outcome or transaction artifacts.
>
> This limitation is a proof fact, not a structural validity rule. A transition-containing transaction MAY declare any `IdempotencyGuarantee` permitted for other transactions, including `Unspecified`, `NotDeduplicated`, or `DeduplicatedBy { key }`.
>
> `DeduplicatedBy { key }` provides a durable recovery route: after the first successful logical commit, a later encounter with the same transaction identity resolves the prior `Commit(T,K)` rather than reapplying the transition and restores the exact transaction artifacts retained by that commit.
>
> Where an `IdempotencyRequirement`, `RecoverabilityRequirement`, or another analysis depends on replay or recovery of a transition-containing transaction, the analyzer MUST prove a sufficient route from the actual declared semantics. It MUST NOT assume natural transaction replayability, and it MUST NOT assume durable keyed recovery unless `DeduplicatedBy { key }` is declared and its key relationship is proven.
>
> Failure to establish such a route affects the relevant requirement outcome; it does not make the transaction declaration structurally invalid.

---

# 31. Summary

Before:

```text
Transition
    ->
MUST DeduplicatedBy(K)
    ->
otherwise validation error
```

After:

```text
Transition
    ->
natural transaction replay proof unavailable
```

and independently:

```text
DeduplicatedBy(K)
    ->
durable keyed transaction recovery available
```

Then:

```text
declared requirement
        +
actual transaction guarantees
        +
operation control/recovery paths
        ->
Proven | Violated | Unknown
```

The guiding rule is:

> **Do not encode a preferred proof strategy as a structural validity constraint.**

---

# 32. Reconciliation

Executed 2026-09-05, on top of the implemented Revision V3.

1. **Structural rule removed** (§10, §11): `validate_transactions` no
   longer requires `deduplicated_by` on transition-containing
   transactions; `ValidationError::TransitionTransactionNotDeduplicated`
   and `ValidationCode::TransitionTransactionNotDeduplicated` are
   deleted with their rendering. No warning replaces them. Every §12
   structural transition rule is retained unchanged.
2. **Solver** (§5, §13): no changes were needed — the replay engine
   already inspects the actual declaration at every site. The natural
   route records `ReplayGap::ContainsTransition` (the retained §4
   fact), the recovery route records `ReplayGap::NoKeyedCommit` for
   `unspecified` and `not_deduplicated` alike (their §20 distinction is
   preserved in the declarations, and neither yields a recovery fact),
   and requirement verdicts settle `Unknown` with those gaps as
   evidence, per §14–§15. The §23 diagnostic guidance already holds:
   the recorded gaps state missing proof facts, not a prescribed
   mechanism.
3. **Tests** (§24–§26): the rejection test became
   `transition_transactions_accept_any_idempotency_guarantee` plus
   `transition_effect_values_coverage_is_independent_of_the_guarantee`
   (tests/validation.rs); solver coverage added as
   `a_transition_without_keyed_recovery_is_unknown_not_invalid`
   (idempotency and recoverability over a `not_deduplicated` transition
   transaction: `NoKeyedCommit` + `ContainsTransition`, and the
   transition intent replay-unavailable) and
   `a_transition_established_output_without_recovery_defeats_result_replay`
   (a transition transaction establishing a `TransactionOutput` without
   a key is structurally valid and the result-replay obligation over it
   is unknown) in tests/verification.rs. Keyed recovery remains covered
   by the standing fixture tests.
4. **Fixture** (§16): `tx.cancel_order` in
   tests/fixtures/flash_checkout.yaml now declares
   `idempotency: unspecified` — a transition transaction with no retry
   obligation over it, valid without a key. `operation.cancel_order`
   declares no requirements, so every verdict and the checked-in report
   are unchanged (10 proven / 4 unknown). `tx.apply_payment` remains
   the keyed example.
5. **Documentation** (§21, §22, §30): the main document's §22 "V1
   transition replay rule" became "Transition transaction replay",
   carrying the §30 normative text; its transition-effect-values
   recovery paragraph is now conditional on the declared guarantee; the
   superseded MUST statements in
   ARCHSPEC_SEMANTICS_REVISION_DRAFT.md (§13.4, §22.1),
   ARCHSPEC_OPERATION_EXECUTION_REVISION_DRAFT_V3.md (§27), and the
   route-B remarks in ARCHSPEC_EFFECT_SAFETY_DRAFT.md §5 and
   ARCHSPEC_REPLAY_STABILITY_DRAFT.md are annotated in place.
