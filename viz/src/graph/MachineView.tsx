import { useMemo } from "react";

import { shortId, pathText } from "../lib/ids";
import { worstStatus } from "../lib/obligations";
import { useApp } from "../state/AppState";
import { layoutMachine } from "./layoutMachine";
import { LegendChip, LegendLine, SvgCanvas, sel } from "./SvgCanvas";

export function MachineView({ id, highlight }: { id: string; highlight: string | null }) {
  const { model, selection, obligations, overlay } = useApp();
  const machine = model.state_machines[id];
  const layout = useMemo(() => (machine ? layoutMachine(machine) : null), [machine]);

  const legend = (
    <>
      <LegendLine color="var(--arch-text-subtle)" label="transition" />
      <LegendLine color="var(--arch-edge-publish)" label="⚡ = transition side effects" />
      <LegendChip color="var(--arch-edge-client)" label="initial state" />
    </>
  );

  if (!machine || !layout) {
    return <SvgCanvas legend={legend} empty={`unknown state machine ${id}`}>{null}</SvgCanvas>;
  }

  return (
    <SvgCanvas legend={legend}>
      <text className="arch-heading" x={0} y={-70}>
        {shortId(id)}
      </text>
      <text className="arch-subheading" x={0} y={-50}>
        {`subject: ${machine.subject.object} · state field: ${pathText(machine.subject.state)}`}
      </text>

      {layout.edges.map((edge) => {
        const t = machine.transitions[edge.transition];
        const key = `t:${edge.transition}`;
        const isSel = selection === key;
        const isHl = highlight === edge.transition;
        const obs = overlay ? (obligations.get(`${id}/${edge.transition}`) ?? []) : [];
        const status = obs.length ? worstStatus(obs) : null;
        const stroke = isHl || isSel ? "var(--arch-accent)" : status ? `var(--arch-${status})` : undefined;
        const fxCount = Object.keys(t.side_effects).length;
        return (
          <g key={`${edge.transition}|${edge.from}`} data-sel={sel({ key, id: edge.transition })}>
            <path
              className={`arch-sm-edge${isSel ? " selected" : ""}`}
              d={edge.d}
              style={stroke ? { stroke } : undefined}
              markerEnd={isHl || isSel ? "url(#arr-accent)" : "url(#arr-neutral)"}
            />
            <path className="arch-edge-hit" d={edge.d} />
            <text
              className={`arch-sm-edge-label${isSel ? " selected" : ""}`}
              x={edge.labelX}
              y={edge.labelY}
              style={stroke ? { fill: stroke } : undefined}
            >
              {shortId(edge.transition)}
              {fxCount ? <tspan className="fx">{` ⚡${fxCount}`}</tspan> : null}
            </text>
            <title>
              {`${edge.transition}\nfrom: ${t.from.join(", ")}\nto: ${t.to}` +
                (fxCount ? `\nside effects: ${Object.keys(t.side_effects).join(", ")}` : "")}
            </title>
          </g>
        );
      })}

      {machine.states.map((s) => {
        const p = layout.pos.get(s)!;
        const isInitial = s === machine.initial;
        const key = `s:${s}`;
        const classes = ["arch-node", "state-node"];
        if (isInitial) classes.push("initial");
        if (selection === key) classes.push("selected");
        return (
          <g key={s} className={classes.join(" ")} data-sel={sel({ key, id: s })}>
            <rect className="body" x={p.x} y={p.y} width={p.w} height={p.h} rx={16} />
            <text className="title" x={p.x + p.w / 2} y={p.y + 22} textAnchor="middle">
              {p.label}
            </text>
            {isInitial && (
              <text className="init-mark" x={p.x + p.w / 2} y={p.y + 36} textAnchor="middle">
                ● initial
              </text>
            )}
            <title>{s}</title>
          </g>
        );
      })}
    </SvgCanvas>
  );
}
