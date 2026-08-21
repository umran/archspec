import type { Edge, Graph } from "../types/graph";

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ServiceBox extends Box {
  id: string;
}

export interface Point {
  x: number;
  y: number;
}

export interface EdgeGeometry {
  edge: Edge;
  d: string;
  from: Point;
  to: Point;
}

export interface SystemLayout {
  pos: Map<string, Box>;
  services: ServiceBox[];
  edges: EdgeGeometry[];
}

export const SYS = {
  OP_W: 200, OP_H: 62, OP_VGAP: 16,
  SVC_PAD: 16, SVC_TITLE: 34, SVC_GAP: 100,
  TOPIC_W: 200, TOPIC_H: 48, TOPIC_BAND: 150, TOPIC_GAP: 60,
  EXT_W: 200, EXT_H: 46, EXT_BAND: 130,
  CLIENT_W: 160, CLIENT_H: 58, CLIENT_GAP: 130,
};

/**
 * Service-level reachability order: requests directly, pub/sub through
 * topics. Entry services are those with client-facing inputs.
 */
function serviceOrder(graph: Graph): string[] {
  const ids = graph.services.map((s) => s.id);
  const opService = new Map(graph.operations.map((o) => [o.id, o.service]));
  const succ = new Map<string, Set<string>>(ids.map((id) => [id, new Set()]));
  const pubs = new Map<string, Set<string>>();
  const subs = new Map<string, Set<string>>();
  const entries = new Set<string>();

  const add = (map: Map<string, Set<string>>, key: string, value: string) => {
    let set = map.get(key);
    if (!set) {
      set = new Set();
      map.set(key, set);
    }
    set.add(value);
  };

  for (const e of graph.edges) {
    if (e.kind === "request") {
      const a = opService.get(e.operation);
      const b = opService.get(e.to);
      if (a && b && a !== b) succ.get(a)?.add(b);
    } else if (e.kind === "publish") {
      const a = opService.get(e.operation);
      if (a) add(pubs, e.to, a);
    } else if (e.kind === "subscribe") {
      const b = opService.get(e.operation);
      if (b) add(subs, e.from, b);
    } else if (e.kind === "client") {
      const b = opService.get(e.operation);
      if (b) entries.add(b);
    }
  }

  for (const [topic, publishers] of pubs) {
    for (const p of publishers) {
      for (const s of subs.get(topic) ?? []) if (p !== s) succ.get(p)?.add(s);
    }
  }

  let roots = [...entries];
  if (!roots.length) {
    const indeg = new Map(ids.map((id) => [id, 0]));
    for (const [, targets] of succ) {
      for (const s of targets) indeg.set(s, (indeg.get(s) ?? 0) + 1);
    }
    roots = ids.filter((id) => !indeg.get(id));
  }
  if (!roots.length) roots = ids.slice(0, 1);

  const rank = new Map<string, number>();
  const queue: [string, number][] = roots.map((id) => [id, 0]);
  while (queue.length) {
    const [id, d] = queue.shift()!;
    if (rank.has(id)) continue;
    rank.set(id, d);
    for (const s of succ.get(id) ?? []) queue.push([s, d + 1]);
  }
  const maxRank = Math.max(0, ...rank.values());
  for (const id of ids) if (!rank.has(id)) rank.set(id, maxRank + 1);

  return [...ids].sort((a, b) => rank.get(a)! - rank.get(b)! || a.localeCompare(b));
}

/** Assigns ports along one side of a node for a set of edges. */
function assignPorts(edges: Edge[], node: Box, side: "top" | "bottom", sortBy: (e: Edge) => number) {
  const sorted = [...edges].sort((a, b) => sortBy(a) - sortBy(b));
  const n = sorted.length;
  const inset = Math.min(26, node.w / (n + 1));
  const span = node.w - inset * 2;
  const ports = new Map<string, Point>();
  sorted.forEach((e, i) => {
    const frac = n === 1 ? 0.5 : i / (n - 1);
    ports.set(e.id, { x: node.x + inset + frac * span, y: side === "top" ? node.y : node.y + node.h });
  });
  return ports;
}

export function layoutSystem(graph: Graph): SystemLayout {
  const pos = new Map<string, Box>();
  const services: ServiceBox[] = [];
  const byService = new Map<string, Graph["operations"]>(graph.services.map((s) => [s.id, []]));
  for (const op of graph.operations) {
    if (!byService.has(op.service)) byService.set(op.service, []);
    byService.get(op.service)!.push(op);
  }

  let x = 0;
  for (const svcId of serviceOrder(graph)) {
    const ops = byService.get(svcId) ?? [];
    const w = SYS.OP_W + SYS.SVC_PAD * 2;
    const h =
      SYS.SVC_TITLE + SYS.SVC_PAD + ops.length * SYS.OP_H + Math.max(0, ops.length - 1) * SYS.OP_VGAP;
    services.push({ id: svcId, x, y: 0, w, h: Math.max(h, 70) });
    ops.forEach((op, i) => {
      pos.set(op.id, {
        x: x + SYS.SVC_PAD,
        y: SYS.SVC_TITLE + i * (SYS.OP_H + SYS.OP_VGAP),
        w: SYS.OP_W,
        h: SYS.OP_H,
      });
    });
    x += w + SYS.SVC_GAP;
  }

  const maxBottom = Math.max(80, ...services.map((b) => b.y + b.h));

  // Band placement shared by topics and externals: desired x is the
  // mean of connected operation centers; overlaps resolved in order.
  function placeBand<T extends { id: string }>(
    nodes: T[],
    connectedOps: (node: T) => string[],
    w: number,
    gap: number,
    y: number,
    h: number,
  ) {
    const items = nodes
      .map((n) => {
        const ops = connectedOps(n).map((id) => pos.get(id)).filter((p): p is Box => !!p);
        const cx = ops.length ? ops.reduce((a, p) => a + p.x + p.w / 2, 0) / ops.length : 0;
        return { n, cx };
      })
      .sort((a, b) => a.cx - b.cx || a.n.id.localeCompare(b.n.id));
    let cursor = -Infinity;
    for (const item of items) {
      const left = Math.max(item.cx - w / 2, cursor);
      pos.set(item.n.id, { x: left, y, w, h });
      cursor = left + w + gap;
    }
  }

  placeBand(
    graph.topics,
    (t) =>
      graph.edges
        .filter((e) => (e.kind === "publish" && e.to === t.id) || (e.kind === "subscribe" && e.from === t.id))
        .map((e) => (e.kind === "publish" || e.kind === "subscribe" ? e.operation : "")),
    SYS.TOPIC_W, SYS.TOPIC_GAP, maxBottom + SYS.TOPIC_BAND, SYS.TOPIC_H,
  );

  placeBand(
    graph.externals,
    (ext) =>
      graph.edges
        .filter((e) => e.kind === "external" && e.to === ext.id)
        .map((e) => (e.kind === "external" ? e.operation : "")),
    SYS.EXT_W, SYS.TOPIC_GAP, -(SYS.EXT_BAND + SYS.EXT_H), SYS.EXT_H,
  );

  if (graph.client) {
    const targets = graph.edges
      .filter((e) => e.kind === "client")
      .map((e) => pos.get(e.to))
      .filter((p): p is Box => !!p);
    const cy = targets.length ? targets.reduce((a, p) => a + p.y + p.h / 2, 0) / targets.length : 100;
    const minX = Math.min(0, ...services.map((b) => b.x));
    pos.set(graph.client.id, {
      x: minX - SYS.CLIENT_GAP - SYS.CLIENT_W,
      y: cy - SYS.CLIENT_H / 2,
      w: SYS.CLIENT_W,
      h: SYS.CLIENT_H,
    });
  }

  return { pos, services, edges: routeEdges(graph, pos) };
}

function routeEdges(graph: Graph, pos: Map<string, Box>): EdgeGeometry[] {
  const portOf = new Map<string, { from?: Point; to?: Point }>();
  const vertical = graph.edges.filter(
    (e) => e.kind === "publish" || e.kind === "subscribe" || e.kind === "external",
  );
  const record = (id: string, side: "from" | "to", point: Point) => {
    const rec = portOf.get(id) ?? {};
    rec[side] = point;
    portOf.set(id, rec);
  };

  for (const op of graph.operations) {
    const p = pos.get(op.id);
    if (!p) continue;
    const bottom = vertical.filter((e) => e.kind !== "external" && (e.from === op.id || e.to === op.id));
    const bPorts = assignPorts(bottom, p, "bottom", (e) => {
      const other = pos.get(e.kind === "publish" ? e.to : e.from);
      return other ? other.x : 0;
    });
    for (const [id, pt] of bPorts) {
      const e = graph.edges.find((x) => x.id === id)!;
      record(id, e.kind === "publish" ? "from" : "to", pt);
    }
    const top = vertical.filter((e) => e.kind === "external" && e.from === op.id);
    const tPorts = assignPorts(top, p, "top", (e) => pos.get(e.to)?.x ?? 0);
    for (const [id, pt] of tPorts) record(id, "from", pt);
  }

  for (const t of graph.topics) {
    const p = pos.get(t.id);
    if (!p) continue;
    const es = vertical.filter((e) => e.from === t.id || e.to === t.id);
    const ports = assignPorts(es, p, "top", (e) => {
      const other = pos.get(e.kind === "publish" ? e.operation : e.to);
      return other ? other.x : 0;
    });
    for (const [id, pt] of ports) {
      const e = graph.edges.find((x) => x.id === id)!;
      record(id, e.kind === "publish" ? "to" : "from", pt);
    }
  }

  for (const ext of graph.externals) {
    const p = pos.get(ext.id);
    if (!p) continue;
    const es = vertical.filter((e) => e.to === ext.id);
    const ports = assignPorts(es, p, "bottom", (e) =>
      e.kind === "external" ? (pos.get(e.operation)?.x ?? 0) : 0,
    );
    for (const [id, pt] of ports) record(id, "to", { x: pt.x, y: p.y + p.h });
  }

  const out: EdgeGeometry[] = [];
  for (const e of graph.edges) {
    const rec = portOf.get(e.id) ?? {};
    const a = pos.get(e.from);
    const b = pos.get(e.to);
    if (!a || !b) continue;

    let p1 = rec.from;
    let p2 = rec.to;
    let d: string;
    if (e.kind === "request") {
      const forward = b.x >= a.x + a.w + 20;
      if (forward) {
        p1 = { x: a.x + a.w, y: a.y + a.h / 2 };
        p2 = { x: b.x, y: b.y + b.h / 2 };
        const dx = Math.min(140, Math.max(40, (p2.x - p1.x) * 0.35));
        d = `M${p1.x},${p1.y} C${p1.x + dx},${p1.y} ${p2.x - dx},${p2.y} ${p2.x},${p2.y}`;
      } else {
        p1 = { x: a.x + a.w / 2, y: a.y };
        p2 = { x: b.x + b.w / 2, y: b.y };
        const top = Math.min(p1.y, p2.y) - 70;
        d = `M${p1.x},${p1.y} C${p1.x},${top} ${p2.x},${top} ${p2.x},${p2.y}`;
      }
    } else if (e.kind === "client") {
      p1 = { x: a.x + a.w, y: a.y + a.h / 2 };
      p2 = { x: b.x, y: b.y + b.h / 2 };
      const dx = Math.min(120, Math.max(40, Math.abs(p2.x - p1.x) * 0.4));
      d = `M${p1.x},${p1.y} C${p1.x + dx},${p1.y} ${p2.x - dx},${p2.y} ${p2.x},${p2.y}`;
    } else {
      if (!p1 || !p2) continue;
      const dir = p2.y > p1.y ? 1 : -1;
      const dy = Math.min(120, Math.max(36, Math.abs(p2.y - p1.y) * 0.4));
      d = `M${p1.x},${p1.y} C${p1.x},${p1.y + dir * dy} ${p2.x},${p2.y - dir * dy} ${p2.x},${p2.y}`;
    }
    out.push({ edge: e, d, from: p1!, to: p2! });
  }
  return out;
}
