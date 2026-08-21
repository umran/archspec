import { Badge } from "@cloudflare/kumo/components/badge";
import { Button } from "@cloudflare/kumo/components/button";
import { Collapsible } from "@cloudflare/kumo/components/collapsible";
import { Empty } from "@cloudflare/kumo/components/empty";
import { Text } from "@cloudflare/kumo/components/text";
import { ArrowDownIcon, ArrowSquareOutIcon, CaretRightIcon, GraphIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";

import { pathText, shortId } from "../lib/ids";
import { effectDef, effectSummary } from "../lib/index";
import { STATUS_GLYPH, propertyMatchesRequirement, worstStatus } from "../lib/obligations";
import { hashes } from "../lib/route";
import { concurrencyText, predicateText } from "../lib/text";
import { useApp, type DetailContext } from "../state/AppState";
import { IdLink, Mono, StatusChips } from "../panels/parts";
import type { Effect, Id, InvocationFlow, Operation, RequirementKind, TransactionStep, TransitionSideEffect } from "../types/model";

type EffectKind = (Effect | TransitionSideEffect)["kind"];

const EFFECT_BADGE: Record<EffectKind, { variant: "purple" | "orange" | "warning"; label: string }> = {
  publication: { variant: "purple", label: "publication" },
  request: { variant: "orange", label: "request" },
  external: { variant: "warning", label: "external" },
};

const STEP_STRIPE: Record<string, string> = {
  tx: "var(--arch-edge-request)",
  effect: "var(--arch-edge-publish)",
  intent: "var(--arch-edge-publish)",
  response: "var(--arch-edge-client)",
};

function EffectKindBadge({ kind }: { kind: EffectKind | null }) {
  if (!kind) return <Badge variant="neutral">unresolved</Badge>;
  const { variant, label } = EFFECT_BADGE[kind];
  return <Badge variant={variant}>{label}</Badge>;
}

/** A selectable card in a flow column. */
function StepCard({
  selKey, detailId, ctx, stripe, dashed, children,
}: { selKey: string; detailId: string; ctx?: DetailContext; stripe: string; dashed?: boolean; children: ReactNode }) {
  const { selection, select } = useApp();
  const selected = selection === selKey;
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => select(selKey, { id: detailId, ctx: ctx ?? {} })}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          select(selKey, { id: detailId, ctx: ctx ?? {} });
        }
      }}
      className={`cursor-pointer rounded-lg border border-kumo-hairline bg-kumo-base p-3 text-left shadow-sm transition-shadow hover:shadow-md ${selected ? "ring-2 ring-kumo-brand" : ""}`}
      style={{ borderLeft: `3px ${dashed ? "dashed" : "solid"} ${stripe}` }}
    >
      {children}
    </div>
  );
}

function Connector() {
  return (
    <div className="flex flex-col items-center py-0.5 text-kumo-inactive">
      <span className="h-3 w-px bg-kumo-line" />
      <ArrowDownIcon size={12} />
    </div>
  );
}

function TxStepRow({ step, index, txId, opId }: { step: TransactionStep; index: number; txId: Id; opId: Id }) {
  const { selection, select, navigateTo } = useApp();
  const selKey = `ts:${txId}:${index}`;
  const selected = selection === selKey;

  let kind: string;
  let title: string;
  let note: string;
  switch (step.kind) {
    case "read":
      kind = "read"; title = shortId(step.result);
      note = `${shortId(step.target.object)} where ${predicateText(step.target.predicate)}`;
      break;
    case "write":
      kind = "write"; title = shortId(step.target.object);
      note = `${step.fields.map(pathText).join(", ")} · ${step.values.kind}`;
      break;
    case "insert":
      kind = "insert"; title = shortId(step.object); note = `values: ${step.values.kind}`;
      break;
    case "delete":
      kind = "delete"; title = shortId(step.target.object); note = `where ${predicateText(step.target.predicate)}`;
      break;
    case "lock":
      kind = "lock"; title = shortId(step.target.object); note = `${step.mode} · order ${step.order.kind}`;
      break;
    case "transition":
      kind = "transition"; title = shortId(step.transition); note = shortId(step.machine);
      break;
    case "establish_effect_intent":
      kind = "establish intent"; title = shortId(step.intent); note = `values: ${step.values.kind}`;
      break;
    case "establish_invocation_result":
      kind = "establish result"; title = shortId(step.result); note = `values: ${step.values.kind}`;
      break;
  }

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={(e) => {
        e.stopPropagation();
        select(selKey, { id: txId, ctx: { txStep: { op: opId, tx: txId, index } } });
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          e.stopPropagation();
          select(selKey, { id: txId, ctx: { txStep: { op: opId, tx: txId, index } } });
        }
      }}
      className={`flex cursor-pointer items-start gap-2 rounded-md px-2 py-1.5 hover:bg-kumo-tint ${selected ? "bg-kumo-tint ring-1 ring-kumo-brand" : ""}`}
    >
      <Badge variant="neutral">{index + 1}</Badge>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="text-[11px] uppercase tracking-wider text-kumo-subtle">{kind}</span>
          <Mono className="text-kumo-strong">{title}</Mono>
        </div>
        <div className="truncate text-xs text-kumo-subtle">{note}</div>
      </div>
      {step.kind === "transition" && (
        <Button
          variant="ghost"
          size="xs"
          shape="square"
          icon={ArrowSquareOutIcon}
          aria-label="Open state machine"
          onClick={(e) => {
            e.stopPropagation();
            navigateTo(hashes.machine(step.machine, step.transition), `t:${step.transition}`);
          }}
        />
      )}
    </div>
  );
}

function FlowColumn({ opId, op, flowId, flow }: { opId: Id; op: Operation; flowId: Id; flow: InvocationFlow }) {
  const { model, index, expandedTx, toggleTx, selection, select } = useApp();
  const effectKind = (effectId: Id): EffectKind | null => effectDef(model, index, effectId)?.effect.kind ?? null;
  const viaTransition = (effectId: Id) => {
    const owner = index.get(effectId);
    return !!owner && owner.kind === "effect" && owner.machine !== undefined;
  };

  const cards: ReactNode[] = [];
  flow.steps.forEach((step, si) => {
    if (step.kind === "transaction") {
      const tx = op.transactions[step.transaction];
      const expandKey = `${flowId}/${si}`;
      const expanded = expandedTx.has(expandKey);
      cards.push(
        <StepCard key={si} selKey={`tx:${step.transaction}`} detailId={step.transaction} stripe={STEP_STRIPE.tx}>
          <div className="flex items-center justify-between gap-2">
            <Badge variant="neutral">transaction</Badge>
            <StatusChips obKey={`${opId}/${step.transaction}`} />
          </div>
          <div className="mt-1.5 font-mono text-[13px] font-semibold text-kumo-strong">{shortId(step.transaction)}</div>
          {tx ? (
            <>
              <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-kumo-subtle">
                <span>{tx.isolation}</span>
                <span>·</span>
                <Badge variant={tx.idempotency.kind === "deduplicated_by" ? "success" : "warning"}>{tx.idempotency.kind}</Badge>
                {tx.data_model && <span>· {shortId(tx.data_model)}</span>}
              </div>
              <Collapsible.Root open={expanded} onOpenChange={() => toggleTx(expandKey)}>
                <Collapsible.Trigger
                  className="mt-2 flex w-full cursor-pointer items-center gap-1 text-xs text-kumo-link hover:underline"
                  onClick={(e) => e.stopPropagation()}
                >
                  <CaretRightIcon size={12} className={`transition-transform ${expanded ? "rotate-90" : ""}`} />
                  {tx.steps.length} step{tx.steps.length === 1 ? "" : "s"}
                </Collapsible.Trigger>
                <Collapsible.Panel>
                  <div className="mt-1.5 space-y-0.5 rounded-md border border-kumo-hairline bg-kumo-elevated/40 p-1">
                    {tx.steps.map((ts, ti) => (
                      <TxStepRow key={ti} step={ts} index={ti} txId={step.transaction} opId={opId} />
                    ))}
                  </div>
                </Collapsible.Panel>
              </Collapsible.Root>
            </>
          ) : (
            <div className="mt-1 text-xs text-kumo-danger">unresolved transaction</div>
          )}
        </StepCard>,
      );
    } else if (step.kind === "execute_effect") {
      cards.push(
        <StepCard key={si} selKey={`fx:${si}:${step.effect}`} detailId={step.effect} stripe={STEP_STRIPE.effect}>
          <div className="flex flex-wrap items-center gap-1.5">
            <Badge variant="neutral">execute effect</Badge>
            <EffectKindBadge kind={effectKind(step.effect)} />
          </div>
          <div className="mt-1.5 font-mono text-[13px] font-semibold text-kumo-strong">{shortId(step.effect)}</div>
          <div className="mt-1 text-xs text-kumo-subtle">{effectSummary(model, index, step.effect)}</div>
          <div className="mt-1 text-xs text-kumo-subtle">
            instance: <Badge variant={step.values.kind === "deterministic" ? "info" : "warning"}>{step.values.kind}</Badge>
          </div>
        </StepCard>,
      );
    } else {
      const intent = op.effect_intents[step.intent];
      const eff = intent?.effect;
      cards.push(
        <StepCard key={si} selKey={`fi:${si}:${step.intent}`} detailId={step.intent} stripe={STEP_STRIPE.intent} dashed>
          <div className="flex flex-wrap items-center gap-1.5">
            <Badge variant="neutral">execute intent</Badge>
            <EffectKindBadge kind={eff ? effectKind(eff) : null} />
            {eff && viaTransition(eff) && <Badge variant="info">via transition</Badge>}
          </div>
          <div className="mt-1.5 font-mono text-[13px] font-semibold text-kumo-strong">{shortId(step.intent)}</div>
          <div className="mt-1 text-xs text-kumo-subtle">{eff ? effectSummary(model, index, eff) : "unresolved intent"}</div>
        </StepCard>,
      );
    }
  });

  if (flow.response) {
    const resp = op.responses[flow.response];
    cards.push(
      <StepCard key="response" selKey={`resp:${flow.response}`} detailId={flow.response} stripe={STEP_STRIPE.response}>
        <Badge variant="neutral">terminal response</Badge>
        <div className="mt-1.5 font-mono text-[13px] font-semibold text-kumo-strong">{shortId(flow.response)}</div>
        {resp && (
          <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-kumo-subtle">
            <IdLink id={resp.schema}>{shortId(resp.schema)}</IdLink>
            <span>· source</span>
            <Badge variant={resp.source.kind === "invocation_result" ? "info" : "warning"}>{resp.source.kind}</Badge>
          </div>
        )}
      </StepCard>,
    );
  }

  const flowKey = `flow:${flowId}`;
  return (
    <div className="w-[340px] shrink-0">
      <div
        role="button"
        tabIndex={0}
        onClick={() => select(flowKey, { id: flowId, ctx: {} })}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") select(flowKey, { id: flowId, ctx: {} });
        }}
        className={`mb-3 flex cursor-pointer items-center justify-between gap-2 rounded-md px-2 py-1.5 hover:bg-kumo-tint ${selection === flowKey ? "bg-kumo-tint ring-1 ring-kumo-brand" : ""}`}
      >
        <span className="flex items-center gap-2">
          <span className="text-[11px] font-semibold uppercase tracking-wider text-kumo-subtle">flow</span>
          <span className="font-mono text-[13px] font-semibold text-kumo-strong">{shortId(flowId)}</span>
        </span>
        <StatusChips obKey={`${opId}/${flowId}`} />
      </div>
      {cards.length ? (
        cards.map((card, i) => (
          <div key={i}>
            {i > 0 && <Connector />}
            {card}
          </div>
        ))
      ) : (
        <div className="text-sm text-kumo-subtle">flow declares no steps</div>
      )}
    </div>
  );
}

export function OperationView({ id }: { id: string }) {
  const { model, obligations, overlay, selection, select, navigateTo } = useApp();
  const op = model.operations[id];

  if (!op) {
    return (
      <div className="flex h-full items-center justify-center">
        <Empty size="sm" icon={<GraphIcon size={32} className="text-kumo-inactive" />} title={`unknown operation ${id}`} />
      </div>
    );
  }

  const chips: { prop: RequirementKind; i: number; label: string }[] = [];
  const reqs = op.requirements;
  reqs.serialization.forEach((_, i) => chips.push({ prop: "serialization", i, label: `serialization #${i}` }));
  reqs.ordering.forEach((_, i) => chips.push({ prop: "ordering", i, label: `ordering #${i}` }));
  reqs.idempotency.forEach((r, i) =>
    chips.push({ prop: "idempotency", i, label: `idempotency #${i}${r.response === "replay_consistent" ? " · replay-consistent" : ""}` }));
  reqs.recoverability.forEach((r, i) => chips.push({ prop: "recoverability", i, label: `recoverability #${i} · ${r.completion}` }));

  const inputs = Object.entries(op.inputs);
  const flows = Object.entries(op.flows);
  const node = { machines: Object.values(op.transactions).flatMap((tx) => tx.steps.flatMap((s) => (s.kind === "transition" ? [s.machine] : []))) };

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-[1400px] space-y-7 p-6">
        <header className="space-y-2">
          <div className="flex flex-wrap items-baseline gap-3">
            <Text variant="heading" size="lg" as="h1">{shortId(id)}</Text>
            <Mono className="text-kumo-subtle">{id}</Mono>
          </div>
          <div className="flex flex-wrap items-center gap-2 text-sm text-kumo-subtle">
            <span>operation on</span>
            <IdLink id={op.service} />
            <span>·</span>
            <span>concurrency</span>
            <Badge variant="neutral">{concurrencyText(op.execution.concurrency)}</Badge>
            {[...new Set(node.machines)].map((m) => (
              <Button key={m} variant="ghost" size="xs" icon={ArrowSquareOutIcon} onClick={() => navigateTo(hashes.machine(m))}>
                {shortId(m)}
              </Button>
            ))}
          </div>
          {op.description && <p className="max-w-3xl text-sm leading-relaxed text-kumo-default">{op.description}</p>}
        </header>

        {chips.length > 0 && (
          <section className="space-y-2">
            <div className="text-[11px] font-semibold uppercase tracking-wider text-kumo-subtle">requirements</div>
            <div className="flex flex-wrap gap-2">
              {chips.map((chip) => {
                const obs = overlay
                  ? (obligations.get(id) ?? []).filter(
                      (ob) => ob.subject.kind === "operation" && ob.subject.requirement === chip.i &&
                        propertyMatchesRequirement(ob.property, chip.prop))
                  : [];
                const status = obs.length ? worstStatus(obs) : null;
                const key = `req:${chip.prop}:${chip.i}`;
                const variant = status === "proven" ? "success" : status === "disproven" ? "error" : status === "unknown" ? "warning" : "outline";
                return (
                  <button
                    key={key}
                    type="button"
                    onClick={() => select(key, { id, ctx: { req: { prop: chip.prop, index: chip.i } } })}
                    className={`cursor-pointer rounded-full ${selection === key ? "ring-2 ring-kumo-brand" : ""}`}
                  >
                    <Badge variant={variant}>{`${chip.label}${status ? ` ${STATUS_GLYPH[status]}` : ""}`}</Badge>
                  </button>
                );
              })}
            </div>
          </section>
        )}

        {inputs.length > 0 && (
          <section className="space-y-2">
            <div className="text-[11px] font-semibold uppercase tracking-wider text-kumo-subtle">inputs</div>
            <div className="flex flex-wrap gap-3">
              {inputs.map(([inputId, input]) => (
                <div key={inputId} className="w-[340px]">
                  <StepCard selKey={`in:${inputId}`} detailId={inputId} stripe={input.kind === "request" ? STEP_STRIPE.response : "var(--arch-edge-subscribe)"}>
                    <div className="flex flex-wrap items-center gap-1.5">
                      <Badge variant={input.kind === "request" ? "info" : "blue"}>{input.kind}</Badge>
                      {input.kind === "subscription" && (
                        <>
                          <Badge variant="neutral">{input.delivery}</Badge>
                          <Badge variant="neutral">{input.dispatch.routing}</Badge>
                        </>
                      )}
                      {input.kind === "request" && input.identity.kind === "keyed" && <Badge variant="success">identity keyed</Badge>}
                    </div>
                    <div className="mt-1.5 font-mono text-[13px] font-semibold text-kumo-strong">{shortId(inputId)}</div>
                    <div className="mt-1 text-xs text-kumo-subtle">
                      {input.kind === "request" ? `schema ${shortId(input.schema)}` : `⇦ ${shortId(input.topic)} · lane ${concurrencyText(input.dispatch.lane_concurrency)}`}
                    </div>
                  </StepCard>
                </div>
              ))}
            </div>
          </section>
        )}

        <section className="space-y-2">
          <div className="text-[11px] font-semibold uppercase tracking-wider text-kumo-subtle">flows</div>
          {flows.length ? (
            <div className="flex gap-8 overflow-x-auto pb-4">
              {flows.map(([flowId, flow]) => (
                <FlowColumn key={flowId} opId={id} op={op} flowId={flowId} flow={flow} />
              ))}
            </div>
          ) : (
            <Empty size="sm" title="operation declares no flows" />
          )}
        </section>
      </div>
    </div>
  );
}
