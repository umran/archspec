// Declared facts, said the way a reader would say them: a short label for
// a badge, and one sentence on what the fact implies for retries,
// duplicates, and proofs. The DSL's enum names are precise but opaque;
// these are what they mean (ARCHSPEC_DSL_SEMANTICS.md §8, §9, §13, §17).

import type { Concurrency, IdempotencyGuarantee, Input, ResultType, Topic } from "../types/model";
import { pathText } from "./ids";

export type Tone = "success" | "warning" | "neutral" | "info";

export interface Explanation {
  /** Badge text. */
  label: string;
  tone: Tone;
  /** What the fact implies, in one or two sentences. */
  summary: string;
}

/** A transaction's commit deduplication: whether a retry commits again or
 *  recovers the first commit. */
export function commitGuarantee(guarantee: IdempotencyGuarantee): Explanation {
  switch (guarantee.kind) {
    case "deduplicated_by":
      return {
        label: "keyed commit",
        tone: "success",
        summary:
          "Commits are deduplicated by the key: at most one commit per key value. A retry that " +
          "re-encounters this transaction recovers the prior commit — and the results and effect " +
          "intents it established — instead of executing again.",
      };
    case "not_deduplicated":
      return {
        label: "no keyed commit",
        tone: "warning",
        summary:
          "Every attempt commits. A retry is safe only if the body is naturally replayable — " +
          "re-execution reproduces the same logical state — which is not established for writes " +
          "that depend on reads, for inserts and deletes, or for state transitions.",
      };
    case "unspecified":
      return {
        label: "commit deduplication unspecified",
        tone: "warning",
        summary:
          "No fact says whether a retried attempt commits again or recovers the first commit, so " +
          "nothing about replay can be proven through this transaction.",
      };
  }
}

/** Whether an artifact (result, effect intent) established by a transaction
 *  survives for a retry to find. */
export function artifactRetention(guarantee: IdempotencyGuarantee): Explanation {
  switch (guarantee.kind) {
    case "deduplicated_by":
      return {
        label: "durably retained",
        tone: "success",
        summary:
          "Established inside a keyed commit, so it is retained with that commit and recovered " +
          "exactly by any retry that re-encounters the transaction.",
      };
    case "not_deduplicated":
      return {
        label: "not retained across attempts",
        tone: "warning",
        summary:
          "Established by a transaction without a keyed commit, so a retry cannot recover it; it " +
          "can only be reconstructed by re-executing the transaction, which requires natural " +
          "replayability and a replay-deterministic derivation.",
      };
    case "unspecified":
      return {
        label: "retention unknown",
        tone: "warning",
        summary:
          "The establishing transaction declares no commit deduplication fact, so whether a " +
          "retry recovers or reconstructs this artifact is unknown.",
      };
  }
}

export function isolation(level: "unspecified" | "read_committed" | "snapshot" | "serializable"): Explanation {
  switch (level) {
    case "read_committed":
      return {
        label: "read committed",
        tone: "neutral",
        summary:
          "Reads see only committed data, but a value may change between two reads, and " +
          "read-then-write races are possible unless locks or serialization prevent them.",
      };
    case "snapshot":
      return {
        label: "snapshot isolation",
        tone: "neutral",
        summary:
          "All ordinary reads come from one committed snapshot. Write skew and predicate " +
          "anomalies remain possible.",
      };
    case "serializable":
      return {
        label: "serializable",
        tone: "neutral",
        summary:
          "Committed transactions are equivalent to some serial order. This does not imply " +
          "real-time precedence, nor that a retry is safe.",
      };
    case "unspecified":
      return { label: "isolation unspecified", tone: "warning", summary: "No isolation fact may be assumed." };
  }
}

/** A request input's identity declaration. */
export function requestIdentity(identity: Extract<Input, { kind: "request" }>["identity"]): Explanation {
  if (identity.kind === "keyed") {
    const fields = identity.fields.map(pathText).join(", ");
    return {
      label: `keyed identity: ${fields}`,
      tone: "success",
      summary:
        `Two requests with equal ${fields} present equal payloads — the boundary rejects a retry ` +
        "whose payload disagrees with the original. This fixes what one logical request is; it " +
        "deduplicates nothing by itself.",
    };
  }
  return {
    label: "identity unspecified",
    tone: "warning",
    summary:
      "Distinct attempts may present different payloads under equal field values, so nothing " +
      "pins down which requests are retries of the same logical request.",
  };
}

export function delivery(semantics: Extract<Input, { kind: "subscription" }>["delivery"]): Explanation {
  switch (semantics) {
    case "at_least_once":
      return {
        label: "at-least-once delivery",
        tone: "info",
        summary:
          "A published message may be delivered more than once, so duplicate invocations must be " +
          "expected. Redelivery is also what re-drives an interrupted invocation.",
      };
    case "at_most_once":
      return {
        label: "at-most-once delivery",
        tone: "info",
        summary: "A message is never delivered twice, but it may be lost.",
      };
    case "unspecified":
      return {
        label: "delivery unspecified",
        tone: "warning",
        summary: "Neither duplicate delivery nor loss can be excluded.",
      };
  }
}

export function routing(value: Extract<Input, { kind: "subscription" }>["dispatch"]["routing"]): Explanation {
  switch (value) {
    case "by_topic_key":
      return {
        label: "same-key deliveries share a lane",
        tone: "info",
        summary:
          "Deliveries sharing the topic's key enter one logical lane, in delivery order. With a " +
          "keyed topic this keeps same-key invocations together; the lane's concurrency decides " +
          "whether they can overlap.",
      };
    case "single_lane":
      return {
        label: "every delivery in one lane",
        tone: "info",
        summary: "All deliveries of this subscription enter one logical lane, in delivery order.",
      };
    case "unconstrained":
      return {
        label: "no lane affinity",
        tone: "warning",
        summary: "Related deliveries may be dispatched to different lanes, in any order.",
      };
    case "unspecified":
      return { label: "routing unspecified", tone: "warning", summary: "No lane-affinity fact is available." };
  }
}

export function laneConcurrency(value: Concurrency): Explanation {
  switch (value.kind) {
    case "bounded":
      return value.value === 1
        ? {
            label: "one invocation at a time per lane",
            tone: "success",
            summary: "Invocations in one lane never overlap, so a later delivery cannot overtake an earlier one.",
          }
        : {
            label: `up to ${value.value} at a time per lane`,
            tone: "warning",
            summary: "Invocations in one lane may overlap, so a later delivery may overtake an earlier one.",
          };
    case "unbounded":
      return { label: "unbounded lane concurrency", tone: "warning", summary: "Invocations in one lane may overlap without limit." };
    case "unspecified":
      return { label: "lane concurrency unspecified", tone: "warning", summary: "No per-lane concurrency fact is available." };
  }
}

export function operationConcurrency(value: Concurrency): Explanation {
  switch (value.kind) {
    case "bounded":
      return value.value === 1
        ? { label: "one invocation at a time", tone: "success", summary: "At most one invocation of the operation is active at any moment, whatever triggered it." }
        : { label: `up to ${value.value} concurrent invocations`, tone: "neutral", summary: "Invocations may overlap, up to the bound." };
    case "unbounded":
      return { label: "unbounded concurrency", tone: "neutral", summary: "No global limit on simultaneously active invocations." };
    case "unspecified":
      return { label: "concurrency unspecified", tone: "warning", summary: "No global concurrency fact is available." };
  }
}

export function messageIdentity(identity: Topic["message_identity"]): Explanation {
  if (identity.kind === "keyed") {
    return {
      label: "keyed message identity",
      tone: "success",
      summary:
        "One logical message is identified by the mapped fields of its schema: publications with " +
        "equal identity are the same message, however many times it is published.",
    };
  }
  return {
    label: "message identity unspecified",
    tone: "warning",
    summary: "Nothing identifies a logical message, so two publications are two messages even with equal payloads.",
  };
}

export function topicOrdering(ordering: Topic["ordering"]): Explanation {
  switch (ordering.kind) {
    case "keyed":
      return { label: "ordered per key", tone: "success", summary: "Messages sharing the mapped key are delivered in publication order; different keys are unordered relative to each other." };
    case "global":
      return { label: "globally ordered", tone: "success", summary: "Every message is part of one ordered sequence." };
    case "unordered":
      return { label: "unordered", tone: "warning", summary: "No delivery-order guarantee; observed order may not be relied on." };
    case "unspecified":
      return { label: "ordering unspecified", tone: "warning", summary: "No usable ordering fact is declared." };
  }
}

export function externalIdempotency(guarantee: IdempotencyGuarantee): Explanation {
  switch (guarantee.kind) {
    case "deduplicated_by":
      return { label: "deduplicated by the external system", tone: "success", summary: "The boundary performs at most one execution per key value; a duplicate execution with the same key is absorbed there." };
    case "not_deduplicated":
      return { label: "not deduplicated", tone: "warning", summary: "The external system performs every execution it receives: a duplicate execution is duplicate work." };
    case "unspecified":
      return { label: "deduplication unspecified", tone: "warning", summary: "No fact says whether the external system absorbs duplicate executions." };
  }
}

/** A request input's declared `Result<Ok, Err>` contract. */
export function requestResult(): Explanation {
  return {
    label: "returns Result<ok, err>",
    tone: "info",
    summary:
      "A request through this input completes with exactly one of two typed outcomes: an ok " +
      "payload or an err payload. Err is a logical outcome the boundary returned — a declined " +
      "card, a rejected request — not a crash, a timeout, or a lost connection.",
  };
}

/** What an external boundary's result says, and what it does not. */
export function externalResult(result: ResultType | null): Explanation {
  if (result) {
    return {
      label: "returns a result",
      tone: "info",
      summary:
        "The boundary returns Result<ok, err>, and the program may branch on it. No declared fact " +
        "says a repeated execution returns the same outcome, so a decision on this result is not " +
        "established to replay.",
    };
  }
  return {
    label: "no synchronous result",
    tone: "neutral",
    summary: "The boundary returns nothing the program can observe; executing it binds no result.",
  };
}

/** A request effect's result, inherited from the input it targets. */
export function inheritedResult(): Explanation {
  return {
    label: "inherits the target's result",
    tone: "info",
    summary:
      "The request yields the Result<ok, err> its target input declares. Repeated payload-equal " +
      "requests observe the same outcome exactly when the target proves its result " +
      "replay-consistent for that input.",
  };
}

/** A transaction output: data a transaction exports into the program. */
export function transactionOutput(): Explanation {
  return {
    label: "exported by a transaction",
    tone: "info",
    summary:
      "A typed value the transaction establishes atomically with its commit and exposes to the " +
      "steps that follow. It is data, not work: an effect intent is the artifact for that. A " +
      "transaction read never leaves its transaction; this is the only way an observation does.",
  };
}

/** A result binding: an operation-local observation of an effect's outcome. */
export function resultBinding(): Explanation {
  return {
    label: "operation-local observation",
    tone: "neutral",
    summary:
      "The bound result is available to the steps after the binding; its ok payload only inside " +
      "the ok arm of a match on it, its err payload only inside the err arm. It is not a " +
      "transaction artifact and is not durable: a retry re-executes the effect and observes afresh.",
  };
}
