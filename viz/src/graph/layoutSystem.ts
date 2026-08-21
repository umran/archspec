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
  /** Where a label for the edge reads best. */
  labelAt: Point;
}

export interface SystemLayout {
  pos: Map<string, Box>;
  services: ServiceBox[];
  edges: EdgeGeometry[];
}

export const SYS = {
  OP_W: 200, OP_H: 62, OP_HGAP: 18,
  SVC_PAD: 16, SVC_TITLE: 34, SVC_GAP: 96,
  TOPIC_W: 200, TOPIC_H: 48, TOPIC_BAND: 150, TOPIC_GAP: 60,
  EXT_W: 200, EXT_H: 46, EXT_BAND: 150,
  CLIENT_W: 160, CLIENT_H: 58, CLIENT_GAP: 170,
  /** Routing channel above the service row, below the externals. */
  CHANNEL_Y: -60, LANE: 12, CORNER: 18, PORT_INSET: 26,
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
  const inset = Math.min(SYS.PORT_INSET, node.w / (n + 1));
  const span = node.w - inset * 2;
  const ports = new Map<string, Point>();
  sorted.forEach((e, i) => {
    const frac = n === 1 ? 0.5 : i / (n - 1);
    ports.set(e.id, { x: node.x + inset + frac * span, y: side === "top" ? node.y : node.y + node.h });
  });
  return ports;
}

/**
 * Operations sit in one row inside their service, so every operation
 * has a clear vertical line to the topic band below and the externals
 * band above; nothing stacks under anything else.
 */
export function layoutSystem(graph: Graph): SystemLayout {
  const pos = new Map<string, Box>();
  const services: ServiceBox[] = [];
  const byService = new Map<string, Graph["operations"]>(graph.services.map((s) => [s.id, []]));
  for (const op of graph.operations) {
    if (!byService.has(op.service)) byService.set(op.service, []);
    byService.get(op.service)!.push(op);
  }
  const clientFacing = new Set(graph.edges.filter((e) => e.kind === "client").map((e) => e.to));

  let x = 0;
  for (const svcId of serviceOrder(graph)) {
    // Client-facing operations first, so the entry edge from the left
    // reaches the row's first card without passing the others.
    const ops = [...(byService.get(svcId) ?? [])].sort(
      (a, b) => Number(clientFacing.has(b.id)) - Number(clientFacing.has(a.id)) || a.id.localeCompare(b.id),
    );
    const n = Math.max(1, ops.length);
    const w = SYS.SVC_PAD * 2 + n * SYS.OP_W + (n - 1) * SYS.OP_HGAP;
    const h = SYS.SVC_TITLE + SYS.SVC_PAD + SYS.OP_H;
    services.push({ id: svcId, x, y: 0, w, h });
    ops.forEach((op, i) => {
      pos.set(op.id, { x: x + SYS.SVC_PAD + i * (SYS.OP_W + SYS.OP_HGAP), y: SYS.SVC_TITLE, w: SYS.OP_W, h: SYS.OP_H });
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
    const minX = Math.min(0, ...services.map((b) => b.x));
    pos.set(graph.client.id, {
      x: minX - SYS.CLIENT_GAP - SYS.CLIENT_W,
      y: SYS.SVC_TITLE + SYS.OP_H / 2 - SYS.CLIENT_H / 2,
      w: SYS.CLIENT_W,
      h: SYS.CLIENT_H,
    });
  }

  return { pos, services, edges: routeEdges(graph, pos, services) };
}

/** An axis-aligned polyline with rounded corners. */
function roundedPolyline(points: Point[], radius: number): string {
  if (points.length < 2) return "";
  let d = `M${points[0].x},${points[0].y}`;
  for (let i = 1; i < points.length - 1; i++) {
    const prev = points[i - 1];
    const corner = points[i];
    const next = points[i + 1];
    const inLen = Math.hypot(corner.x - prev.x, corner.y - prev.y);
    const outLen = Math.hypot(next.x - corner.x, next.y - corner.y);
    const r = Math.min(radius, inLen / 2, outLen / 2);
    const ux = inLen ? (corner.x - prev.x) / inLen : 0;
    const uy = inLen ? (corner.y - prev.y) / inLen : 0;
    const vx = outLen ? (next.x - corner.x) / outLen : 0;
    const vy = outLen ? (next.y - corner.y) / outLen : 0;
    d += ` L${corner.x - ux * r},${corner.y - uy * r}`;
    d += ` Q${corner.x},${corner.y} ${corner.x + vx * r},${corner.y + vy * r}`;
  }
  const last = points[points.length - 1];
  d += ` L${last.x},${last.y}`;
  return d;
}

function verticalCubic(p1: Point, p2: Point): string {
  const dir = p2.y > p1.y ? 1 : -1;
  const dy = Math.min(120, Math.max(36, Math.abs(p2.y - p1.y) * 0.4));
  return `M${p1.x},${p1.y} C${p1.x},${p1.y + dir * dy} ${p2.x},${p2.y - dir * dy} ${p2.x},${p2.y}`;
}

function horizontalCubic(p1: Point, p2: Point): string {
  const dx = Math.min(140, Math.max(40, Math.abs(p2.x - p1.x) * 0.35));
  return `M${p1.x},${p1.y} C${p1.x + dx},${p1.y} ${p2.x - dx},${p2.y} ${p2.x},${p2.y}`;
}

function routeEdges(graph: Graph, pos: Map<string, Box>, services: ServiceBox[]): EdgeGeometry[] {
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

  const serviceIndex = new Map(services.map((s, i) => [s.id, i]));
  const opService = new Map(graph.operations.map((o) => [o.id, o.service]));
  const rowOf = (svc: string) => graph.operations.filter((o) => o.service === svc).map((o) => pos.get(o.id)!).filter(Boolean);
  const isLeftmost = (box: Box, svc: string) => rowOf(svc).every((p) => p.x >= box.x);
  const isRightmost = (box: Box, svc: string) => rowOf(svc).every((p) => p.x <= box.x);

  const out: EdgeGeometry[] = [];
  let channelLane = 0;

  for (const e of graph.edges) {
    const rec = portOf.get(e.id) ?? {};
    const a = pos.get(e.from);
    const b = pos.get(e.to);
    if (!a || !b) continue;

    if (e.kind === "request") {
      const sa = opService.get(e.operation);
      const sb = opService.get(e.to);
      const ia = sa !== undefined ? serviceIndex.get(sa) : undefined;
      const ib = sb !== undefined ? serviceIndex.get(sb) : undefined;
      const adjacentForward =
        sa !== undefined && sb !== undefined && ia !== undefined && ib !== undefined &&
        ib === ia + 1 && isRightmost(a, sa) && isLeftmost(b, sb);
      if (adjacentForward) {
        const p1 = { x: a.x + a.w, y: a.y + a.h / 2 };
        const p2 = { x: b.x, y: b.y + b.h / 2 };
        out.push({ edge: e, d: horizontalCubic(p1, p2), from: p1, to: p2, labelAt: { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 - 8 } });
      } else {
        // Over the top: up from the source, along the channel, down
        // into the target. Every lane gets its own height so parallel
        // routes stay distinguishable.
        const lane = channelLane++;
        const yc = SYS.CHANNEL_Y - lane * SYS.LANE;
        const p1 = { x: a.x + a.w - SYS.PORT_INSET - 8, y: a.y };
        const p2 = { x: b.x + SYS.PORT_INSET + 8, y: b.y };
        const points = [p1, { x: p1.x, y: yc }, { x: p2.x, y: yc }, p2];
        out.push({ edge: e, d: roundedPolyline(points, SYS.CORNER), from: p1, to: p2, labelAt: { x: (p1.x + p2.x) / 2, y: yc - 8 } });
      }
    } else if (e.kind === "client") {
      const sb = opService.get(e.operation);
      const first = services[0];
      const direct = !!sb && first !== undefined && serviceIndex.get(sb) === 0 && isLeftmost(b, sb);
      if (direct) {
        const p1 = { x: a.x + a.w, y: a.y + a.h / 2 };
        const p2 = { x: b.x, y: b.y + b.h / 2 };
        out.push({ edge: e, d: horizontalCubic(p1, p2), from: p1, to: p2, labelAt: { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 - 8 } });
      } else {
        const lane = channelLane++;
        const yc = SYS.CHANNEL_Y - lane * SYS.LANE;
        const riser = a.x + a.w + 40 + lane * SYS.LANE;
        const p1 = { x: a.x + a.w, y: a.y + a.h / 2 };
        const p2 = { x: b.x + SYS.PORT_INSET + 8, y: b.y };
        const points = [p1, { x: riser, y: p1.y }, { x: riser, y: yc }, { x: p2.x, y: yc }, p2];
        out.push({ edge: e, d: roundedPolyline(points, SYS.CORNER), from: p1, to: p2, labelAt: { x: (riser + p2.x) / 2, y: yc - 8 } });
      }
    } else {
      const p1 = rec.from;
      const p2 = rec.to;
      if (!p1 || !p2) continue;
      out.push({ edge: e, d: verticalCubic(p1, p2), from: p1, to: p2, labelAt: { x: (p1.x + p2.x) / 2 + 8, y: (p1.y + p2.y) / 2 - 6 } });
    }
  }
  return out;
}
