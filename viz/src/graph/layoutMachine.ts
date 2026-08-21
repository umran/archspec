import { shortId } from "../lib/ids";
import type { Id, StateMachine } from "../types/model";

export interface StateBox {
  x: number;
  y: number;
  w: number;
  h: number;
  label: string;
}

export interface TransitionGeometry {
  transition: Id;
  from: Id;
  to: Id;
  d: string;
  labelX: number;
  labelY: number;
}

export interface MachineLayout {
  pos: Map<Id, StateBox>;
  edges: TransitionGeometry[];
}

const XGAP = 270;
const YGAP = 160;
const H = 58;

export function layoutMachine(machine: StateMachine): MachineLayout {
  // BFS layering from the initial state.
  const succ = new Map<Id, Set<Id>>(machine.states.map((s) => [s, new Set()]));
  for (const t of Object.values(machine.transitions)) {
    for (const f of t.from) succ.get(f)?.add(t.to);
  }
  const layerOf = new Map<Id, number>();
  let frontier = [machine.initial];
  let depth = 0;
  while (frontier.length) {
    const next: Id[] = [];
    for (const s of frontier) {
      if (layerOf.has(s)) continue;
      layerOf.set(s, depth);
      for (const n of succ.get(s) ?? []) next.push(n);
    }
    frontier = next;
    depth += 1;
  }
  const maxLayer = Math.max(0, ...layerOf.values());
  for (const s of machine.states) if (!layerOf.has(s)) layerOf.set(s, maxLayer + 1);

  const layers: Id[][] = [];
  for (const [s, l] of layerOf) (layers[l] = layers[l] ?? []).push(s);
  layers.forEach((l) => l.sort());

  const pos = new Map<Id, StateBox>();
  layers.forEach((states, li) => {
    states.forEach((s, i) => {
      const label = shortId(s);
      const w = Math.max(160, label.length * 8 + 48);
      pos.set(s, { x: (i - (states.length - 1) / 2) * XGAP - w / 2, y: li * YGAP, w, h: H, label });
    });
  });

  // Rightmost node edge; upward bows route beyond it so they clear
  // every layer they pass.
  const rightmost = Math.max(0, ...[...pos.values()].map((p) => p.x + p.w));

  const edges: TransitionGeometry[] = [];
  const pairSeen = new Map<string, number>();
  for (const [tId, t] of Object.entries(machine.transitions)) {
    for (const from of t.from) {
      const a = pos.get(from);
      const b = pos.get(t.to);
      if (!a || !b) continue;
      const pk = `${from}→${t.to}`;
      const n = pairSeen.get(pk) ?? 0;
      pairSeen.set(pk, n + 1);
      const offset = n * 26;

      let d: string;
      let labelX: number;
      let labelY: number;
      if (from === t.to) {
        const cx = a.x + a.w;
        const cy = a.y + a.h / 2;
        const bow = 60 + offset;
        d = `M${cx},${cy - 10} C${cx + bow},${cy - 34 - offset} ${cx + bow},${cy + 34 + offset} ${cx},${cy + 10}`;
        labelX = cx + bow + 8;
        labelY = cy - offset * 0.6;
      } else if (b.y > a.y) {
        const p1 = { x: a.x + a.w / 2 + offset, y: a.y + a.h };
        const p2 = { x: b.x + b.w / 2 + offset, y: b.y };
        const dy = Math.max(34, (p2.y - p1.y) * 0.35);
        d = `M${p1.x},${p1.y} C${p1.x},${p1.y + dy} ${p2.x},${p2.y - dy} ${p2.x},${p2.y}`;
        labelX = (p1.x + p2.x) / 2 + 10;
        labelY = (p1.y + p2.y) / 2 - offset;
      } else if (b.y === a.y) {
        const dir = b.x > a.x ? 1 : -1;
        const p1 = { x: a.x + a.w / 2 + dir * 14, y: a.y + a.h };
        const p2 = { x: b.x + b.w / 2 - dir * 14, y: b.y + b.h };
        const sag = 46 + offset + (dir < 0 ? 24 : 0);
        d = `M${p1.x},${p1.y} C${p1.x},${p1.y + sag} ${p2.x},${p2.y + sag} ${p2.x},${p2.y}`;
        labelX = (p1.x + p2.x) / 2 + 10;
        labelY = (p1.y + p2.y) / 2 + sag * 0.75 + 4;
      } else {
        const x1 = a.x + a.w;
        const y1 = a.y + a.h / 2;
        const x2 = b.x + b.w;
        const y2 = b.y + b.h / 2;
        const cxr = rightmost + 40 + offset;
        d = `M${x1},${y1} C${cxr},${y1} ${cxr},${y2} ${x2},${y2}`;
        labelX = (x1 + x2 + 6 * cxr) / 8 + 8;
        labelY = (y1 + y2) / 2;
      }
      edges.push({ transition: tId, from, to: t.to, d, labelX, labelY });
    }
  }

  return { pos, edges };
}
