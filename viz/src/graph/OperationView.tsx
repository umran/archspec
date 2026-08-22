import { Badge } from "@cloudflare/kumo/components/badge";
import { Button } from "@cloudflare/kumo/components/button";
import { ClipboardText } from "@cloudflare/kumo/components/clipboard-text";
import { Collapsible } from "@cloudflare/kumo/components/collapsible";
import { Empty } from "@cloudflare/kumo/components/empty";
import { Flow } from "@cloudflare/kumo/components/flow";
import { Table } from "@cloudflare/kumo/components/table";
import { Tabs } from "@cloudflare/kumo/components/tabs";
import { Text } from "@cloudflare/kumo/components/text";
import { ArrowSquareOutIcon, CaretRightIcon, GraphIcon } from "@phosphor-icons/react";
import type { CSSProperties, ComponentPropsWithRef, ReactElement, ReactNode } from "react";

import { pathText, shortId } from "../lib/ids";
import { effectDef, effectSummary } from "../lib/index";
import { propertyMatchesRequirement, worstStatus } from "../lib/obligations";
import { hashes } from "../lib/route";
import { concurrencyText, predicateText } from "../lib/text";
import { useApp, type DetailContext } from "../state/AppState";
import { Fact, IdLink, KeyComponents, Mono, Muted, RefText, SectionCard, StatusBadge, StatusChips, selectableRow } from "../panels/parts";
import type { Effect, Id, InvocationFlow, Operation, RequirementKind, TransactionStep, TransitionSideEffect } from "../types/model";

type EffectKind = (Effect | TransitionSideEffect)["kind"];

const EFFECT_BADGE: Record<EffectKind, { variant: "purple" | "orange" | "warning"; label: string }> = {
  publication: { variant: "purple", label: "publication" },
  request: { variant: "orange", label: "request" },
  external: { variant: "warning", label: "external" },
};

/** Left-edge stripes reuse the system graph's edge colours, so a step's
 *  kind reads the same way here as its edge does there. */
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

// ---------------------------------------------------------------------------
// Flow steps
// ---------------------------------------------------------------------------

type StepCardProps = Omit<ComponentPropsWithRef<"div">, "children"> & {
  selKey: string;
  detailId: string;
  ctx?: DetailContext;
  stripe: string;
  dashed?: boolean;
  children: ReactNode;
};

/** A selectable step card. Rendered through `Flow.Node`'s `render` prop, so
 *  it forwards the ref, position style and data attributes Kumo's layout
 *  engine clones onto it. */
function StepCard({ selKey, detailId, ctx, stripe, dashed, children, className, style, ...rest }: StepCardProps) {
  const { selection, select } = useApp();
  const selected = selection === selKey;
  const activate = () => select(selKey, { id: detailId, ctx: ctx ?? {} });
  return (
    <div
      {...rest}
      role="button"
      tabIndex={0}
      onClick={activate}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          activate();
        }
      }}
      className={`w-(--step-w) rounded-lg border border-kumo-hairline bg-kumo-base p-3 text-left shadow-sm transition-shadow hover:shadow-md ${selected ? "ring-2 ring-kumo-brand" : ""} ${className ?? ""}`}
      // Flow.Node pins `cursor: default` inline; the card is interactive.
      style={{ ...style, cursor: "pointer", borderLeft: `3px ${dashed ? "dashed" : "solid"} ${stripe}` }}
    >
      {children}
    </div>
  );
}

function StepTitle({ children }: { children: ReactNode }) {
  return <div className="mt-1.5 font-mono text-[13px] font-semibold text-kumo-strong">{children}</div>;
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

  const activate = () => select(selKey, { id: txId, ctx: { txStep: { op: opId, tx: txId, index } } });

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={(e) => {
        e.stopPropagation();
        activate();
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          e.stopPropagation();
          activate();
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

/** One invocation flow: its selectable header strip and the step diagram.
 *  Steps are Kumo `Flow` nodes, so the connectors between them are drawn
 *  by Kumo's layout engine and follow the cards as they expand. */
function FlowBody({ opId, op, flowId, flow }: { opId: Id; op: Operation; flowId: Id; flow: InvocationFlow }) {
  const { model, index, expandedTx, toggleTx, selection, select } = useApp();
  const effectKind = (effectId: Id): EffectKind | null => effectDef(model, index, effectId)?.effect.kind ?? null;
  const viaTransition = (effectId: Id) => {
    const owner = index.get(effectId);
    return !!owner && owner.kind === "effect" && owner.machine !== undefined;
  };

  const nodes: { key: string; element: ReactElement }[] = [];
  flow.steps.forEach((step, si) => {
    const key = `${flowId}/${si}`;
    if (step.kind === "transaction") {
      const tx = op.transactions[step.transaction];
      const expandKey = `${flowId}/${si}`;
      const expanded = expandedTx.has(expandKey);
      nodes.push({
        key,
        element: (
          <StepCard selKey={`tx:${step.transaction}`} detailId={step.transaction} stripe={STEP_STRIPE.tx}>
            <div className="flex items-center justify-between gap-2">
              <Badge variant="neutral">transaction</Badge>
              <StatusChips obKey={`${opId}/${step.transaction}`} />
            </div>
            <StepTitle>{shortId(step.transaction)}</StepTitle>
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
          </StepCard>
        ),
      });
    } else if (step.kind === "execute_effect") {
      nodes.push({
        key,
        element: (
          <StepCard selKey={`fx:${si}:${step.effect}`} detailId={step.effect} stripe={STEP_STRIPE.effect}>
            <div className="flex flex-wrap items-center gap-1.5">
              <Badge variant="neutral">execute effect</Badge>
              <EffectKindBadge kind={effectKind(step.effect)} />
            </div>
            <StepTitle>{shortId(step.effect)}</StepTitle>
            <div className="mt-1 text-xs text-kumo-subtle">{effectSummary(model, index, step.effect)}</div>
            <div className="mt-1 text-xs text-kumo-subtle">
              instance: <Badge variant={step.values.kind === "deterministic" ? "info" : "warning"}>{step.values.kind}</Badge>
            </div>
          </StepCard>
        ),
      });
    } else {
      const intent = op.effect_intents[step.intent];
      const eff = intent?.effect;
      nodes.push({
        key,
        element: (
          <StepCard selKey={`fi:${si}:${step.intent}`} detailId={step.intent} stripe={STEP_STRIPE.intent} dashed>
            <div className="flex flex-wrap items-center gap-1.5">
              <Badge variant="neutral">execute intent</Badge>
              <EffectKindBadge kind={eff ? effectKind(eff) : null} />
              {eff && viaTransition(eff) && <Badge variant="info">via transition</Badge>}
            </div>
            <StepTitle>{shortId(step.intent)}</StepTitle>
            <div className="mt-1 text-xs text-kumo-subtle">{eff ? effectSummary(model, index, eff) : "unresolved intent"}</div>
          </StepCard>
        ),
      });
    }
  });

  if (flow.response) {
    const resp = op.responses[flow.response];
    nodes.push({
      key: `${flowId}/response`,
      element: (
        <StepCard selKey={`resp:${flow.response}`} detailId={flow.response} stripe={STEP_STRIPE.response}>
          <Badge variant="neutral">terminal response</Badge>
          <StepTitle>{shortId(flow.response)}</StepTitle>
          {resp && (
            <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-kumo-subtle">
              <IdLink id={resp.schema}>{shortId(resp.schema)}</IdLink>
              <span>· source</span>
              <Badge variant={resp.source.kind === "invocation_result" ? "info" : "warning"}>{resp.source.kind}</Badge>
            </div>
          )}
        </StepCard>
      ),
    });
  }

  const flowKey = `flow:${flowId}`;
  const flowSelected = selection === flowKey;
  const activateFlow = () => select(flowKey, { id: flowId, ctx: {} });
  const stepCount = flow.steps.length;

  return (
    <div className="space-y-3">
      <div
        role="button"
        tabIndex={0}
        onClick={activateFlow}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            activateFlow();
          }
        }}
        title="Flow details"
        className={`flex cursor-pointer flex-wrap items-center gap-x-3 gap-y-1 rounded-md px-2 py-1.5 hover:bg-kumo-tint ${flowSelected ? "bg-kumo-tint ring-1 ring-kumo-brand" : ""}`}
      >
        <Badge variant="outline">flow</Badge>
        <span className="font-mono text-[13px] font-semibold text-kumo-strong">{shortId(flowId)}</span>
        <span className="text-xs text-kumo-subtle">
          {stepCount} step{stepCount === 1 ? "" : "s"} · terminal response{" "}
          {flow.response ? <Mono>{shortId(flow.response)}</Mono> : "none"}
        </span>
        <span className="ml-auto flex items-center gap-2">
          <StatusChips obKey={`${opId}/${flowId}`} />
          <CaretRightIcon size={12} className="text-kumo-inactive" />
        </span>
      </div>

      {nodes.length ? (
        // Step cards size to the section body (a container), capped for
        // readability; the 12px accounts for the diagram's own padding,
        // which keeps selection rings clear of its clipping edge.
        <div className="arch-flow" style={{ "--step-w": "min(640px, 100cqw - 12px)" } as CSSProperties}>
          <Flow orientation="vertical" canvas={false} padding={{ x: 6, y: 6 }}>
            {nodes.map((n) => (
              <Flow.Node key={n.key} id={n.key} render={n.element} />
            ))}
          </Flow>
        </div>
      ) : (
        <Muted>flow declares no steps</Muted>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Requirements and inputs
// ---------------------------------------------------------------------------

function RequirementsTable({ id, op }: { id: Id; op: Operation }) {
  const { obligations, overlay, selection, select } = useApp();
  const reqs = op.requirements;

  const rows: { prop: RequirementKind; i: number; declares: ReactNode }[] = [];
  reqs.serialization.forEach((r, i) => rows.push({ prop: "serialization", i, declares: <RefText value={r.key} /> }));
  reqs.ordering.forEach((r, i) => rows.push({ prop: "ordering", i, declares: <RefText value={r.key} /> }));
  reqs.idempotency.forEach((r, i) =>
    rows.push({
      prop: "idempotency",
      i,
      declares: (
        <>
          <KeyComponents value={r.key} />
          {r.response === "replay_consistent" && <Badge variant="info">replay-consistent response</Badge>}
        </>
      ),
    }));
  reqs.recoverability.forEach((r, i) =>
    rows.push({
      prop: "recoverability",
      i,
      declares: (
        <>
          <KeyComponents value={r.key} />
          <Badge variant={r.completion === "guaranteed" ? "success" : "neutral"}>{r.completion} completion</Badge>
        </>
      ),
    }));

  if (!rows.length) return <Muted>operation declares no requirements</Muted>;

  return (
    <Table>
      <Table.Header variant="compact">
        <Table.Row>
          <Table.Head>requirement</Table.Head>
          <Table.Head>declares</Table.Head>
          <Table.Head>verdict</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {rows.map((row) => {
          const key = `req:${row.prop}:${row.i}`;
          const obs = overlay
            ? (obligations.get(id) ?? []).filter(
                (ob) => ob.subject.kind === "operation" && ob.subject.requirement === row.i &&
                  propertyMatchesRequirement(ob.property, row.prop))
            : [];
          const status = obs.length ? worstStatus(obs) : null;
          return (
            <Table.Row
              key={key}
              className={selectableRow(selection === key)}
              onClick={() => select(key, { id, ctx: { req: { prop: row.prop, index: row.i } } })}
            >
              <Table.Cell className="whitespace-nowrap">
                <span className="font-medium text-kumo-strong">{row.prop}</span>
                <span className="ml-1.5 text-kumo-inactive">#{row.i}</span>
              </Table.Cell>
              <Table.Cell>
                <span className="flex flex-wrap items-center gap-x-2 gap-y-1">{row.declares}</span>
              </Table.Cell>
              <Table.Cell className="whitespace-nowrap">
                {status ? (
                  <span className="inline-flex items-center gap-1.5">
                    <StatusBadge status={status} />
                    {obs.length > 1 && <span className="text-xs text-kumo-inactive">{obs.length} obligations</span>}
                  </span>
                ) : (
                  <span className="text-kumo-inactive">—</span>
                )}
              </Table.Cell>
            </Table.Row>
          );
        })}
      </Table.Body>
    </Table>
  );
}

function InputsTable({ op }: { op: Operation }) {
  const { selection, select } = useApp();
  const inputs = Object.entries(op.inputs);

  if (!inputs.length) return <Muted>operation declares no inputs</Muted>;

  return (
    <Table>
      <Table.Header variant="compact">
        <Table.Row>
          <Table.Head>input</Table.Head>
          <Table.Head>kind</Table.Head>
          <Table.Head>source</Table.Head>
          <Table.Head>semantics</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {inputs.map(([inputId, input]) => {
          const key = `in:${inputId}`;
          return (
            <Table.Row
              key={key}
              className={selectableRow(selection === key)}
              onClick={() => select(key, { id: inputId, ctx: {} })}
            >
              <Table.Cell><Mono className="text-kumo-strong">{shortId(inputId)}</Mono></Table.Cell>
              <Table.Cell><Badge variant={input.kind === "request" ? "info" : "blue"}>{input.kind}</Badge></Table.Cell>
              <Table.Cell className="whitespace-nowrap">
                {input.kind === "request" ? (
                  <span className="inline-flex items-center gap-1.5">
                    <span className="text-xs text-kumo-subtle">schema</span>
                    <IdLink id={input.schema}>{shortId(input.schema)}</IdLink>
                  </span>
                ) : (
                  <span className="inline-flex items-center gap-1.5">
                    <span className="text-xs text-kumo-subtle">topic</span>
                    <IdLink id={input.topic}>{shortId(input.topic)}</IdLink>
                  </span>
                )}
              </Table.Cell>
              <Table.Cell>
                <span className="flex flex-wrap items-center gap-1.5">
                  {input.kind === "request" ? (
                    input.identity.kind === "keyed" ? (
                      <Badge variant="success">identity keyed</Badge>
                    ) : (
                      <Badge variant="neutral">identity unspecified</Badge>
                    )
                  ) : (
                    <>
                      <Badge variant="neutral">{input.delivery}</Badge>
                      <Badge variant="neutral">{input.dispatch.routing}</Badge>
                      <span className="text-xs text-kumo-subtle">lane {concurrencyText(input.dispatch.lane_concurrency)}</span>
                    </>
                  )}
                </span>
              </Table.Cell>
            </Table.Row>
          );
        })}
      </Table.Body>
    </Table>
  );
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export function OperationView({ id }: { id: string }) {
  const { model, navigateTo, route, obligations } = useApp();
  const op = model.operations[id];

  if (!op) {
    return (
      <div className="flex h-full items-center justify-center">
        <Empty size="sm" icon={<GraphIcon size={32} className="text-kumo-inactive" />} title={`unknown operation ${id}`} />
      </div>
    );
  }

  const reqs = op.requirements;
  const requirementCount = reqs.serialization.length + reqs.ordering.length + reqs.idempotency.length + reqs.recoverability.length;
  const inputCount = Object.keys(op.inputs).length;
  const transactionCount = Object.keys(op.transactions).length;
  const flowIds = Object.keys(op.flows);
  const requested = route.view === "op" ? route.flow : null;
  const activeFlow = requested && op.flows[requested] ? requested : (flowIds[0] ?? null);
  const machines = [...new Set(
    Object.values(op.transactions).flatMap((tx) => tx.steps.flatMap((s) => (s.kind === "transition" ? [s.machine] : []))),
  )];

  return (
    <div className="h-full overflow-auto">
      <div className="@container mx-auto max-w-[1240px] space-y-6 p-6">
        <header className="space-y-4 border-b border-kumo-hairline pb-5">
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <Text variant="heading2" as="h1">{shortId(id)}</Text>
            <ClipboardText text={id} size="sm" tooltip={{ text: "Copy id", copiedText: "Copied" }} />
          </div>
          {op.description && <p className="max-w-3xl text-sm leading-relaxed text-kumo-default">{op.description}</p>}
          <dl className="flex flex-wrap gap-x-8 gap-y-3">
            <Fact label="service"><IdLink id={op.service}>{shortId(op.service)}</IdLink></Fact>
            <Fact label="concurrency"><Badge variant="neutral">{concurrencyText(op.execution.concurrency)}</Badge></Fact>
            <Fact label="transactions">{transactionCount}</Fact>
            <Fact label="flows">{flowIds.length}</Fact>
            {machines.length > 0 && (
              <Fact label="state machines">
                {machines.map((m) => (
                  <Button key={m} variant="ghost" size="xs" icon={ArrowSquareOutIcon} onClick={() => navigateTo(hashes.machine(m))}>
                    {shortId(m)}
                  </Button>
                ))}
              </Fact>
            )}
            {(obligations.get(id) ?? []).length > 0 && <Fact label="verdicts"><StatusChips obKey={id} /></Fact>}
          </dl>
        </header>

        {/* Sized by the pane (a container), not the window: the detail and
            obligation asides take room the viewport width doesn't reflect. */}
        <div className="grid gap-6 @5xl:grid-cols-2">
          <SectionCard title="Requirements" count={requirementCount} hint="proof obligations on every invocation" bodyClassName="p-0">
            <div className="overflow-x-auto">
              <RequirementsTable id={id} op={op} />
            </div>
          </SectionCard>
          <SectionCard title="Inputs" count={inputCount} hint="what starts an invocation" bodyClassName="p-0">
            <div className="overflow-x-auto">
              <InputsTable op={op} />
            </div>
          </SectionCard>
        </div>

        <SectionCard
          title="Flows"
          count={flowIds.length}
          hint="alternative invocation paths — an invocation takes exactly one"
          bodyClassName="@container gap-4"
        >
          {activeFlow ? (
            <>
              <Tabs
                variant="underline"
                tabs={flowIds.map((flowId) => ({
                  value: flowId,
                  label: (
                    <span className="inline-flex items-center gap-1.5">
                      {shortId(flowId)}
                      <StatusChips obKey={`${id}/${flowId}`} />
                    </span>
                  ),
                }))}
                value={activeFlow}
                onValueChange={(value) => navigateTo(hashes.op(id, value))}
              />
              <FlowBody key={activeFlow} opId={id} op={op} flowId={activeFlow} flow={op.flows[activeFlow]} />
            </>
          ) : (
            <Empty size="sm" title="operation declares no flows" />
          )}
        </SectionCard>
      </div>
    </div>
  );
}
