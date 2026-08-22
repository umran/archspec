import type {
  Concurrency,
  Derivation,
  SelectorPredicate,
  TypeRef,
  ValueRef,
} from "../types/model";
import { pathText, shortId } from "./ids";

export function refString(ref: ValueRef, short = true): string {
  const id = short ? shortId(ref.source.id) : ref.source.id;
  return `${ref.source.kind}(${id}).${pathText(ref.path)}`;
}

export function predicateText(p: SelectorPredicate): string {
  switch (p.kind) {
    case "all":
      return "all instances";
    case "eq": {
      const v =
        p.value.kind === "value" ? refString(p.value.value) : JSON.stringify(p.value.value.value);
      return `${pathText(p.field)} = ${v}`;
    }
    case "and":
      return p.predicates.map(predicateText).join(" ∧ ");
  }
}

export function derivationText(d: Derivation): string {
  return d.kind === "deterministic" ? `deterministic from ${d.from.length} root(s)` : "unspecified";
}

export function concurrencyText(c: Concurrency): string {
  return c.kind === "bounded" ? `bounded(${c.value})` : c.kind;
}

/** Plain rendering of a type, or null when it should be rendered as a schema link. */
export function typeText(ty: TypeRef): string | null {
  switch (ty.kind) {
    case "scalar":
      return ty.value;
    case "schema":
      return null;
    case "list": {
      const inner = typeText(ty.value);
      return inner === null ? null : `list<${inner}>`;
    }
  }
}
