// TypeScript mirror of the serialized `archspec::spec::Model`. Shapes
// follow the serde conventions in `src/spec/`: internally tagged enums
// carry `kind`, maps are plain objects keyed by id.

export type Id = string;
export type FieldPath = string[];

export interface Model {
  revision: number;
  services: Record<Id, Service>;
  schemas: Record<Id, Schema>;
  data_models: Record<Id, DataModel>;
  topics: Record<Id, Topic>;
  state_machines: Record<Id, StateMachine>;
  operations: Record<Id, Operation>;
}

export interface Service {
  kind: "backend" | "frontend" | "worker" | "job";
}

export type ScalarType =
  | "string"
  | "bool"
  | "int"
  | "float"
  | "decimal"
  | "uuid"
  | "timestamp";

export type TypeRef =
  | { kind: "scalar"; value: ScalarType }
  | { kind: "schema"; value: Id }
  | { kind: "list"; value: TypeRef };

export interface Field {
  ty: TypeRef;
  optional: boolean;
}

export type Schema =
  | {
      kind: "canonical";
      description: string | null;
      completeness: "partial" | "complete";
      fields: Record<string, Field>;
    }
  | { kind: "fragment"; source: Id; mapping: Record<string, FieldPath> };

export interface DataModel {
  objects: Record<Id, DataObject>;
}

export interface DataObject {
  schema: Id;
  identity: FieldPath[];
  requirements: { history: "linearizable"[] };
}

export type TopicOrdering =
  | { kind: "unspecified" }
  | { kind: "unordered" }
  | { kind: "global" }
  | { kind: "keyed"; mapping: Record<Id, FieldPath> };

export type MessageIdentity =
  | { kind: "unspecified" }
  | { kind: "keyed"; mapping: Record<Id, FieldPath[]> };

export interface Topic {
  messages: Id[];
  ordering: TopicOrdering;
  message_identity: MessageIdentity;
}

export interface StateMachine {
  subject: { kind: "object"; object: Id; state: FieldPath };
  states: Id[];
  initial: Id;
  transitions: Record<Id, Transition>;
}

export interface Transition {
  from: Id[];
  to: Id;
  side_effects: Record<Id, TransitionSideEffect>;
}

export type TransitionSideEffect =
  | ({ kind: "publication" } & PublicationEffect)
  | ({ kind: "request" } & RequestEffect);

export type ValueSourceKind =
  | "input"
  | "effect"
  | "invocation_result"
  | "state_machine_subject"
  | "transaction_read";

export interface ValueSource {
  kind: ValueSourceKind;
  id: Id;
}

export interface ValueRef {
  source: ValueSource;
  path: FieldPath;
}

export type Derivation =
  | { kind: "unspecified" }
  | { kind: "deterministic"; from: ValueRef[] };

export interface IdempotencyKey {
  components: ValueRef[];
}

export interface IdempotencyKeyPropagation {
  source: IdempotencyKey;
  target: IdempotencyKey;
}

export type IdempotencyGuarantee =
  | { kind: "unspecified" }
  | { kind: "not_deduplicated" }
  | { kind: "deduplicated_by"; key: IdempotencyKey };

export interface PublicationEffect {
  topic: Id;
  schema: Id;
  idempotency_key_propagation: IdempotencyKeyPropagation[];
}

export interface RequestEffect {
  target: { operation: Id; input: Id };
  schema: Id;
  retry: "unspecified" | "never" | "may_repeat";
  idempotency_key_propagation: IdempotencyKeyPropagation[];
}

export interface ExternalEffect {
  name: string;
  idempotency: IdempotencyGuarantee;
}

export type Effect =
  | ({ kind: "publication" } & PublicationEffect)
  | ({ kind: "request" } & RequestEffect)
  | ({ kind: "external" } & ExternalEffect);

export type Concurrency =
  | { kind: "unspecified" }
  | { kind: "bounded"; value: number }
  | { kind: "unbounded" };

export type RequestIdentity =
  | { kind: "unspecified" }
  | { kind: "keyed"; fields: FieldPath[] };

export type MessageSelector = { kind: "all" } | { kind: "only"; schemas: Id[] };

export type DeliverySemantics = "unspecified" | "at_most_once" | "at_least_once";

export type DispatchRouting =
  | "unspecified"
  | "unconstrained"
  | "single_lane"
  | "by_topic_key";

export type Input =
  | { kind: "request"; schema: Id; identity: RequestIdentity }
  | {
      kind: "subscription";
      topic: Id;
      messages: MessageSelector;
      delivery: DeliverySemantics;
      dispatch: { routing: DispatchRouting; lane_concurrency: Concurrency };
    };

export interface Response {
  request: Id;
  schema: Id;
  source: { kind: "unspecified" } | { kind: "invocation_result"; result: Id };
}

export type Literal =
  | { kind: "string"; value: string }
  | { kind: "bool"; value: boolean }
  | { kind: "int"; value: number };

export type SelectorValue =
  | { kind: "value"; value: ValueRef }
  | { kind: "literal"; value: Literal };

export type SelectorPredicate =
  | { kind: "all" }
  | { kind: "eq"; field: FieldPath; value: SelectorValue }
  | { kind: "and"; predicates: SelectorPredicate[] };

export interface ObjectSelector {
  object: Id;
  predicate: SelectorPredicate;
}

export type FieldSelection = { kind: "all" } | { kind: "only"; fields: FieldPath[] };

export type LockOrder =
  | { kind: "unspecified" }
  | { kind: "by"; terms: { field: FieldPath; direction: "ascending" | "descending" }[] };

export type TransactionStep =
  | { kind: "read"; result: Id; target: ObjectSelector; fields: FieldSelection }
  | { kind: "write"; target: ObjectSelector; fields: FieldPath[]; values: Derivation }
  | { kind: "insert"; object: Id; values: Derivation }
  | { kind: "delete"; target: ObjectSelector }
  | { kind: "lock"; target: ObjectSelector; mode: "shared" | "exclusive"; order: LockOrder }
  | {
      kind: "transition";
      machine: Id;
      transition: Id;
      subject: ObjectSelector;
      effect_values: Record<Id, Derivation>;
    }
  | { kind: "establish_effect_intent"; intent: Id; values: Derivation }
  | { kind: "establish_invocation_result"; result: Id; values: Derivation };

export interface Transaction {
  data_model: Id | null;
  isolation: "unspecified" | "read_committed" | "snapshot" | "serializable";
  idempotency: IdempotencyGuarantee;
  steps: TransactionStep[];
}

export type FlowStep =
  | { kind: "transaction"; transaction: Id }
  | { kind: "execute_effect"; effect: Id; values: Derivation }
  | { kind: "execute_effect_intent"; intent: Id };

export interface InvocationFlow {
  steps: FlowStep[];
  response: Id | null;
}

export interface OperationRequirements {
  serialization: { key: ValueRef }[];
  ordering: { key: ValueRef }[];
  idempotency: { key: IdempotencyKey; response: "unspecified" | "replay_consistent" }[];
  recoverability: { key: IdempotencyKey; completion: "resumable" | "guaranteed" }[];
}

export type RequirementKind = keyof OperationRequirements;

export interface Operation {
  service: Id;
  description: string | null;
  inputs: Record<Id, Input>;
  effects: Record<Id, Effect>;
  effect_intents: Record<Id, { effect: Id }>;
  invocation_results: Record<Id, { schema: Id }>;
  responses: Record<Id, Response>;
  transactions: Record<Id, Transaction>;
  flows: Record<Id, InvocationFlow>;
  requirements: OperationRequirements;
  execution: { concurrency: Concurrency };
}
