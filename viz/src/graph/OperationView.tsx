import { useMemo, type ReactNode } from "react";

import { shortId, pathText, truncate, wrapText } from "../lib/ids";
import { effectSummary } from "../lib/index";
import { STATUS_GLYPH, propertyMatchesRequirement, statusChipText, statusCounts, worstStatus } from "../lib/obligations";
import { hashes } from "../lib/route";
import { concurrencyText, predicateText } from "../lib/text";
import { useApp, type DetailContext } from "../state/AppState";
import type { Id, Operation, RequirementKind, TransactionStep } from "../types/model";
import { LegendLine, SvgCanvas, sel } from "./SvgCanvas";
import { StatusChip, StatusRing } from "./status";

const OPV = { COL_W: 300, COL_GAP: 48, CARD_H: 58, NEST_H: 56, GAP: 16, NEST_INDENT: 18, HEADER_GAP: 40 };

interface CardSpec {
  nested: boolean;
  cls: string;
  kind: string;
  title: string;
  note: string;
  selKey: string;
  selId: string;
  ctx?: DetailContext;
  statusKey?: string;
  expandKey?: string;
  hint?: string;
  navTo?: string;
}

function txStepCard(step: TransactionStep, index: number, txId: Id, opId: Id): CardSpec {
  const base = {
    nested: true,
    cls: "txstep",
    selKey: `ts:${txId}:${index}`,
    selId: txId,
    ctx: { txStep: { op: opId, tx: txId, index } },
  };
  switch (step.kind) {
    case "read":
      return { ...base, kind: "read", title: shortId(step.result),
        note: `${shortId(step.target.object)} where ${truncate(predicateText(step.target.predicate), 34)}` };
    case "write":
      return { ...base, kind: "write", title: shortId(step.target.object),
        note: `${step.fields.map(pathText).join(", ")} · ${step.values.kind}` };
    case "insert":
      return { ...base, kind: "insert", title: shortId(step.object), note: `values: ${step.values.kind}` };
    case "delete":
      return { ...base, kind: "delete", title: shortId(step.target.object),
        note: `where ${truncate(predicateText(step.target.predicate), 36)}` };
    case "lock":
      return { ...base, kind: "lock", title: shortId(step.target.object), note: `${step.mode} · order ${step.order.kind}` };
    case "transition":
      return { ...base, cls: "txstep transition", kind: "state transition", title: shortId(step.transition),
        note: `${shortId(step.machine)} · open machine ⇢`, navTo: hashes.machine(step.machine, step.transition) };
    case "establish_effect_intent":
      return { ...base, kind: "establish intent", title: shortId(step.intent), note: `values: ${step.values.kind}` };
    case "establish_invocation_result":
      return { ...base, kind: "establish result", title: shortId(step.result), note: `values: ${step.values.kind}` };
  }
}

function Card({ x, y, w, h, spec }: { x: number; y: number; w: number; h: number; spec: CardSpec }) {
  const { selection } = useApp();
  const selected = selection === spec.selKey;
  return (
    <g
      className={`arch-card ${spec.cls}${selected ? " selected" : ""}`}
      data-sel={sel({ key: spec.selKey, id: spec.selId, ctx: spec.ctx })}
    >
      {spec.statusKey && <StatusRing x={x} y={y} w={w} h={h} rx={8} obKey={spec.statusKey} />}
      <rect className="body" x={x} y={y} width={w} height={h} rx={8} />
      <text className="kind" x={x + 12} y={y + 15}>{spec.kind}</text>
      <text className="title" x={x + 12} y={y + 31}>{truncate(spec.title, Math.floor((w - 24) / 7.2))}</text>
      {spec.note && (
        <text className="note" x={x + 12} y={y + h - 10}>{truncate(spec.note, Math.floor((w - 24) / 6))}</text>
      )}
      {spec.hint && spec.expandKey && (
        <text className="hint" x={x + w - 12} y={y + 15} textAnchor="end" data-act={JSON.stringify({ key: spec.expandKey })}>
          {spec.hint}
        </text>
      )}
      {spec.navTo && (
        <text className="hint" x={x + w - 15} y={y + h - 11} textAnchor="middle" fontSize={12} data-nav={spec.navTo}>
          ⇢
        </text>
      )}
      {spec.statusKey && <StatusChip x={x + w - 6} y={y} obKey={spec.statusKey} />}
    </g>
  );
}

function flowCards(op: Operation, opId: Id, flowId: Id, expandedTx: ReadonlySet<string>, summary: (id: Id) => string, viaTransition: (effect: Id) => boolean): CardSpec[] {
  const flow = op.flows[flowId];
  const cards: CardSpec[] = [];
  flow.steps.forEach((step, si) => {
    const expandKey = `${flowId}/${si}`;
    if (step.kind === "transaction") {
      const tx = op.transactions[step.transaction];
      const expanded = expandedTx.has(expandKey);
      cards.push({
        nested: false, cls: "tx", kind: "transaction", title: shortId(step.transaction),
        note: tx ? `${tx.steps.length} steps · ${tx.isolation} · ${tx.idempotency.kind}` : "unresolved",
        selKey: `tx:${step.transaction}`, selId: step.transaction,
        statusKey: `${opId}/${step.transaction}`, expandKey,
        hint: tx ? (expanded ? "▾ collapse" : `▸ expand ${tx.steps.length} steps`) : undefined,
      });
      if (expanded && tx) tx.steps.forEach((ts, ti) => cards.push(txStepCard(ts, ti, step.transaction, opId)));
    } else if (step.kind === "execute_effect") {
      cards.push({
        nested: false, cls: "effect", kind: "execute effect", title: shortId(step.effect),
        note: summary(step.effect), selKey: `fx:${si}:${step.effect}`, selId: step.effect,
      });
    } else {
      const intent = op.effect_intents[step.intent];
      const eff = intent?.effect;
      cards.push({
        nested: false, cls: "intent", kind: "execute effect intent", title: shortId(step.intent),
        note: (eff ? summary(eff) : "unresolved") + (eff && viaTransition(eff) ? " · via transition" : ""),
        selKey: `fi:${si}:${step.intent}`, selId: step.intent,
      });
    }
  });
  if (flow.response) {
    const resp = op.responses[flow.response];
    cards.push({
      nested: false, cls: "response", kind: "terminal response", title: shortId(flow.response),
      note: resp ? `${shortId(resp.schema)} · source: ${resp.source.kind}` : "",
      selKey: `resp:${flow.response}`, selId: flow.response,
    });
  }
  return cards;
}

export function OperationView({ id }: { id: string }) {
  const { model, index, selection, expandedTx, obligations, overlay } = useApp();
  const op = model.operations[id];

  const legend = (
    <>
      <LegendLine color="var(--arch-edge-request)" label="transaction (click ▸ to expand)" />
      <LegendLine color="var(--arch-edge-publish)" label="effect execution" />
      <LegendLine color="var(--arch-edge-publish)" label="effect-intent execution" dashed />
      <LegendLine color="var(--arch-edge-client)" label="terminal response" />
    </>
  );

  const summary = (effectId: Id) => effectSummary(model, index, effectId);
  const viaTransition = (effectId: Id) => {
    const owner = index.get(effectId);
    return !!owner && owner.kind === "effect" && owner.machine !== undefined;
  };

  const scene = useMemo(() => {
    if (!op) return null;
    const nodes: ReactNode[] = [];
    let y = 0;

    nodes.push(<text key="h" className="arch-heading" x={0} y={y}>{shortId(id)}</text>);
    nodes.push(
      <text key="sub" className="arch-subheading" x={0} y={y + 20}>
        {`${id}  ·  service: ${op.service}  ·  concurrency: ${concurrencyText(op.execution.concurrency)}`}
      </text>,
    );
    y += 34;
    if (op.description) {
      for (const line of wrapText(op.description, 96)) {
        nodes.push(<text key={`d${y}`} className="arch-desc" x={0} y={y}>{line}</text>);
        y += 17;
      }
    }
    y += 6;

    // Requirement chips, colored by their obligation status.
    const chips: { prop: RequirementKind; i: number; label: string }[] = [];
    const reqs = op.requirements;
    reqs.serialization.forEach((_, i) => chips.push({ prop: "serialization", i, label: `serialization #${i}` }));
    reqs.ordering.forEach((_, i) => chips.push({ prop: "ordering", i, label: `ordering #${i}` }));
    reqs.idempotency.forEach((_, i) => chips.push({ prop: "idempotency", i, label: `idempotency #${i}` }));
    reqs.recoverability.forEach((r, i) =>
      chips.push({ prop: "recoverability", i, label: `recoverability #${i} (${r.completion})` }));

    let cx = 0;
    for (const chip of chips) {
      const obs = overlay
        ? (obligations.get(id) ?? []).filter(
            (ob) => ob.subject.kind === "operation" && ob.subject.requirement === chip.i &&
              propertyMatchesRequirement(ob.property, chip.prop))
        : [];
      const status = obs.length ? worstStatus(obs) : null;
      const text = chip.label + (status ? ` ${STATUS_GLYPH[status]}` : "");
      const w = text.length * 6.6 + 18;
      const key = `req:${chip.prop}:${chip.i}`;
      const color = status ? `var(--arch-${status})` : "var(--arch-node-line)";
      nodes.push(
        <g key={key} className={`arch-chip${selection === key ? " selected" : ""}`}
          data-sel={sel({ key, id, ctx: { req: { prop: chip.prop, index: chip.i } } })}>
          <rect x={cx} y={y} width={w} height={24} rx={12} fill="var(--color-kumo-base)" stroke={color} strokeWidth={selection === key ? 2 : 1.2} />
          <text x={cx + w / 2} y={y + 16} textAnchor="middle" fill={status ? color : "var(--arch-text-subtle)"}>{text}</text>
        </g>,
      );
      cx += w + 10;
    }
    if (chips.length) y += 40;

    // Inputs.
    const inputIds = Object.keys(op.inputs);
    if (inputIds.length) {
      nodes.push(<text key="inputs-h" className="arch-flow-title" x={0} y={y + 12}>inputs</text>);
      y += 20;
      let ix = 0;
      for (const inputId of inputIds) {
        const input = op.inputs[inputId];
        const note = input.kind === "request"
          ? `request · ${shortId(input.schema)}`
          : `⇦ ${shortId(input.topic)} · ${input.delivery} · ${input.dispatch.routing}`;
        nodes.push(
          <Card key={`in:${inputId}`} x={ix} y={y} w={270} h={52}
            spec={{ nested: false, cls: "effect", kind: input.kind === "request" ? "request input" : "subscription",
              title: shortId(inputId), note, selKey: `in:${inputId}`, selId: inputId }} />,
        );
        ix += 270 + 16;
      }
      y += 52 + OPV.HEADER_GAP;
    } else {
      y += OPV.HEADER_GAP / 2;
    }

    // Flows: one column each.
    let fx = 0;
    const flowTop = y;
    for (const flowId of Object.keys(op.flows)) {
      let fy = flowTop;
      const flowObs = overlay ? (obligations.get(`${id}/${flowId}`) ?? []) : [];
      nodes.push(
        <g key={`flow:${flowId}`} className="arch-card" data-sel={sel({ key: `flow:${flowId}`, id: flowId })}>
          <text className="arch-flow-title" x={fx} y={fy}>
            {`flow: ${shortId(flowId)}` + (flowObs.length ? `  ${statusChipText(statusCounts(flowObs))}` : "")}
          </text>
        </g>,
      );
      fy += 16;

      let prevBottom: number | null = null;
      const cards = flowCards(op, id, flowId, expandedTx, summary, viaTransition);
      cards.forEach((card, ci) => {
        const x = fx + (card.nested ? OPV.NEST_INDENT : 0);
        const w = OPV.COL_W - (card.nested ? OPV.NEST_INDENT : 0);
        const h = card.nested ? OPV.NEST_H : OPV.CARD_H;
        if (prevBottom !== null) {
          fy = prevBottom + (card.nested ? 8 : OPV.GAP);
          if (!card.nested) {
            nodes.push(
              <path key={`arrow:${flowId}:${ci}`} className="arch-flow-arrow"
                d={`M${fx + OPV.COL_W / 2},${prevBottom + 2} L${fx + OPV.COL_W / 2},${fy - 2}`}
                markerEnd="url(#arr-faint)" />,
            );
          }
        }
        nodes.push(<Card key={`${flowId}:${ci}:${card.selKey}`} x={x} y={fy} w={w} h={h} spec={card} />);
        prevBottom = fy + h;
      });
      fx += OPV.COL_W + OPV.COL_GAP;
    }
    return nodes;
  }, [op, id, index, selection, expandedTx, obligations, overlay, model]);

  if (!op) return <SvgCanvas legend={legend} empty={`unknown operation ${id}`}>{null}</SvgCanvas>;

  return (
    <SvgCanvas legend={legend} empty={Object.keys(op.flows).length ? null : "operation declares no flows"}>
      {scene}
    </SvgCanvas>
  );
}
