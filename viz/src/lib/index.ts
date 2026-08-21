import type { Effect, Id, Model, Operation, TransitionSideEffect } from "../types/model";
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
  | { kind: "result"; op: Id }
  | { kind: "response"; op: Id }
  | { kind: "transaction"; op: Id }
  | { kind: "flow"; op: Id };

export type ModelIndex = Map<Id, IndexEntry>;

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
    for (const id of Object.keys(op.invocation_results)) put(id, { kind: "result", op: opId });
    for (const id of Object.keys(op.responses)) put(id, { kind: "response", op: opId });
    for (const id of Object.keys(op.transactions)) put(id, { kind: "transaction", op: opId });
    for (const id of Object.keys(op.flows)) put(id, { kind: "flow", op: opId });
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

/** The first declared flow whose steps run the transaction, if any. */
export function flowContaining(operation: Operation, transaction: Id): Id | null {
  for (const [flowId, flow] of Object.entries(operation.flows)) {
    if (flow.steps.some((s) => s.kind === "transaction" && s.transaction === transaction)) return flowId;
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
