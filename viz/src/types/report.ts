// Mirror of `archspec::analyzer::report`: the obligation report.

import type { Id } from "./model";

export type Status = "proven" | "disproven" | "unknown";

export type Property =
  | { kind: "serialization" }
  | { kind: "ordering" }
  | { kind: "idempotency" }
  | { kind: "recoverability" }
  | { kind: "response_replay" }
  | { kind: "object_history" }
  | { kind: "custom"; name: string };

export type Subject =
  | { kind: "operation"; operation: Id; requirement?: number }
  | { kind: "flow"; operation: Id; flow: Id }
  | { kind: "transaction"; operation: Id; transaction: Id }
  | { kind: "object"; data_model: Id; object: Id }
  | { kind: "state_machine"; machine: Id; transition?: Id }
  | { kind: "topic"; topic: Id };

export interface EvidenceItem {
  subject?: Id;
  message: string;
}

export interface TraceStep {
  actor?: Id;
  description: string;
}

export interface Obligation {
  id: string;
  property: Property;
  subject: Subject;
  status: Status;
  summary: string;
  assumptions: string[];
  evidence: EvidenceItem[];
  counterexample?: { trace: TraceStep[] };
}

export interface ProverReport {
  format: number;
  model_revision: number | null;
  obligations: Obligation[];
  /** Model-wide warnings that belong to no single obligation. */
  notes?: EvidenceItem[];
}

export function propertyName(property: Property): string {
  return property.kind === "custom" ? property.name : property.kind;
}
