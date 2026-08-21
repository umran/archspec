import { useMemo } from "react";

import { shortId, truncate } from "../lib/ids";
import { hashes } from "../lib/route";
import { useApp } from "../state/AppState";
import type { Edge } from "../types/graph";
import { layoutSystem } from "./layoutSystem";
import { LegendChip, LegendLine, SvgCanvas, sel } from "./SvgCanvas";
import { StatusChip, StatusRing } from "./status";

function edgeShortLabel(e: Edge): string {
  switch (e.kind) {
    case "publish":
    case "request":
    case "client":
      return shortId(e.schema);
    case "subscribe":
      return e.schemas.map(shortId).join(", ");
    case "external":
      return "external";
  }
}

export function SystemView() {
  const { graph, report, overlay, selection, search } = useApp();
  const layout = useMemo(() => layoutSystem(graph), [graph]);

  const q = search.trim().toLowerCase();
  const matches = (id: string) =>
    !q || id.toLowerCase().includes(q) || shortId(id).toLowerCase().includes(q);

  const related = useMemo(() => {
    const set = new Set<string>();
    if (!selection) return set;
    set.add(selection);
    for (const e of graph.edges) {
      if (e.id === selection || e.from === selection || e.to === selection) {
        set.add(e.id);
        set.add(e.from);
        set.add(e.to);
      }
    }
    return set;
  }, [graph, selection]);

  const isDim = (key: string) => (q && !matches(key)) || (!!selection && !related.has(key));

  const legend = (
    <>
      <LegendLine color="var(--arch-edge-publish)" label="publication" />
      <LegendLine color="var(--arch-edge-subscribe)" label="subscription" />
      {graph.edges.some((e) => e.kind === "request") && (
        <LegendLine color="var(--arch-edge-request)" label="request" />
      )}
      {graph.externals.length > 0 && <LegendLine color="var(--arch-edge-external)" label="external effect" />}
      {graph.client && <LegendLine color="var(--arch-edge-client)" label="client request" />}
      <LegendLine color="var(--arch-text-subtle)" label="declared, unexecuted" dashed />
      {report && overlay && (
        <>
          <LegendChip color="var(--arch-proven)" label="proven" />
          <LegendChip color="var(--arch-disproven)" label="disproven" />
          <LegendChip color="var(--arch-unknown)" label="unknown" />
        </>
      )}
    </>
  );

  const empty =
    !graph.operations.length && !graph.topics.length ? "model declares no operations or topics" : null;

  return (
    <SvgCanvas legend={legend} empty={empty}>
      {layout.services.map((box) => {
        const svc = graph.services.find((s) => s.id === box.id);
        return (
          <g key={box.id} className={`arch-service${isDim(box.id) ? " dimmed" : ""}`} data-sel={sel({ key: box.id, id: box.id })}>
            <rect className="box" x={box.x} y={box.y} width={box.w} height={box.h} rx={10} />
            <text className="label" x={box.x + 12} y={box.y + 21}>
              {truncate(shortId(box.id), 22)}
            </text>
            <text className="kind" x={box.x + box.w - 12} y={box.y + 21} textAnchor="end">
              {svc?.kind ?? ""}
            </text>
            <title>{box.id + (svc ? ` (${svc.kind})` : "")}</title>
          </g>
        );
      })}

      {layout.edges.map(({ edge: e, d, from, to }) => {
        const unexecuted = "executed_by" in e && e.executed_by.length === 0;
        const dimmed = selection ? !related.has(e.id) : q ? !(matches(e.from) || matches(e.to)) : false;
        const classes = ["arch-edge", e.kind];
        if (unexecuted) classes.push("unexecuted");
        if (dimmed) classes.push("dimmed");
        if (selection === e.id) classes.push("selected");
        return (
          <g key={e.id} data-sel={sel({ key: e.id, id: e.id, ctx: { edge: true } })}>
            <path className={classes.join(" ")} d={d} markerEnd={`url(#arr-${e.kind})`} />
            <path className="arch-edge-hit" d={d} />
            {selection === e.id && (
              <text className="arch-edge-label" x={(from.x + to.x) / 2 + 8} y={(from.y + to.y) / 2 - 6}>
                {edgeShortLabel(e)}
              </text>
            )}
          </g>
        );
      })}

      {graph.operations.map((op) => {
        const p = layout.pos.get(op.id);
        if (!p) return null;
        const r = op.requirements;
        const badges: string[] = [];
        if (r.serialization) badges.push(`S${r.serialization}`);
        if (r.ordering) badges.push(`O${r.ordering}`);
        if (r.idempotency) badges.push(`I${r.idempotency}`);
        if (r.recoverability) badges.push(`R${r.recoverability}`);
        if (op.machines.length) badges.push("SM");
        const classes = ["arch-node", "operation"];
        if (isDim(op.id)) classes.push("dimmed");
        if (selection === op.id) classes.push("selected");
        return (
          <g key={op.id} className={classes.join(" ")} data-sel={sel({ key: op.id, id: op.id })} data-dbl={hashes.op(op.id)}>
            <StatusRing x={p.x} y={p.y} w={p.w} h={p.h} rx={8} obKey={op.id} />
            <rect className="body" x={p.x} y={p.y} width={p.w} height={p.h} rx={8} />
            <text className="title" x={p.x + 10} y={p.y + 20}>
              {truncate(shortId(op.id), 24)}
            </text>
            <text className="subtitle" x={p.x + 10} y={p.y + 36}>
              {`${op.flows} flow${op.flows === 1 ? "" : "s"} · ${op.inputs} input${op.inputs === 1 ? "" : "s"}`}
            </text>
            <text className="badge-text" x={p.x + 10} y={p.y + 52}>
              {badges.join("  ")}
            </text>
            <title>{op.id + (op.description ? `\n${op.description}` : "") + "\n(double-click to open flows)"}</title>
            <StatusChip x={p.x + p.w - 6} y={p.y} obKey={op.id} />
          </g>
        );
      })}

      {graph.topics.map((t) => {
        const p = layout.pos.get(t.id);
        if (!p) return null;
        const classes = ["arch-node", "topic"];
        if (isDim(t.id)) classes.push("dimmed");
        if (selection === t.id) classes.push("selected");
        return (
          <g key={t.id} className={classes.join(" ")} data-sel={sel({ key: t.id, id: t.id })}>
            <StatusRing x={p.x} y={p.y} w={p.w} h={p.h} rx={24} obKey={t.id} />
            <rect className="body" x={p.x} y={p.y} width={p.w} height={p.h} rx={24} />
            <text className="title" x={p.x + p.w / 2} y={p.y + 20} textAnchor="middle">
              {truncate(shortId(t.id), 24)}
            </text>
            <text className="subtitle" x={p.x + p.w / 2} y={p.y + 36} textAnchor="middle">
              {`topic · ${t.ordering}`}
            </text>
            <title>{`${t.id}\nordering: ${t.ordering}\nmessages: ${t.messages.map(shortId).join(", ")}`}</title>
            <StatusChip x={p.x + p.w - 6} y={p.y} obKey={t.id} />
          </g>
        );
      })}

      {graph.externals.map((ext) => {
        const p = layout.pos.get(ext.id);
        if (!p) return null;
        const classes = ["arch-node", "external"];
        if (isDim(ext.id)) classes.push("dimmed");
        if (selection === ext.id) classes.push("selected");
        return (
          <g key={ext.id} className={classes.join(" ")} data-sel={sel({ key: ext.id, id: ext.id })}>
            <rect className="body" x={p.x} y={p.y} width={p.w} height={p.h} rx={6} />
            <text className="title" x={p.x + p.w / 2} y={p.y + 19} textAnchor="middle">
              {truncate(ext.name, 24)}
            </text>
            <text className="subtitle" x={p.x + p.w / 2} y={p.y + 34} textAnchor="middle">
              external system
            </text>
            <title>{`external: ${ext.name}\nthe modeled system ends here`}</title>
          </g>
        );
      })}

      {graph.client && (() => {
        const p = layout.pos.get(graph.client.id)!;
        const classes = ["arch-node", "client"];
        if (isDim(graph.client.id)) classes.push("dimmed");
        if (selection === graph.client.id) classes.push("selected");
        return (
          <g className={classes.join(" ")} data-sel={sel({ key: graph.client.id, id: graph.client.id })}>
            <rect className="body" x={p.x} y={p.y} width={p.w} height={p.h} rx={10} />
            <text className="title" x={p.x + p.w / 2} y={p.y + 24} textAnchor="middle">
              clients
            </text>
            <text className="subtitle" x={p.x + p.w / 2} y={p.y + 40} textAnchor="middle">
              unmodeled callers
            </text>
            <title>request inputs no modeled operation invokes</title>
          </g>
        );
      })()}
    </SvgCanvas>
  );
}
