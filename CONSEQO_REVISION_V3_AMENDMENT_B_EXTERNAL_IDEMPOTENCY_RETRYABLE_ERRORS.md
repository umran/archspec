# Conseqo Revision V3 — Amendment B
## Strong External Idempotency and Retryable Error Outcomes

**Status:** Implemented 2026-09-05 (see §40, Reconciliation)  
**Apply after:** Revision V3 and Amendment A  
**Scope:** `ExternalEffect` idempotency, synchronous external result replay, `Result<Ok, Err>` error disposition, replay analysis, and validation  
**Explicit non-goal:** This amendment does **not** introduce an execution retry policy, retry loop, attempt count, backoff, timeout policy, or general loop primitive.

---

# 1. Purpose

Revision V3 introduced first-class synchronous `Result<Ok, Err>` outcomes for request boundaries and result-bearing effects.

The current V1 external-effect semantics contain a gap: `ExternalEffect.idempotency = DeduplicatedBy(K)` currently guarantees only that same-key executions do not repeat the logical external work. It explicitly does **not** guarantee that a duplicate execution returns the same logical result.

As a consequence, the current analyzer treats every external effect result as non-replay-stable, even when the external effect is declared idempotent.

This amendment changes that interpretation.

For Conseqo, a result-bearing external interaction declared `DeduplicatedBy(K)` must be treated as one logical interaction identified by `K`, including its **terminal logical result**.

At the same time, the model must distinguish a **terminal logical error** from a **retryable attempt error** so that transient/retryable errors do not become permanently memoized as the logical terminal result of an idempotent external interaction.

---

# 2. Core semantic distinction

After this amendment, an external interaction has two semantic layers:

```text
logical external execution
    identified by an idempotency key

individual attempts
    concrete executions made while that logical execution
    has not yet terminally resolved
```

A result-bearing attempt may produce `Ok(value)` or `Err(error)`, but an `Err` may be classified as either `terminal` or `retryable`.

A retryable error is conclusive for that attempt but does not terminally resolve the logical external execution.

---

# 3. Strengthen `ExternalEffect::DeduplicatedBy`

For a result-bearing external effect:

```text
ExternalEffect E
    idempotency = DeduplicatedBy(K)
```

the guarantee now means:

1. equal evaluated keys identify the same logical external-effect execution;
2. the logical external work is not terminally committed more than once for that identity;
3. retryable error outcomes do not establish the terminal logical result;
4. once the logical execution reaches a terminal result, that exact logical result is stable for all subsequent same-key executions;
5. the stable terminal result includes both the `Result` variant and a replay-equivalent payload.

Therefore, after terminal resolution:

```text
same K
    -> same logical external execution
    -> same terminal Result variant
    -> same replay-equivalent terminal payload
```

This is the intended meaning of external idempotency in Conseqo.

---

# 4. No separate external result-replay guarantee

Do **not** introduce a separate `ResultReplayGuarantee` for external effects.

For Conseqo, result consistency is part of the semantic meaning of `ExternalEffect.idempotency = DeduplicatedBy(K)` when the effect has a synchronous result.

An external API that performs work only once but returns:

```text
first call:
    Ok(original_value)

duplicate:
    Err(AlreadyProcessed)
```

does **not** conform to Conseqo's strengthened `DeduplicatedBy` contract unless the modeled boundary abstracts the duplicate response back into the original logical result.

If the distinct duplicate response is exposed as the modeled result, the boundary must not be declared `DeduplicatedBy` under this contract.

---

# 5. Non-result-bearing external effects

For `ExternalEffect.result = None`, the existing meaning remains:

```text
DeduplicatedBy(K)
    -> same-key logical work is deduplicated
```

There is no result-replay component because no synchronous result is modeled.

---

# 6. Retryable errors are not terminal results

Revision V3 correctly distinguishes a logical `Err` from execution uncertainty. Retain that distinction.

A retryable `Err` means:

> This attempt completed conclusively and returned the declared error payload, but the error contract admits another attempt of the same logical interaction.

It is not equivalent to crash, timeout with unknown completion, lost response, or remote completion uncertainty. Those remain execution/recoverability phenomena.

Likewise, a retryable error is not a terminal failure of the logical external execution.

---

# 7. Add error disposition to `ResultType`

Current:

```rust
pub struct ResultType {
    pub ok: Id,
    pub err: Id,
}
```

Revise to:

```rust
pub struct ResultType {
    pub ok: Id,
    pub err: ErrorResultType,
}
```

with:

```rust
pub struct ErrorResultType {
    pub schema: Id,
    pub disposition: ErrorDisposition,
}
```

and:

```rust
#[serde(rename_all = "snake_case")]
pub enum ErrorDisposition {
    Unspecified,
    Terminal,
    Retryable,
}
```

The error schema and its disposition belong to the **result contract**, not to the schema globally.

Therefore the same error schema may participate in different result contracts with different retry semantics.

---

# 8. Error-disposition semantics

## `Unspecified`

`disposition = unspecified` means:

> The model provides no usable fact about whether observing this `Err` terminally resolves the logical interaction or admits another attempt.

No retry or terminality fact may be inferred.

## `Terminal`

`disposition = terminal` means:

> Observing this `Err` terminally resolves the logical interaction with the declared error payload.

For an idempotent result-bearing external effect, the terminal `Err` becomes the stable same-key logical result.

## `Retryable`

`disposition = retryable` means:

> Observing this `Err` conclusively ends the current attempt but does not terminally resolve the logical interaction. Another attempt of the same logical interaction is semantically admitted.

It does **not** mean that a retry will occur, that a retry is guaranteed, that a retry will succeed, that the next attempt will return a different result, or that the caller retries immediately.

Those require separate execution semantics not introduced here.

---

# 9. Example: idempotent external interaction with retryable errors

```text
ExternalEffect Charge
    idempotency = DeduplicatedBy(payment_id)

    result =
        Result<
            ChargeAccepted,
            GatewayError retryable
        >
```

Admitted history:

```text
attempt 1, key K:
    Err(GatewayBusy)

attempt 2, key K:
    Err(RateLimited)

attempt 3, key K:
    Ok({ authorization_id: A123 })

attempt 4, key K:
    Ok({ authorization_id: A123 })
```

The retryable errors are attempt-level outcomes. The first terminal `Ok` establishes the logical result. Every later same-key execution must expose the same terminal `Ok` with replay-equivalent payload.

Not admitted under `DeduplicatedBy(K)`:

```text
attempt 3:
    Ok({ authorization_id: A123 })

attempt 4:
    Err(AlreadyProcessed)
```

because the logical execution has already terminally resolved to the `Ok(A123)` result.

---

# 10. Example: terminal error

For:

```text
Result<
    ChargeAccepted,
    CardDeclined terminal
>
```

this is admitted:

```text
attempt 1, key K:
    Err({ reason: declined })

attempt 2, key K:
    Err({ reason: declined })
```

The first `Err` terminally resolves the logical interaction.

This is not admitted under the same `DeduplicatedBy(K)` contract:

```text
attempt 1:
    Err(CardDeclined)

attempt 2:
    Ok(ChargeAccepted)
```

---

# 11. Replay stability of external results

Replace the current V1 rule:

```text
external effect results are never replay-stable
```

with the following.

A **terminal result** bound from an external effect is replay-stable relative to governing operation key `Kop` when:

1. the external effect declares `IdempotencyGuarantee::DeduplicatedBy { key: Ke }`;
2. `Ke` is class-fixed / replay-stable relative to `Kop` under the existing key-provenance rules;
3. the observed result is known to be terminal:
   - `Ok`, which is terminal; or
   - `Err` whose result contract declares `disposition: terminal`.

Then:

```text
same operation replay class
    -> same external idempotency key
    -> same logical external execution
    -> same terminal result
```

and the terminal variant payload is replay-stable.

---

# 12. Replayability of retryable `Err`

A retryable error is **not** the stable terminal result of the logical execution.

Therefore `EffectResultErr(r)` from a contract with `disposition = retryable` must not be promoted into stable terminal-result evidence merely because the external effect is `DeduplicatedBy`.

The attempt may return one retryable error and a later attempt may return another retryable error or a terminal result.

Example:

```text
Err(GatewayBusy)
Err(RateLimited)
Ok(Accepted)
```

is compatible with the same logical external execution.

Accordingly, an operation decision that treats a retryable `Err` as an ordinary replay-stable terminal branch cannot be justified from external idempotency alone.

---

# 13. Replayability of `Err` with unspecified disposition

For `disposition = unspecified`, the analyzer has no usable fact that the error is terminal or retryable.

Therefore external idempotency alone does not establish replay stability of an observed `Err`.

The result remains `Unknown` for proofs that require terminality/replay stability of that error outcome.

This preserves Conseqo's epistemic meaning of `unspecified`.

---

# 14. `Ok` terminality

For the current two-variant `Result<Ok, Err>` model, `Ok` is terminal by definition.

No additional `Ok` disposition is introduced.

An idempotent result-bearing external effect that returns `Ok(v)` has terminally resolved that logical external execution.

Every subsequent same-key execution returns the same replay-equivalent `Ok(v)`.

---

# 15. `MatchResult` implications

For:

```text
ExecuteEffect external -> r

MatchResult r:
    ok  -> A
    err -> B
```

replay analysis becomes path-sensitive.

## `Ok` path

If the external effect is `DeduplicatedBy(Ke)` and `Ke` is class-fixed:

```text
Ok result
    -> terminal
    -> replay-stable
    -> same MatchResult arm on same-key replay
```

## terminal `Err` path

If `err.disposition = terminal` and the same external idempotency-key proof holds:

```text
Err result
    -> terminal
    -> replay-stable
    -> same MatchResult arm on same-key replay
```

## retryable `Err` path

If `err.disposition = retryable`, the observed error is attempt-level and not a fixed terminal result.

External idempotency does not prove that a later attempt will observe the same `Err`.

A proof that depends on re-entering the same `err` branch therefore needs some other fact or remains `Unknown`.

## unspecified `Err`

No replay-stability fact is available.

---

# 16. Request-effect results are not changed by external-effect idempotency

This amendment directly changes **external effects**.

A `RequestEffect` still derives result replay from the modeled target request operation.

Its result is replay-stable under the existing request-effect rules when the target operation's declared/proven result-replay requirement establishes it for the corresponding logical request class.

Do not replace those rules with `ExternalEffect` semantics.

---

# 17. Error disposition applies to request contracts too

Although the strengthened idempotency rule is specific to `ExternalEffect`, `ResultType` is shared by `RequestInput` and `ExternalEffect.result`.

Therefore error disposition applies consistently to both.

A request input may declare `Err terminal` or `Err retryable` as part of its synchronous result contract.

For request effects, this describes whether an error outcome returned by the target operation semantically admits another logical request attempt.

It does not itself cause the caller to retry.

---

# 18. Distinguish retryability from `RequestEffect.retry`

Current request-effect `RetrySemantics` describes whether the request boundary may issue repeated attempts:

```text
Unspecified
Never
MayRepeat
```

Error disposition describes something else:

```text
Is another attempt semantically admitted after this Err?
```

Therefore:

```text
RetrySemantics != ErrorDisposition
```

For example:

```text
Err retryable
+
RequestEffect.retry = Never
```

is coherent: the target says another attempt would be semantically admissible, but this request effect declares that it never repeats.

Likewise:

```text
Err terminal
+
RequestEffect.retry = MayRepeat
```

means the request boundary may have repeated attempts for other reasons, but observing the terminal `Err` resolves the logical result; a conforming retry mechanism must not reinterpret it as an unresolved attempt.

No automatic coupling is introduced in this amendment.

---

# 19. No retry policy in this amendment

Do not add:

```text
RetryPolicy
RetryOn
MaxAttempts
Backoff
Timeout
RetryLoop
```

to operation control in this amendment.

`ErrorDisposition::Retryable` is a capability/contract fact:

```text
another attempt is semantically admitted
```

It does not describe the mechanism that performs one.

A future retry-execution revision may consume this fact.

---

# 20. Validation changes

Update `ResultType` validation.

For every result contract, `ok` and `err.schema` must reference valid schemas.

`err.disposition` is always syntactically present in canonical form.

No default may silently convert omission into `terminal` or `retryable`, because `unspecified` is epistemic.

If shorthand is later added, it must preserve this rule.

---

# 21. External-effect validation

For `ExternalEffect.result = Some(ResultType)`, validate both variant schemas and the error disposition.

No additional `result_replay` field exists.

`ExternalEffect.idempotency` retains the existing key validation.

The validator does not prove that the real external service actually satisfies stable terminal-result semantics; that remains a conformance obligation attached to the declaration.

---

# 22. Solver changes: external result stability

Replace the current external-result branch of replay-stability analysis.

Current conceptual rule:

```text
external result
    -> never replay-stable in V1
```

New conceptual rule:

```text
external result r
    |
    +-- effect.idempotency != DeduplicatedBy
    |       -> Unknown
    |
    +-- dedup key not replay-stable/class-fixed
    |       -> Unknown
    |
    +-- observed variant = Ok
    |       -> Proven replay-stable
    |
    +-- observed variant = Err
            |
            +-- Terminal
            |       -> Proven replay-stable
            |
            +-- Retryable
            |       -> Unknown as terminal replay result
            |
            +-- Unspecified
                    -> Unknown
```

`Unknown` here means only that the observed value cannot be used as a replay-stable root for the relevant proof.

It does not mean the declaration is invalid.

---

# 23. Solver changes: replay-consistent request results

A request operation may construct its terminal result from an external result.

Under the new rule, this can now prove `ResultReplayRequirement::ReplayConsistent` when the relevant external result root is terminal and replay-stable through the strengthened `DeduplicatedBy` guarantee.

Example:

```text
Execute external Charge
    idempotency = DeduplicatedBy(payment_id)
    -> r

MatchResult r:

    ok:
        Return Ok(
            deterministic_from EffectResultOk(r)
        )

    err:
        terminal error
        Return Err(
            deterministic_from EffectResultErr(r)
        )
```

If the external key is class-fixed and both terminal paths satisfy ordinary provenance rules, the external boundary no longer automatically destroys request-result replayability.

---

# 24. Retryable errors and operation result replay

If an operation immediately maps a retryable external `Err` into a terminal operation result:

```text
external Err(retryable)
    -> Return Err(...)
```

that is an explicit operation-level decision and is allowed.

However, the external retryable error itself is not established to be replay-stable by `DeduplicatedBy`, so an operation-level replay-consistency proof depending on that error payload may remain `Unknown`.

A later retry-policy revision may instead consume the retryable error and attempt the external interaction again before the operation terminally resolves.

---

# 25. Idempotency safety implications

Strengthening external `DeduplicatedBy` does not change its duplicate-work safety role.

For an upstream operation idempotency proof:

```text
external effect
+
DeduplicatedBy(Ke)
+
Ke class-fixed
```

continues to prove that repeated same-class operation attempts do not perform duplicate logical external work.

The new semantic addition is:

```text
if the external logical interaction has terminally resolved,
its same-key terminal result is also stable
```

These are now two consequences of the same external idempotency guarantee.

---

# 26. `NotDeduplicated` and `Unspecified`

Retain the existing distinctions.

## `Unspecified`

No usable external duplicate-work or terminal-result stability fact.

## `NotDeduplicated`

Explicitly denies same-key logical deduplication.

Therefore it also cannot establish stable same-key terminal-result semantics through the strengthened `DeduplicatedBy` rule.

Do not infer that a `NotDeduplicated` boundary necessarily returns different results; only that the `DeduplicatedBy` guarantee is unavailable.

---

# 27. Documentation changes

Remove normative language equivalent to:

> `deduplicated_by` collapses only the work and does not imply that each execution returns the same result.

Remove:

> an external result is never replay-stable in V1.

Replace with:

> For a result-bearing `ExternalEffect`, `DeduplicatedBy { key }` identifies one logical external interaction for equal evaluated keys. In addition to suppressing duplicate logical work, it fixes the interaction's terminal logical `Result`: after the first terminal `Ok` or terminal `Err`, every subsequent same-key execution observes the same variant and replay-equivalent payload. Retryable `Err` outcomes are attempt-level, nonterminal outcomes and do not establish the logical interaction's terminal result.

Update the replay-stable-provenance section accordingly.

---

# 28. Current source comments to revise

The current `ExternalEffect.result` comments state that declaring a result says nothing about repeated-result consistency and that an external result is never replay-stable in V1.

Replace those comments to reflect:

```text
result shape
+
error disposition
+
DeduplicatedBy terminal-result semantics
```

Do not add a separate result-replay field.

---

# 29. Migration of existing `ResultType` declarations

Existing canonical:

```yaml
result:
  ok: schema.Success
  err: schema.Error
```

must migrate to:

```yaml
result:
  ok: schema.Success
  err:
    schema: schema.Error
    disposition: unspecified
```

unless the author can truthfully declare `terminal` or `retryable`.

The migration must not silently assume terminality.

---

# 30. Example canonical declarations

## Terminal error

```yaml
result:
  ok: schema.ChargeAccepted
  err:
    schema: schema.CardDeclined
    disposition: terminal
```

## Retryable error

```yaml
result:
  ok: schema.ChargeAccepted
  err:
    schema: schema.GatewayUnavailable
    disposition: retryable
```

## Unknown semantics

```yaml
result:
  ok: schema.ChargeAccepted
  err:
    schema: schema.ProviderError
    disposition: unspecified
```

---

# 31. Limitation: heterogeneous error classes

A single `Err` schema with one disposition cannot express:

```text
CardDeclined       terminal
FraudRejected      terminal
RateLimited        retryable
GatewayBusy        retryable
```

inside one result contract.

Do not solve that in this amendment.

The current amendment intentionally supports one disposition for the declared `Err` variant.

If real architectures require heterogeneous error-class retry semantics, a later revision may introduce structurally explicit error variants or conditional disposition over the `Err` payload.

---

# 32. Validation tests

Add/update tests for:

1. `ResultType.err` parses with `schema + disposition`;
2. canonical serialization always emits disposition;
3. `terminal`, `retryable`, and `unspecified` all parse;
4. invalid disposition is rejected;
5. missing error schema is rejected;
6. publication effects still cannot bind a result;
7. external effect result schemas are validated;
8. request-input result schemas are validated;
9. external `DeduplicatedBy` requires no extra result-replay declaration;
10. existing external idempotency-key reference/path validation remains unchanged.

---

# 33. Solver tests

Add focused tests.

## Idempotent external `Ok`

```text
DeduplicatedBy(K)
+
K class-fixed
+
Ok(v)
```

must make the bound `Ok` payload replay-stable.

## Idempotent terminal `Err`

```text
DeduplicatedBy(K)
+
K class-fixed
+
Err(e)
+
disposition = terminal
```

must make the bound `Err` payload replay-stable.

## Idempotent retryable `Err`

```text
DeduplicatedBy(K)
+
K class-fixed
+
Err(e)
+
disposition = retryable
```

must **not** make the observed error a replay-stable terminal-result root.

## Idempotent unspecified `Err`

Result remains `Unknown` for terminal replay stability.

## Non-idempotent external result

Without proven `DeduplicatedBy`, no external result replay-stability fact is obtained from this amendment.

## Match result

A match on an idempotent external terminal result may be proven replay-stable.

A match whose controlling path is a retryable/unspecified external `Err` must not be proven stable from external idempotency alone.

---

# 34. Requirement-level tests

Add tests showing that an operation result previously blocked solely because it depended on an external result may now prove `replay_consistent` when:

```text
external effect DeduplicatedBy
+
external key class-fixed
+
terminal result
+
terminal payload derivation/provenance valid
```

Also test that retryable-error-derived terminal operation results remain unproven when the proof depends on stable replay of the retryable error payload.

---

# 35. Performance-layer relevance

This amendment intentionally does not implement probabilistic performance analysis, but its semantics should support that future layer.

A future performance overlay may distinguish:

```text
P(Ok terminal)
P(Err terminal)
P(Err retryable)
```

and use retry policies to derive attempt-count distributions, latency amplification, downstream throughput amplification, and retry/saturation risk.

The correctness DSL should expose the semantic distinction now without embedding performance behavior into the core model.

---

# 36. Primitives not redesigned

This amendment does not redesign:

```text
Transaction
TransactionOutput
EffectIntent
StateTransition
OperationBlock
OperationStep
MatchResult
Branch
Return
Complete

RequestEffect.retry
IdempotencyRequirement
RecoverabilityRequirement
ResultReplayRequirement
```

It modifies only:

```text
ResultType
ExternalEffect.idempotency semantics
external effect-result replay rules
```

---

# 37. Acceptance criteria

The amendment is complete when all of the following hold.

## Strong external idempotency

For a result-bearing external effect, `DeduplicatedBy(K)` means same-key executions identify one logical external interaction whose terminal result is stable.

## No duplicate-response loophole

A same-key duplicate response such as `Err(AlreadyProcessed)` after the logical interaction previously returned `Ok(original)` does not conform to the modeled `DeduplicatedBy` guarantee unless the boundary abstraction maps it back to the original logical result.

## Error disposition

`ResultType.err` explicitly declares one of:

```text
unspecified
terminal
retryable
```

## Retryable error

A retryable `Err` is conclusive for one attempt but nonterminal for the logical interaction.

It admits another attempt semantically but causes no retry by itself.

## Replay analysis

A terminal external result is replay-stable when:

```text
external effect is DeduplicatedBy
+
external key is class-fixed
```

A retryable or unspecified external `Err` does not gain terminal replay stability merely from idempotency.

## No retry policy

No loop, backoff, max-attempt, or retry-execution semantics are introduced.

---

# 38. Normative replacement text

> **External idempotency and terminal results.** For an `ExternalEffect` declared `DeduplicatedBy { key }`, equal evaluated keys identify one logical external-effect execution. The guarantee suppresses duplicate logical work and, when the effect has a synchronous result, also fixes the logical execution's terminal result. Once the execution returns a terminal `Ok` or terminal `Err`, every subsequent same-key execution observes the same result variant and replay-equivalent payload.
>
> A retryable `Err` is an attempt-level, nonterminal outcome. It conclusively reports the result of that attempt and semantically admits another attempt, but it does not establish the logical external execution's terminal result. Conseqo does not infer that a retry actually occurs, that it eventually succeeds, or that later retryable errors equal the first.
>
> An `Err` with unspecified disposition provides no usable fact about terminality or retryability.
>
> Therefore, relative to a governing operation replay class, an external terminal result is replay-stable when the external effect is `DeduplicatedBy { key }` and the evaluated external key is class-fixed under the existing provenance rules. A retryable or unspecified `Err` is not promoted into a replay-stable terminal result by that guarantee alone.

---

# 39. Summary

Before:

```text
External DeduplicatedBy(K)
    -> same-key work deduplicated
    -> external result still never replay-stable
```

After:

```text
External DeduplicatedBy(K)
    -> one logical external interaction
       + duplicate logical work suppressed
       + terminal Result fixed
```

while:

```text
Err(retryable)
    -> attempt completed
    -> logical interaction still unresolved
    -> another attempt semantically admitted
```

and:

```text
Err(terminal) or Ok
    -> logical interaction resolved
    -> same-key terminal result stable
```

The guiding rule is:

> **For Conseqo, idempotency of a result-bearing external interaction includes consistency of its terminal logical result. Retryable errors are nonterminal attempt outcomes, not contradictory terminal results.**

---

# 40. Reconciliation (implemented 2026-09-05)

How the implementation realized each section, and where it exercised
judgment the text left open.

## 40.1 Spec surface

`ResultType.err` became `ErrorResultType { schema, disposition }` with
`ErrorDisposition::{Unspecified, Terminal, Retryable}` (§7), shared by
`RequestInput.result` and `ExternalEffect.result` (§17). Serialization
always emits both fields (§20). Judgment call on §20/§29: the repo
already has a shorthand surface (fields, paths, value sources,
selector values), so the bare-schema form `err: schema.X` is accepted
as shorthand meaning `disposition: unspecified` — existing models keep
parsing, and the rule §20 protects is preserved: nothing converts
omission into `terminal` or `retryable`. Omitting `disposition` inside
the map form likewise defaults to `unspecified`. No separate
result-replay guarantee or field exists (§4, §21); no retry policy,
loop, or attempt vocabulary was added (§19); no primitive of §36 was
touched.

## 40.2 Solver

The replay engine's result judgment became **per variant**
(`BoundResult { ok, err }` in the path context), because one binding's
stability now differs between arms (§15). The external branch of
`result_replay` implements the §22 tree: guarantee, then key
stability (judged by the same §18 root rules the idempotency leg uses
— key equality alone, no instance condition, §25), then variant
terminality. `ResultGap::ExternalResultUndeclared` was replaced by
`ExternalNotDeduplicated`, `ExternalDeduplicationUnknown`,
`ExternalDeduplicationKeyUnstable`, `ExternalErrorRetryable`, and
`ExternalErrorDispositionUnspecified` (§13, §22, §26); the stable side
is cited as `ResultStabilityRule::ExternalTerminalResult { variant,
key }` and, for value roots,
`StabilityRule::DeduplicatedExternalResult`. A `match_result` rests on
the judgment of the arm it takes (§15): a value root's variant is the
source kind (`effect_result_ok`/`err`), a decision's the arm. Request
results are untouched (§16) and remain one judgment in both variants.
Request-result proofs may now rest on external terminal results (§23);
retryable-derived terminal results stay allowed at the operation level
and unproven for replay consistency (§24). The idempotency checker's
external duplicate-work leg is unchanged (§25).

## 40.3 Fixtures and tests

`video_streaming` declares the engine's `RenderFailed` terminal — a
rejected source terminally fails the job — and now proves all 15
obligations; its header documents that a retryable or undeclared
disposition would reopen the gap. `flash_checkout` keeps the card
charge `not_deduplicated`, so `charge_payment` remains unproven with
the new gap prose (its committed report regenerated). §32's validation
tests: disposition parse/serialize/reject and missing-schema rejection
in `tests/parser.rs`; the referenced-schema, publication-binding, and
key-validation rules were already covered and unchanged. §33/§34's
solver tests in `tests/verification.rs`: terminal `Ok` and terminal
`Err` stability (idempotency proof over the branching charge, and
`replay_consistent` proven for a return derived from an external
result), retryable and unspecified non-promotion, and the
undeduplicated case — the ok arm gains nothing there either.

## 40.4 Documentation

`ARCHSPEC_DSL_SEMANTICS.md` carries the normative replacement (§27,
§38): the strengthened `deduplicated_by` section, an
`ErrorDisposition` section (with the §18 `retry` orthogonality), the
rewritten `ExternalEffect.result` section, and per-variant §18 rule 6.
The superseded language was removed there and annotated as superseded
in `ARCHSPEC_OPERATION_EXECUTION_REVISION_DRAFT_V3.md` §48.2 and
`ARCHSPEC_EFFECT_SAFETY_DRAFT.md`; open question 11 in
`ARCHSPEC_SEMANTICS_REVISION_DRAFT.md` §27 is resolved by this
amendment, recording what stays open: heterogeneous error-class
dispositions (§31) and a retry-execution revision (§19, §35). The
`ExternalEffect.result` source comments were rewritten (§28). The
visualization renders dispositions in result contracts and explains
the external rule per guarantee and disposition.
