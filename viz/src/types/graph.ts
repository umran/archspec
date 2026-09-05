// Mirror of `src/bin/viz/graph.rs`: the derived system graph.

import type { Id } from "./model";

export const CLIENT_NODE_ID = "@client";
export const EXTERNAL_PREFIX = "@external:";

export interface Graph {
  services: ServiceNode[];
  operations: OperationNode[];
  topics: TopicNode[];
  externals: ExternalNode[];
  client: ClientNode | null;
  edges: Edge[];
  effect_owners: Record<Id, EffectOwner>;
  transition_refs: Record<string, TransitionRef[]>;
}

export interface ServiceNode {
  id: Id;
  kind: string;
  operations: Id[];
}

export interface RequirementBadges {
  serialization: number;
  ordering: number;
  idempotency: number;
  recoverability: number;
}

export interface OperationNode {
  id: Id;
  service: Id;
  description: string | null;
  inputs: number;
  /** Steps of the operation program, nested ones included. */
  steps: number;
  machines: Id[];
  requirements: RequirementBadges;
  concurrency: string;
}

export interface TopicNode {
  id: Id;
  ordering: string;
  messages: Id[];
}

export interface ExternalNode {
  id: string;
  name: string;
}

export interface ClientNode {
  id: string;
}

export interface TransitionKey {
  machine: Id;
  transition: Id;
}

interface EdgeBase {
  id: string;
  from: string;
  to: string;
}

export type Edge = EdgeBase &
  (
    | {
        kind: "publish";
        operation: Id;
        effect: Id;
        schema: Id;
        via_transition: TransitionKey | null;
        /** Program steps executing the effect, as step locations. */
        executed_at: string[];
      }
    | {
        kind: "subscribe";
        operation: Id;
        input: Id;
        schemas: Id[];
        delivery: string;
        routing: string;
        lane_concurrency: string;
      }
    | {
        kind: "request";
        operation: Id;
        effect: Id;
        input: Id;
        schema: Id;
        retry: string;
        via_transition: TransitionKey | null;
        executed_at: string[];
      }
    | {
        kind: "external";
        operation: Id;
        effect: Id;
        idempotency: string;
        executed_at: string[];
      }
    | { kind: "client"; operation: Id; input: Id; schema: Id }
  );

export type EdgeKind = Edge["kind"];

export type EffectOwner =
  | { kind: "operation"; operation: Id }
  | { kind: "transition"; machine: Id; transition: Id };

export interface TransitionRef {
  operation: Id;
  transaction: Id;
  step: number;
}
