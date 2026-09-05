import type {
  Effect, Id, Model, Operation, OperationBlock, OperationStep, ResultType, TransitionSideEffect,
} from "../types/model";
import { shortId } from "./ids";

/** Where an id is declared, resolved once for the whole model. */
export type IndexEntry =
  | { kind: "service" }
  | { kind: "operation" }
  | { kind: "schema" }
  | { kind: "topic" }
  | { kind: "data_model" }
  | { kind: "object"; dataModel: Id }
  | { kind: "machine" }
  | { kind: "state"; machine: Id }
  | { kind: "transition"; machine: Id }
  | { kind: "input"; op: Id }
  | { kind: "effect"; op?: Id; machine?: Id; transition?: Id }
  | { kind: "intent"; op: Id }
  | { kind: "output"; op: Id }
  /** A result binding declared by a program step; `effect` is what it observes. */
  | { kind: "binding"; op: Id; effect: Id; location: string }
  | { kind: "transaction"; op: Id };

export type ModelIndex = Map<Id, IndexEntry>;

/** One hop of a step location: the step's index in its block and, for
 *  every level but the last, the arm entered beneath it. */
export interface StepHop {
  step: number;
  arm?: "ok" | "err" | "then" | "otherwise";
}

/** A step location rendered as the checker names it: one-based, `3.ok.1`
 *  for the first step of the ok arm of the third top-level step. */
export function locationLabel(hops: StepHop[]): string {
  return hops.map((h) => `${h.step + 1}${h.arm ? `.${h.arm}` : ""}`).join(".");
}

export interface LocatedStep {
  location: string;
  hops: StepHop[];
  step: OperationStep;
}

/** Every step of a program with its location, depth first in program order. */
export function walkProgram(block: OperationBlock, parent: StepHop[] = []): LocatedStep[] {
  const out: LocatedStep[] = [];
  block.steps.forEach((step, index) => {
    const hops = [...parent, { step: index }];
    out.push({ location: locationLabel(hops), hops, step });
    const under = (arm: StepHop["arm"]) => [...parent, { step: index, arm }];
    if (step.kind === "match_result") {
      out.push(...walkProgram(step.ok, under("ok")));
      out.push(...walkProgram(step.err, under("err")));
    } else if (step.kind === "branch") {
      out.push(...walkProgram(step.then, under("then")));
      if (step.otherwise) out.push(...walkProgram(step.otherwise, under("otherwise")));
    }
  });
  return out;
}

export function buildIndex(model: Model): ModelIndex {
  const index: ModelIndex = new Map();
  const put = (id: Id, entry: IndexEntry) => {
    if (!index.has(id)) index.set(id, entry);
  };

  for (const id of Object.keys(model.services)) put(id, { kind: "service" });
  for (const id of Object.keys(model.schemas)) put(id, { kind: "schema" });
  for (const id of Object.keys(model.topics)) put(id, { kind: "topic" });

  for (const [dmId, dm] of Object.entries(model.data_models)) {
    put(dmId, { kind: "data_model" });
    for (const objId of Object.keys(dm.objects)) put(objId, { kind: "object", dataModel: dmId });
  }

  for (const [mId, m] of Object.entries(model.state_machines)) {
    put(mId, { kind: "machine" });
    for (const s of m.states) put(s, { kind: "state", machine: mId });
    for (const tId of Object.keys(m.transitions)) put(tId, { kind: "transition", machine: mId });
  }

  for (const [opId, op] of Object.entries(model.operations)) {
    put(opId, { kind: "operation" });
    for (const id of Object.keys(op.inputs)) put(id, { kind: "input", op: opId });
    for (const id of Object.keys(op.effects)) put(id, { kind: "effect", op: opId });
    for (const id of Object.keys(op.effect_intents)) put(id, { kind: "intent", op: opId });
    for (const id of Object.keys(op.transaction_outputs)) put(id, { kind: "output", op: opId });
    for (const id of Object.keys(op.transactions)) put(id, { kind: "transaction", op: opId });
    for (const { location, step } of walkProgram(op.program)) {
      if (step.kind === "execute_effect" && step.result) {
        put(step.result, { kind: "binding", op: opId, effect: step.effect, location });
      } else if (step.kind === "execute_effect_intent" && step.result) {
        const effect = op.effect_intents[step.intent]?.effect;
        if (effect) put(step.result, { kind: "binding", op: opId, effect, location });
      }
    }
  }

  for (const [mId, m] of Object.entries(model.state_machines)) {
    for (const [tId, t] of Object.entries(m.transitions)) {
      for (const eId of Object.keys(t.side_effects)) {
        put(eId, { kind: "effect", machine: mId, transition: tId });
      }
    }
  }

  return index;
}

export interface EffectDef {
  effect: Effect | TransitionSideEffect;
  owner: Extract<IndexEntry, { kind: "effect" }>;
}

export function effectDef(model: Model, index: ModelIndex, effectId: Id): EffectDef | null {
  const owner = index.get(effectId);
  if (!owner || owner.kind !== "effect") return null;
  if (owner.op !== undefined) {
    const effect = model.operations[owner.op]?.effects[effectId];
    return effect ? { effect, owner } : null;
  }
  if (owner.machine !== undefined && owner.transition !== undefined) {
    const effect =
      model.state_machines[owner.machine]?.transitions[owner.transition]?.side_effects[effectId];
    return effect ? { effect, owner } : null;
  }
  return null;
}

export function effectSummary(model: Model, index: ModelIndex, effectId: Id): string {
  const def = effectDef(model, index, effectId);
  if (!def) return "unresolved effect";
  const e = def.effect;
  switch (e.kind) {
    case "publication":
      return `publish ${shortId(e.schema)} → ${shortId(e.topic)}`;
    case "request":
      return `request ${shortId(e.target.operation)} (${shortId(e.target.input)}) · retry ${e.retry}`;
    case "external":
      return `external ${e.name} · ${e.idempotency.kind}`;
  }
}

/** The `Result<Ok, Err>` contract an effect's execution yields: a
 *  request inherits its target input's, an external effect declares its
 *  own, a publication has none. */
export function effectResultType(model: Model, index: ModelIndex, effectId: Id): ResultType | null {
  const def = effectDef(model, index, effectId);
  if (!def) return null;
  const e = def.effect;
  switch (e.kind) {
    case "publication":
      return null;
    case "external":
      return e.result;
    case "request": {
      const input = model.operations[e.target.operation]?.inputs[e.target.input];
      return input?.kind === "request" ? input.result : null;
    }
  }
}

/** The transaction that establishes an artifact: an output or intent by
 *  an explicit establishing step, or an intent implicitly established by
 *  the transaction whose transition owns the intent's effect. */
export function establishingTransaction(model: Model, operation: Operation, artifact: Id): Id | null {
  for (const [txId, tx] of Object.entries(operation.transactions)) {
    for (const step of tx.steps) {
      if (step.kind === "establish_transaction_output" && step.output === artifact) return txId;
      if (step.kind === "establish_effect_intent" && step.intent === artifact) return txId;
    }
  }
  const intent = operation.effect_intents[artifact];
  if (!intent) return null;
  for (const [txId, tx] of Object.entries(operation.transactions)) {
    for (const step of tx.steps) {
      if (step.kind !== "transition") continue;
      const transition = model.state_machines[step.machine]?.transitions[step.transition];
      if (transition && intent.effect in transition.side_effects) return txId;
    }
  }
  return null;
}

/** Operations that execute an effect through a declared intent. */
export function intentExecutors(model: Model, effectId: Id): { op: Id; intent: Id }[] {
  const out: { op: Id; intent: Id }[] = [];
  for (const [opId, op] of Object.entries(model.operations)) {
    for (const [intentId, intent] of Object.entries(op.effect_intents)) {
      if (intent.effect === effectId) out.push({ op: opId, intent: intentId });
    }
  }
  return out;
}
