import { Badge } from "@cloudflare/kumo/components/badge";
import { Empty } from "@cloudflare/kumo/components/empty";
import { Table } from "@cloudflare/kumo/components/table";
import { Text } from "@cloudflare/kumo/components/text";
import { GraphIcon } from "@phosphor-icons/react";
import { useMemo } from "react";

import { pathText, shortId } from "../lib/ids";
import { intentExecutors } from "../lib/index";
import { worstStatus } from "../lib/obligations";
import { useApp } from "../state/AppState";
import { IdLink, Mono, StatusChips } from "../panels/parts";
import { layoutMachine } from "./layoutMachine";

const PAD = 56;
const LABEL_W = 150;

export function MachineView({ id, highlight }: { id: string; highlight: string | null }) {
  const { model, graph, selection, obligations, overlay, select } = useApp();
  const machine = model.state_machines[id];
  const layout = useMemo(() => (machine ? layoutMachine(machine) : null), [machine]);

  if (!machine || !layout) {
    return (
      <div className="flex h-full items-center justify-center">
        <Empty size="sm" icon={<GraphIcon size={32} className="text-kumo-inactive" />} title={`unknown state machine ${id}`} />
      </div>
    );
  }

  // Scene bounds over nodes and labels, translated into a positive frame.
  const xs: number[] = [];
  const ys: number[] = [];
  for (const p of layout.pos.values()) {
    xs.push(p.x, p.x + p.w);
    ys.push(p.y, p.y + p.h);
  }
  for (const e of layout.edges) {
    xs.push(e.labelX, e.labelX + LABEL_W);
    ys.push(e.labelY - 12, e.labelY + 12);
  }
  const minX = Math.min(0, ...xs) - PAD;
  const minY = Math.min(0, ...ys) - PAD;
  const width = Math.max(...xs) - minX + PAD;
  const height = Math.max(...ys) - minY + PAD;
  const tx = (x: number) => x - minX;
  const ty = (y: number) => y - minY;

  const transitions = Object.entries(machine.transitions);

  return (
    <div className="h-full overflow-auto">
      <div className="space-y-7 p-6">
        <header className="space-y-2">
          <div className="flex flex-wrap items-baseline gap-3">
            <Text variant="heading" size="lg" as="h1">{shortId(id)}</Text>
            <Mono className="text-kumo-subtle">{id}</Mono>
          </div>
          <div className="flex flex-wrap items-center gap-2 text-sm text-kumo-subtle">
            <span>governs</span>
            <IdLink id={machine.subject.object} />
            <span>· state field</span>
            <Mono>{pathText(machine.subject.state)}</Mono>
            <span>·</span>
            <Badge variant="neutral">{machine.states.length} states</Badge>
            <Badge variant="neutral">{transitions.length} transitions</Badge>
            <StatusChips obKey={id} />
          </div>
        </header>

        <div className="relative rounded-xl border border-kumo-hairline bg-kumo-elevated/30" style={{ width, height }}>
          <svg className="absolute inset-0" width={width} height={height} style={{ pointerEvents: "none" }}>
            <defs>
              <marker id="sm-arrow" markerWidth={9} markerHeight={7} refX={8} refY={3.5} orient="auto" markerUnits="userSpaceOnUse">
                <path d="M0,0 L9,3.5 L0,7 Z" fill="var(--arch-text-subtle)" />
              </marker>
              <marker id="sm-arrow-accent" markerWidth={9} markerHeight={7} refX={8} refY={3.5} orient="auto" markerUnits="userSpaceOnUse">
                <path d="M0,0 L9,3.5 L0,7 Z" fill="var(--arch-accent)" />
              </marker>
            </defs>
            <g transform={`translate(${-minX},${-minY})`}>
              {layout.edges.map((edge) => {
                const key = `t:${edge.transition}`;
                const active = selection === key || highlight === edge.transition;
                const obs = overlay ? (obligations.get(`${id}/${edge.transition}`) ?? []) : [];
                const status = obs.length ? worstStatus(obs) : null;
                const stroke = active ? "var(--arch-accent)" : status ? `var(--arch-${status})` : undefined;
                return (
                  <g key={`${edge.transition}|${edge.from}`}>
                    <path
                      className={`arch-sm-edge${active ? " selected" : ""}`}
                      d={edge.d}
                      style={stroke ? { stroke } : undefined}
                      markerEnd={active ? "url(#sm-arrow-accent)" : "url(#sm-arrow)"}
                    />
                    <path
                      className="arch-edge-hit"
                      d={edge.d}
                      style={{ pointerEvents: "stroke" }}
                      onClick={() => select(key, { id: edge.transition, ctx: {} })}
                    />
                  </g>
                );
              })}
            </g>
          </svg>

          {machine.states.map((s) => {
            const p = layout.pos.get(s)!;
            const key = `s:${s}`;
            const selected = selection === key;
            const initial = s === machine.initial;
            return (
              <button
                key={s}
                type="button"
                onClick={() => select(key, { id: s, ctx: {} })}
                className={`absolute flex cursor-pointer flex-col items-center justify-center gap-1 rounded-xl border bg-kumo-base shadow-sm transition-shadow hover:shadow-md ${
                  initial ? "border-kumo-success" : "border-kumo-line"
                } ${selected ? "ring-2 ring-kumo-brand" : ""}`}
                style={{ left: tx(p.x), top: ty(p.y), width: p.w, height: p.h }}
              >
                <span className="font-mono text-[13px] font-semibold text-kumo-strong">{p.label}</span>
                {initial && <Badge variant="success" appearance="dot">initial</Badge>}
              </button>
            );
          })}

          {layout.edges.map((edge) => {
            const t = machine.transitions[edge.transition];
            const key = `t:${edge.transition}`;
            const active = selection === key || highlight === edge.transition;
            const fxCount = Object.keys(t.side_effects).length;
            return (
              <button
                key={`label:${edge.transition}|${edge.from}`}
                type="button"
                onClick={() => select(key, { id: edge.transition, ctx: {} })}
                className={`absolute -translate-y-1/2 cursor-pointer rounded-full ${active ? "ring-2 ring-kumo-brand" : ""}`}
                style={{ left: tx(edge.labelX), top: ty(edge.labelY) }}
                title={`${edge.transition}: ${edge.from} → ${t.to}`}
              >
                <Badge variant={active ? "info" : "outline"}>
                  {shortId(edge.transition)}
                  {fxCount ? ` ⚡${fxCount}` : ""}
                </Badge>
              </button>
            );
          })}
        </div>

        <section className="space-y-2">
          <div className="text-[11px] font-semibold uppercase tracking-wider text-kumo-subtle">transitions</div>
          <div className="overflow-x-auto rounded-lg border border-kumo-hairline">
            <Table>
              <Table.Header>
                <Table.Row>
                  <Table.Head>transition</Table.Head>
                  <Table.Head>from</Table.Head>
                  <Table.Head>to</Table.Head>
                  <Table.Head>side effects</Table.Head>
                  <Table.Head>taken by</Table.Head>
                  <Table.Head>verdicts</Table.Head>
                </Table.Row>
              </Table.Header>
              <Table.Body>
                {transitions.map(([tId, t]) => {
                  const key = `t:${tId}`;
                  const refs = graph.transition_refs[`${id}/${tId}`] ?? [];
                  const effects = Object.entries(t.side_effects);
                  return (
                    <Table.Row
                      key={tId}
                      className={`cursor-pointer ${selection === key ? "bg-kumo-tint" : ""}`}
                      onClick={() => select(key, { id: tId, ctx: {} })}
                    >
                      <Table.Cell><Mono className="text-kumo-strong">{shortId(tId)}</Mono></Table.Cell>
                      <Table.Cell><Mono className="text-kumo-subtle">{t.from.map(shortId).join(", ")}</Mono></Table.Cell>
                      <Table.Cell><Mono className="text-kumo-subtle">{shortId(t.to)}</Mono></Table.Cell>
                      <Table.Cell>
                        {effects.length ? (
                          <div className="flex flex-col gap-1">
                            {effects.map(([eid, e]) => (
                              <span key={eid} className="flex flex-wrap items-center gap-1.5">
                                <Badge variant={e.kind === "publication" ? "purple" : "orange"}>{e.kind}</Badge>
                                <IdLink id={eid}>{shortId(eid)}</IdLink>
                                {intentExecutors(model, eid).map((x) => (
                                  <span key={x.intent} className="text-xs text-kumo-subtle">via <IdLink id={x.op}>{shortId(x.op)}</IdLink></span>
                                ))}
                              </span>
                            ))}
                          </div>
                        ) : (
                          <span className="text-xs text-kumo-inactive">none</span>
                        )}
                      </Table.Cell>
                      <Table.Cell>
                        {refs.length ? (
                          <div className="flex flex-col gap-1">
                            {refs.map((r, i) => (
                              <span key={i} className="text-xs">
                                <IdLink id={r.transaction}>{shortId(r.transaction)}</IdLink>
                                <span className="text-kumo-subtle"> step {r.step + 1}</span>
                              </span>
                            ))}
                          </div>
                        ) : (
                          <span className="text-xs text-kumo-inactive">no transaction</span>
                        )}
                      </Table.Cell>
                      <Table.Cell><StatusChips obKey={`${id}/${tId}`} /></Table.Cell>
                    </Table.Row>
                  );
                })}
              </Table.Body>
            </Table>
          </div>
        </section>
      </div>
    </div>
  );
}
