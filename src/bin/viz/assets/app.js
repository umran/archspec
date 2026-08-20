/* archspec-viz front end.
 *
 * Consumes window.ARCHSPEC = { title, model, graph, report }:
 *   model  — the full parsed Model, serialized wholesale (detail panes)
 *   graph  — the derived system graph (vertices, edges, indexes)
 *   report — optional prover report in the provisional format
 *
 * Three views, routed by location.hash:
 *   #/system        service boundaries, operations, topics, edges
 *   #/op/<id>       one operation: flows -> steps -> transactions
 *   #/machine/<id>  one state machine's state graph
 */
(() => {
"use strict";

const DATA = window.ARCHSPEC;
const MODEL = DATA.model;
const GRAPH = DATA.graph;
const REPORT = DATA.report || null;

/* ================= utilities ================= */

const SVGNS = "http://www.w3.org/2000/svg";

function S(tag, attrs, ...children) {
  const node = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v !== null && v !== undefined) node.setAttribute(k, v);
  }
  for (const child of children) {
    if (child === null || child === undefined) continue;
    node.append(child.nodeType ? child : document.createTextNode(child));
  }
  return node;
}

function H(tag, attrs, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined) continue;
    if (k === "class") node.className = v;
    else node.setAttribute(k, v);
  }
  for (const child of children.flat(Infinity)) {
    if (child === null || child === undefined) continue;
    node.append(child.nodeType ? child : document.createTextNode(child));
  }
  return node;
}

const KIND_PREFIXES = new Set([
  "service", "operation", "topic", "schema", "machine", "state",
  "transition", "flow", "tx", "intent", "effect", "input", "result",
  "response", "object", "data", "read", "oblig",
]);

function shortId(id) {
  if (typeof id !== "string") return String(id);
  const dot = id.indexOf(".");
  if (dot > 0 && KIND_PREFIXES.has(id.slice(0, dot))) {
    return id.slice(dot + 1);
  }
  return id;
}

function trunc(s, n) {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

function wrapText(text, maxChars) {
  const words = String(text).split(/\s+/);
  const lines = [];
  let line = "";
  for (const w of words) {
    if (line && (line + " " + w).length > maxChars) {
      lines.push(line);
      line = w;
    } else {
      line = line ? line + " " + w : w;
    }
  }
  if (line) lines.push(line);
  return lines;
}

function pathText(path) {
  return Array.isArray(path) ? path.join(".") : String(path);
}

/* ================= global id index ================= */

const IX = new Map();

function index(id, entry) { if (!IX.has(id)) IX.set(id, entry); }

for (const id of Object.keys(MODEL.services)) index(id, { kind: "service" });
for (const id of Object.keys(MODEL.schemas)) index(id, { kind: "schema" });
for (const id of Object.keys(MODEL.topics)) index(id, { kind: "topic" });

for (const [dmId, dm] of Object.entries(MODEL.data_models)) {
  index(dmId, { kind: "data_model" });
  for (const objId of Object.keys(dm.objects)) {
    index(objId, { kind: "object", dataModel: dmId });
  }
}

for (const [mId, m] of Object.entries(MODEL.state_machines)) {
  index(mId, { kind: "machine" });
  for (const s of m.states) index(s, { kind: "state", machine: mId });
  for (const tId of Object.keys(m.transitions)) {
    index(tId, { kind: "transition", machine: mId });
  }
}

for (const [opId, op] of Object.entries(MODEL.operations)) {
  index(opId, { kind: "operation" });
  for (const id of Object.keys(op.inputs)) index(id, { kind: "input", op: opId });
  for (const id of Object.keys(op.effects)) index(id, { kind: "effect", op: opId });
  for (const id of Object.keys(op.effect_intents)) index(id, { kind: "intent", op: opId });
  for (const id of Object.keys(op.invocation_results)) index(id, { kind: "result", op: opId });
  for (const id of Object.keys(op.responses)) index(id, { kind: "response", op: opId });
  for (const id of Object.keys(op.transactions)) index(id, { kind: "transaction", op: opId });
  for (const id of Object.keys(op.flows)) index(id, { kind: "flow", op: opId });
}

// Transition-owned effects.
for (const [mId, m] of Object.entries(MODEL.state_machines)) {
  for (const [tId, t] of Object.entries(m.transitions)) {
    for (const eId of Object.keys(t.side_effects)) {
      index(eId, { kind: "effect", machine: mId, transition: tId });
    }
  }
}

/* Resolve an effect id to its definition wherever it is declared. */
function effectDef(effectId) {
  const entry = IX.get(effectId);
  if (!entry || entry.kind !== "effect") return null;
  if (entry.op) {
    return { effect: MODEL.operations[entry.op].effects[effectId], owner: entry };
  }
  const t = MODEL.state_machines[entry.machine].transitions[entry.transition];
  return { effect: t.side_effects[effectId], owner: entry };
}

function effectSummary(effectId) {
  const def = effectDef(effectId);
  if (!def) return "unresolved effect " + effectId;
  const e = def.effect;
  if (e.kind === "publication") {
    return "publish " + shortId(e.schema) + " → " + shortId(e.topic);
  }
  if (e.kind === "request") {
    return "request → " + shortId(e.target.operation);
  }
  return "external " + e.name;
}

/* Operations that execute a given effect through an intent. */
function intentExecutors(effectId) {
  const out = [];
  for (const [opId, op] of Object.entries(MODEL.operations)) {
    for (const [intentId, intent] of Object.entries(op.effect_intents)) {
      if (intent.effect === effectId) out.push({ op: opId, intent: intentId });
    }
  }
  return out;
}

/* ================= report indexing ================= */

const STATUS_RANK = { proven: 0, unknown: 1, disproven: 2 };

function subjectKeys(s) {
  switch (s.kind) {
    case "operation": return [s.operation];
    case "flow": return [s.operation + "/" + s.flow, s.operation];
    case "transaction": return [s.operation + "/" + s.transaction, s.operation];
    case "object": return [s.data_model + "/" + s.object];
    case "state_machine":
      return s.transition
        ? [s.machine + "/" + s.transition, s.machine]
        : [s.machine];
    case "topic": return [s.topic];
    default: return [];
  }
}

const OB_INDEX = new Map(); // subject key -> [obligation]
if (REPORT) {
  for (const ob of REPORT.obligations) {
    for (const key of subjectKeys(ob.subject)) {
      if (!OB_INDEX.has(key)) OB_INDEX.set(key, []);
      OB_INDEX.get(key).push(ob);
    }
  }
}

let overlayOn = !!REPORT;

function obligationsAt(key) {
  if (!REPORT || !overlayOn) return [];
  return OB_INDEX.get(key) || [];
}

function statusCounts(obs) {
  const c = { proven: 0, disproven: 0, unknown: 0 };
  for (const ob of obs) c[ob.status] = (c[ob.status] || 0) + 1;
  return c;
}

function worstStatus(obs) {
  let worst = null;
  for (const ob of obs) {
    if (worst === null || STATUS_RANK[ob.status] > STATUS_RANK[worst]) {
      worst = ob.status;
    }
  }
  return worst;
}

const STATUS_GLYPH = { proven: "✓", disproven: "✗", unknown: "?" };

function statusChipText(counts) {
  const parts = [];
  for (const s of ["disproven", "unknown", "proven"]) {
    if (counts[s]) parts.push(STATUS_GLYPH[s] + counts[s]);
  }
  return parts.join(" ");
}

/* ================= dom handles & app state ================= */

const svg = document.getElementById("canvas");
const legendBox = document.getElementById("legend");
const emptyNote = document.getElementById("empty-note");
const detailPane = document.getElementById("detail");
const detailKind = document.getElementById("detail-kind");
const detailBody = document.getElementById("detail-body");
const obPane = document.getElementById("obligations");
const obList = document.getElementById("obligations-list");
const obFilters = document.getElementById("obligations-filters");
const crumbs = document.getElementById("crumbs");
const searchBox = document.getElementById("search");

const state = {
  route: { view: "system" },
  selection: null,       // string key for highlight matching
  expandedTx: new Set(), // "flowId/stepIndex"
  search: "",
  obFilter: { proven: true, disproven: true, unknown: true },
  obExpanded: new Set(),
  fitted: new Set(),     // route keys already auto-fitted
};

/* ================= pan / zoom ================= */

const view = {
  vb: { x: 0, y: 0, w: 1000, h: 700 },
  apply() {
    svg.setAttribute(
      "viewBox",
      `${this.vb.x} ${this.vb.y} ${this.vb.w} ${this.vb.h}`
    );
  },
  fit(pad) {
    const scene = svg.querySelector("#scene");
    if (!scene) return;
    let b;
    try { b = scene.getBBox(); } catch { return; }
    if (!b || (b.width === 0 && b.height === 0)) return;
    pad = pad === undefined ? 60 : pad;
    const rect = svg.getBoundingClientRect();
    // Before first layout the rect is 0x0; fall back to a sane aspect.
    const aspect = rect.width > 1 && rect.height > 1
      ? rect.width / rect.height
      : 16 / 9;
    let w = b.width + pad * 2;
    let h = b.height + pad * 2;
    if (w / h < aspect) w = h * aspect; else h = w / aspect;
    this.vb = {
      x: b.x + b.width / 2 - w / 2,
      y: b.y + b.height / 2 - h / 2,
      w, h,
    };
    this.apply();
  },
  clientToWorld(cx, cy) {
    const rect = svg.getBoundingClientRect();
    return {
      x: this.vb.x + ((cx - rect.left) / rect.width) * this.vb.w,
      y: this.vb.y + ((cy - rect.top) / rect.height) * this.vb.h,
    };
  },
};

svg.addEventListener("wheel", (e) => {
  e.preventDefault();
  const factor = Math.exp(e.deltaY * 0.0015);
  const next = Math.min(50000, Math.max(120, view.vb.w * factor));
  const real = next / view.vb.w;
  const p = view.clientToWorld(e.clientX, e.clientY);
  view.vb = {
    x: p.x - (p.x - view.vb.x) * real,
    y: p.y - (p.y - view.vb.y) * real,
    w: view.vb.w * real,
    h: view.vb.h * real,
  };
  view.apply();
}, { passive: false });

let drag = null;
let suppressClick = false;

svg.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  drag = {
    sx: e.clientX, sy: e.clientY, vb: { ...view.vb },
    moved: false, pointerId: e.pointerId,
  };
});

svg.addEventListener("pointermove", (e) => {
  if (!drag) return;
  if (!drag.moved) {
    if (Math.abs(e.clientX - drag.sx) + Math.abs(e.clientY - drag.sy) <= 5) {
      return;
    }
    // Capture only once panning actually starts; capturing on
    // pointerdown would retarget the click event away from nodes.
    drag.moved = true;
    svg.setPointerCapture(drag.pointerId);
    svg.classList.add("panning");
  }
  const rect = svg.getBoundingClientRect();
  const dx = ((e.clientX - drag.sx) / rect.width) * drag.vb.w;
  const dy = ((e.clientY - drag.sy) / rect.height) * drag.vb.h;
  view.vb = { ...drag.vb, x: drag.vb.x - dx, y: drag.vb.y - dy };
  view.apply();
});

svg.addEventListener("pointerup", () => {
  if (drag && drag.moved) {
    suppressClick = true;
    setTimeout(() => { suppressClick = false; }, 0);
  }
  drag = null;
  svg.classList.remove("panning");
});

/* ================= routing ================= */

function parseHash() {
  const h = decodeURIComponent(location.hash || "");
  let m;
  if ((m = h.match(/^#\/op\/(.+)$/))) return { view: "op", id: m[1] };
  if ((m = h.match(/^#\/machine\/([^?]+)(?:\?t=(.+))?$/))) {
    return { view: "machine", id: m[1], highlight: m[2] || null };
  }
  return { view: "system" };
}

function navigate(hash) {
  if (location.hash === hash) render();
  else location.hash = hash;
}

window.addEventListener("hashchange", () => {
  state.route = parseHash();
  state.selection = null;
  render();
});

/* ================= selection & clicks ================= */

let pendingSelection = null; // applied on next render (cross-view focus)

function select(key, detail) {
  state.selection = key;
  if (detail) showDetail(detail.kind, detail.render);
  render();
}

function clearSelection() {
  state.selection = null;
  render();
}

svg.addEventListener("click", (e) => {
  if (suppressClick) return;
  const nav = e.target.closest("[data-nav]");
  if (nav) { navigate(nav.getAttribute("data-nav")); return; }
  const act = e.target.closest("[data-act]");
  if (act) { handleAction(JSON.parse(act.getAttribute("data-act"))); return; }
  const sel = e.target.closest("[data-sel]");
  if (sel) { handleSelect(JSON.parse(sel.getAttribute("data-sel"))); return; }
  if (state.selection) clearSelection();
});

svg.addEventListener("dblclick", (e) => {
  const dbl = e.target.closest("[data-dbl]");
  if (dbl) navigate(dbl.getAttribute("data-dbl"));
});

function handleAction(act) {
  if (act.type === "toggle-tx") {
    if (state.expandedTx.has(act.key)) state.expandedTx.delete(act.key);
    else state.expandedTx.add(act.key);
    render();
  }
}

function handleSelect(sel) {
  state.selection = sel.key;
  showDetailById(sel.id, sel);
  render();
}

/* Clicks inside HTML panels (detail + obligations). */
document.addEventListener("click", (e) => {
  if (e.target.closest("#canvas")) return;
  const nav = e.target.closest("[data-nav]");
  if (nav) {
    e.preventDefault();
    const selKey = nav.getAttribute("data-selkey");
    if (selKey) pendingSelection = selKey;
    navigate(nav.getAttribute("data-nav"));
    return;
  }
  const link = e.target.closest("[data-id]");
  if (link) {
    e.preventDefault();
    showDetailById(link.getAttribute("data-id"), {});
  }
});

/* ================= shell chrome ================= */

document.getElementById("page-title").textContent = DATA.title;
document.getElementById("page-rev").textContent = "rev " + MODEL.revision;
document.title = DATA.title + " · archspec";

document.getElementById("fit-btn").addEventListener("click", () => view.fit());

document.getElementById("detail-close").addEventListener("click", () => {
  detailPane.hidden = true;
  clearSelection();
});

searchBox.addEventListener("input", () => {
  state.search = searchBox.value.trim().toLowerCase();
  render();
});

const overlayBtn = document.getElementById("overlay-toggle");
const obBtn = document.getElementById("obligations-toggle");

if (REPORT) {
  overlayBtn.hidden = false;
  obBtn.hidden = false;
  overlayBtn.classList.toggle("active", overlayOn);
  const counts = statusCounts(REPORT.obligations);
  obBtn.textContent = "obligations " + (statusChipText(counts) || "0");

  overlayBtn.addEventListener("click", () => {
    overlayOn = !overlayOn;
    overlayBtn.classList.toggle("active", overlayOn);
    render();
  });
  obBtn.addEventListener("click", () => {
    obPane.hidden = !obPane.hidden;
    obBtn.classList.toggle("active", !obPane.hidden);
    if (!obPane.hidden) renderObligations();
  });
  document.getElementById("obligations-close").addEventListener("click", () => {
    obPane.hidden = true;
    obBtn.classList.remove("active");
  });

  if (REPORT.model_revision !== null &&
      REPORT.model_revision !== undefined &&
      REPORT.model_revision !== MODEL.revision) {
    document.getElementById("page-rev").textContent +=
      ` · report@rev ${REPORT.model_revision} (mismatch)`;
  }
}

function renderCrumbs() {
  crumbs.replaceChildren();
  const here = (t) => H("span", { class: "here" }, t);
  const link = (t, hash) => H("a", { "data-nav": hash }, t);
  const sep = () => H("span", { class: "sep" }, "/");

  if (state.route.view === "system") {
    crumbs.append(here("system"));
    const machines = Object.keys(MODEL.state_machines);
    if (machines.length) {
      crumbs.append(H("span", { class: "sep" }, "·"));
      crumbs.append(H("span", {}, "machines:"));
      for (const m of machines) {
        crumbs.append(link(shortId(m), "#/machine/" + encodeURIComponent(m)));
      }
    }
  } else if (state.route.view === "op") {
    crumbs.append(link("system", "#/system"), sep(),
      H("span", {}, "operation"), sep(), here(state.route.id));
  } else if (state.route.view === "machine") {
    crumbs.append(link("system", "#/system"), sep(),
      H("span", {}, "machine"), sep(), here(state.route.id));
  }
}

/* ================= render dispatch ================= */

function routeKey() {
  const r = state.route;
  return r.view + (r.id ? ":" + r.id : "");
}

function render() {
  renderCrumbs();
  emptyNote.hidden = true;

  if (pendingSelection) {
    state.selection = pendingSelection;
    pendingSelection = null;
  }

  const scene = S("g", { id: "scene" });
  let legend;

  if (state.route.view === "op") {
    legend = renderOperationView(scene);
  } else if (state.route.view === "machine") {
    legend = renderMachineView(scene);
  } else {
    legend = renderSystemView(scene);
  }

  svg.replaceChildren(defs(), scene);
  renderLegend(legend);

  const key = routeKey();
  if (!state.fitted.has(key)) {
    state.fitted.add(key);
    view.fit();
  } else {
    view.apply();
  }
}

function defs() {
  const marker = (id, color) =>
    S("marker", {
      id, markerWidth: 9, markerHeight: 7, refX: 8, refY: 3.5,
      orient: "auto", markerUnits: "userSpaceOnUse",
    }, S("path", { d: "M0,0 L9,3.5 L0,7 Z", fill: color }));

  const css = getComputedStyle(document.documentElement);
  const c = (v) => css.getPropertyValue(v).trim();

  return S("defs", {},
    marker("arr-publish", c("--edge-publish")),
    marker("arr-subscribe", c("--edge-subscribe")),
    marker("arr-request", c("--edge-request")),
    marker("arr-external", c("--edge-external")),
    marker("arr-client", c("--edge-client")),
    marker("arr-neutral", c("--text-dim")),
    marker("arr-accent", c("--accent")),
    marker("arr-faint", c("--text-faint")),
  );
}

function renderLegend(rows) {
  legendBox.replaceChildren();
  for (const row of rows || []) legendBox.append(row);
}

function legendLine(color, label, dashed) {
  return H("div", { class: "row" },
    H("span", {
      class: "swatch" + (dashed ? " dashed" : ""),
      style: "border-color:" + color,
    }),
    H("span", {}, label));
}

function legendChip(varName, label) {
  const css = getComputedStyle(document.documentElement);
  const color = css.getPropertyValue(varName).trim();
  return H("div", { class: "row" },
    H("span", { class: "chip", style: `border-color:${color};background:color-mix(in srgb, ${color} 25%, transparent)` }),
    H("span", {}, label));
}

/* ================= status decoration helpers ================= */

function statusRing(x, y, w, h, rx, key) {
  const obs = obligationsAt(key);
  if (!obs.length) return null;
  return S("rect", {
    class: "status-ring " + worstStatus(obs),
    x: x - 3, y: y - 3, width: w + 6, height: h + 6, rx: rx + 3,
  });
}

function statusChip(x, y, key) {
  const obs = obligationsAt(key);
  if (!obs.length) return null;
  const text = statusChipText(statusCounts(obs));
  const w = text.length * 6.4 + 12;
  const css = getComputedStyle(document.documentElement);
  const color = css.getPropertyValue("--" + worstStatus(obs)).trim();
  return S("g", { class: "status-chip" },
    S("rect", {
      x: x - w, y: y - 9, width: w, height: 18, rx: 9,
      fill: "var(--bg-panel)", stroke: color, "stroke-width": 1.2,
    }),
    S("text", {
      x: x - w / 2, y: y + 3.5, "text-anchor": "middle", fill: color,
    }, text));
}

/* ================= system view ================= */

const SYS = {
  OP_W: 200, OP_H: 62, OP_VGAP: 16,
  SVC_PAD: 16, SVC_TITLE: 34, SVC_GAP: 100,
  TOPIC_W: 200, TOPIC_H: 48, TOPIC_BAND: 150, TOPIC_GAP: 60,
  EXT_W: 200, EXT_H: 46, EXT_BAND: 130,
  CLIENT_W: 160, CLIENT_H: 58, CLIENT_GAP: 130,
};

function serviceOrder() {
  // Service-level reachability: requests directly, pub/sub through
  // topics. Entry services are those with client-facing inputs.
  const ids = GRAPH.services.map((s) => s.id);
  const opService = new Map(GRAPH.operations.map((o) => [o.id, o.service]));
  const succ = new Map(ids.map((id) => [id, new Set()]));
  const pubs = new Map();  // topic -> Set(service)
  const subs = new Map();

  const entries = new Set();
  for (const e of GRAPH.edges) {
    if (e.kind === "request") {
      const a = opService.get(e.operation), b = opService.get(
        (GRAPH.operations.find((o) => o.id === e.to) || {}).id);
      if (a && b && a !== b) succ.get(a)?.add(b);
    } else if (e.kind === "publish") {
      const a = opService.get(e.operation);
      if (a) (pubs.get(e.to) || pubs.set(e.to, new Set()).get(e.to)).add(a);
    } else if (e.kind === "subscribe") {
      const b = opService.get(e.operation);
      if (b) (subs.get(e.from) || subs.set(e.from, new Set()).get(e.from)).add(b);
    } else if (e.kind === "client") {
      const b = opService.get(e.operation);
      if (b) entries.add(b);
    }
  }
  for (const [topic, ps] of pubs) {
    for (const p of ps) for (const s of subs.get(topic) || []) {
      if (p !== s) succ.get(p)?.add(s);
    }
  }

  let roots = [...entries];
  if (!roots.length) {
    const indeg = new Map(ids.map((id) => [id, 0]));
    for (const [, ss] of succ) for (const s of ss) {
      indeg.set(s, (indeg.get(s) || 0) + 1);
    }
    roots = ids.filter((id) => !indeg.get(id));
  }
  if (!roots.length) roots = ids.slice(0, 1);

  const rank = new Map();
  let queue = roots.map((id) => [id, 0]);
  while (queue.length) {
    const [id, d] = queue.shift();
    if (rank.has(id)) continue;
    rank.set(id, d);
    for (const s of succ.get(id) || []) queue.push([s, d + 1]);
  }
  const maxRank = Math.max(0, ...rank.values());
  for (const id of ids) if (!rank.has(id)) rank.set(id, maxRank + 1);

  return [...ids].sort((a, b) =>
    rank.get(a) - rank.get(b) || a.localeCompare(b));
}

function layoutSystem() {
  const pos = new Map(); // node id -> {x,y,w,h}
  const svcBoxes = [];
  const byService = new Map(GRAPH.services.map((s) => [s.id, []]));
  for (const op of GRAPH.operations) {
    if (!byService.has(op.service)) byService.set(op.service, []);
    byService.get(op.service).push(op);
  }

  let x = 0;
  for (const svcId of serviceOrder()) {
    const ops = byService.get(svcId) || [];
    const w = SYS.OP_W + SYS.SVC_PAD * 2;
    const h = SYS.SVC_TITLE + SYS.SVC_PAD +
      ops.length * SYS.OP_H +
      Math.max(0, ops.length - 1) * SYS.OP_VGAP;
    svcBoxes.push({ id: svcId, x, y: 0, w, h: Math.max(h, 70) });
    ops.forEach((op, i) => {
      pos.set(op.id, {
        x: x + SYS.SVC_PAD,
        y: SYS.SVC_TITLE + i * (SYS.OP_H + SYS.OP_VGAP),
        w: SYS.OP_W, h: SYS.OP_H,
      });
    });
    x += w + SYS.SVC_GAP;
  }

  const maxBottom = Math.max(80, ...svcBoxes.map((b) => b.y + b.h));

  // Band placement shared by topics and externals: desired x is the
  // mean of connected operation centers; overlaps resolved in order.
  function placeBand(nodes, connectedOps, w, gap, y, h) {
    const items = nodes.map((n) => {
      const ops = connectedOps(n).map((id) => pos.get(id)).filter(Boolean);
      const cx = ops.length
        ? ops.reduce((a, p) => a + p.x + p.w / 2, 0) / ops.length
        : 0;
      return { n, cx };
    }).sort((a, b) => a.cx - b.cx || a.n.id.localeCompare(b.n.id));
    let cursor = -Infinity;
    for (const item of items) {
      const left = Math.max(item.cx - w / 2, cursor);
      pos.set(item.n.id, { x: left, y, w, h });
      cursor = left + w + gap;
    }
  }

  placeBand(
    GRAPH.topics,
    (t) => GRAPH.edges
      .filter((e) => (e.kind === "publish" && e.to === t.id) ||
                     (e.kind === "subscribe" && e.from === t.id))
      .map((e) => e.operation),
    SYS.TOPIC_W, SYS.TOPIC_GAP, maxBottom + SYS.TOPIC_BAND, SYS.TOPIC_H);

  placeBand(
    GRAPH.externals,
    (x_) => GRAPH.edges
      .filter((e) => e.kind === "external" && e.to === x_.id)
      .map((e) => e.operation),
    SYS.EXT_W, SYS.TOPIC_GAP, -(SYS.EXT_BAND + SYS.EXT_H), SYS.EXT_H);

  if (GRAPH.client) {
    const targets = GRAPH.edges
      .filter((e) => e.kind === "client")
      .map((e) => pos.get(e.to)).filter(Boolean);
    const cy = targets.length
      ? targets.reduce((a, p) => a + p.y + p.h / 2, 0) / targets.length
      : 100;
    const minX = Math.min(0, ...svcBoxes.map((b) => b.x));
    pos.set(GRAPH.client.id, {
      x: minX - SYS.CLIENT_GAP - SYS.CLIENT_W,
      y: cy - SYS.CLIENT_H / 2,
      w: SYS.CLIENT_W, h: SYS.CLIENT_H,
    });
  }

  return { pos, svcBoxes };
}

/* Assign ports along a node side for a set of edges. */
function assignPorts(edges, node, side, sortBy) {
  const sorted = [...edges].sort((a, b) => sortBy(a) - sortBy(b));
  const n = sorted.length;
  const inset = Math.min(26, node.w / (n + 1));
  const span = node.w - inset * 2;
  const ports = new Map();
  sorted.forEach((e, i) => {
    const frac = n === 1 ? 0.5 : i / (n - 1);
    ports.set(e.id, {
      x: node.x + inset + frac * span,
      y: side === "top" ? node.y : node.y + node.h,
    });
  });
  return ports;
}

function edgeMatchesSearch(e, matchFn) {
  return matchFn(e.from) || matchFn(e.to);
}

function renderSystemView(scene) {
  const { pos, svcBoxes } = layoutSystem();
  const q = state.search;
  const matches = (id) => !q ||
    id.toLowerCase().includes(q) || shortId(id).toLowerCase().includes(q);

  // Related set for selection dimming.
  const sel = state.selection;
  const related = new Set();
  if (sel) {
    related.add(sel);
    for (const e of GRAPH.edges) {
      if (e.id === sel || e.from === sel || e.to === sel) {
        related.add(e.id);
        related.add(e.from);
        related.add(e.to);
      }
    }
  }
  const isDim = (key) => {
    if (q && key !== undefined && !matches(key)) return true;
    if (sel && !related.has(key)) return true;
    return false;
  };

  // ---- ports
  const portOf = new Map(); // edge id -> {from:{x,y}, to:{x,y}}
  const vertEdges = GRAPH.edges.filter((e) =>
    e.kind === "publish" || e.kind === "subscribe" || e.kind === "external");

  // op-side bottom ports (topics below), top ports (externals above)
  for (const op of GRAPH.operations) {
    const p = pos.get(op.id);
    if (!p) continue;
    const bottom = vertEdges.filter((e) =>
      (e.kind !== "external") && (e.from === op.id || e.to === op.id));
    const bPorts = assignPorts(bottom, p, "bottom", (e) => {
      const other = pos.get(e.kind === "publish" ? e.to : e.from);
      return other ? other.x : 0;
    });
    const top = vertEdges.filter((e) =>
      e.kind === "external" && e.from === op.id);
    const tPorts = assignPorts(top, p, "top", (e) => {
      const other = pos.get(e.to); return other ? other.x : 0;
    });
    for (const [id, pt] of bPorts) {
      const rec = portOf.get(id) || {};
      const e = GRAPH.edges.find((x) => x.id === id);
      if (e.kind === "publish") rec.from = pt; else rec.to = pt;
      portOf.set(id, rec);
    }
    for (const [id, pt] of tPorts) {
      const rec = portOf.get(id) || {};
      rec.from = pt;
      portOf.set(id, rec);
    }
  }

  for (const t of GRAPH.topics) {
    const p = pos.get(t.id);
    if (!p) continue;
    const es = vertEdges.filter((e) => e.from === t.id || e.to === t.id);
    const ports = assignPorts(es, p, "top", (e) => {
      const other = pos.get(e.kind === "publish" ? e.operation : e.to);
      return other ? other.x : 0;
    });
    for (const [id, pt] of ports) {
      const rec = portOf.get(id) || {};
      const e = GRAPH.edges.find((x) => x.id === id);
      if (e.kind === "publish") rec.to = pt; else rec.from = pt;
      portOf.set(id, rec);
    }
  }

  for (const ext of GRAPH.externals) {
    const p = pos.get(ext.id);
    if (!p) continue;
    const es = vertEdges.filter((e) => e.to === ext.id);
    const ports = assignPorts(es, p, "bottom", (e) => {
      const other = pos.get(e.operation); return other ? other.x : 0;
    });
    ports.forEach((pt, id) => {
      const rec = portOf.get(id) || {};
      rec.to = { x: pt.x, y: p.y + p.h };
      portOf.set(id, rec);
    });
  }

  // ---- service boxes
  for (const box of svcBoxes) {
    const svc = GRAPH.services.find((s) => s.id === box.id);
    scene.append(S("g", {
      "data-sel": JSON.stringify({ key: box.id, id: box.id }),
      class: "node service" + (isDim(box.id) ? " dimmed" : ""),
    },
      S("rect", { class: "service-box", x: box.x, y: box.y, width: box.w, height: box.h, rx: 10 }),
      S("text", { class: "service-label", x: box.x + 12, y: box.y + 21 },
        trunc(shortId(box.id), 22)),
      S("text", { class: "service-kind", x: box.x + box.w - 12, y: box.y + 21, "text-anchor": "end" },
        svc ? svc.kind : ""),
      S("title", {}, box.id + (svc ? " (" + svc.kind + ")" : "")),
    ));
  }

  // ---- edges
  for (const e of GRAPH.edges) {
    const rec = portOf.get(e.id) || {};
    let p1 = rec.from, p2 = rec.to;
    const a = pos.get(e.from), b = pos.get(e.to);
    if (!a || !b) continue;

    let d;
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

    const unexecuted = ("executed_by" in e) && e.executed_by &&
      e.executed_by.length === 0;
    const cls = ["edge", e.kind];
    if (unexecuted) cls.push("unexecuted");
    if (isDim(e.id) && !(q && edgeMatchesSearch(e, matches) && !sel)) {
      if (sel ? !related.has(e.id) : true) cls.push("dimmed");
    }
    if (sel === e.id) cls.push("selected");

    const g = S("g", { "data-sel": JSON.stringify({ key: e.id, id: e.id, edge: true }) },
      S("path", { class: cls.join(" "), d, "marker-end": `url(#arr-${e.kind})` }),
      S("path", { class: "edge-hit", d }),
      S("title", {}, edgeTooltip(e)),
    );

    if (sel === e.id) {
      const mid = midpointOf(p1, p2);
      g.append(S("text", {
        class: "edge-label", x: mid.x + 8, y: mid.y - 6,
      }, edgeShortLabel(e)));
    }
    scene.append(g);
  }

  // ---- operation nodes
  for (const op of GRAPH.operations) {
    const p = pos.get(op.id);
    if (!p) continue;
    const cls = ["node", "operation"];
    if (isDim(op.id)) cls.push("dimmed");
    if (sel === op.id) cls.push("selected");

    const badges = [];
    const r = op.requirements;
    if (r.serialization) badges.push("S" + r.serialization);
    if (r.ordering) badges.push("O" + r.ordering);
    if (r.idempotency) badges.push("I" + r.idempotency);
    if (r.recoverability) badges.push("R" + r.recoverability);
    if (op.machines.length) badges.push("SM");

    const g = S("g", {
      class: cls.join(" "),
      "data-sel": JSON.stringify({ key: op.id, id: op.id }),
      "data-dbl": "#/op/" + encodeURIComponent(op.id),
    },
      statusRing(p.x, p.y, p.w, p.h, 8, op.id),
      S("rect", { class: "body", x: p.x, y: p.y, width: p.w, height: p.h, rx: 8 }),
      S("text", { class: "title", x: p.x + 10, y: p.y + 20 },
        trunc(shortId(op.id), 24)),
      S("text", { class: "subtitle", x: p.x + 10, y: p.y + 36 },
        `${op.flows} flow${op.flows === 1 ? "" : "s"} · ${op.inputs} input${op.inputs === 1 ? "" : "s"}`),
      S("text", { class: "badge-text", x: p.x + 10, y: p.y + 52 },
        badges.join("  ")),
      S("title", {}, op.id + (op.description ? "\n" + op.description : "") +
        "\n(double-click to open flows)"),
      statusChip(p.x + p.w - 6, p.y, op.id),
    );
    scene.append(g);
  }

  // ---- topic nodes
  for (const t of GRAPH.topics) {
    const p = pos.get(t.id);
    if (!p) continue;
    const cls = ["node", "topic"];
    if (isDim(t.id)) cls.push("dimmed");
    if (sel === t.id) cls.push("selected");
    scene.append(S("g", {
      class: cls.join(" "),
      "data-sel": JSON.stringify({ key: t.id, id: t.id }),
    },
      statusRing(p.x, p.y, p.w, p.h, 24, t.id),
      S("rect", { class: "body", x: p.x, y: p.y, width: p.w, height: p.h, rx: 24 }),
      S("text", { class: "title", x: p.x + p.w / 2, y: p.y + 20, "text-anchor": "middle" },
        trunc(shortId(t.id), 24)),
      S("text", { class: "subtitle", x: p.x + p.w / 2, y: p.y + 36, "text-anchor": "middle" },
        "topic · " + t.ordering),
      S("title", {}, t.id + "\nordering: " + t.ordering +
        "\nmessages: " + t.messages.map(shortId).join(", ")),
      statusChip(p.x + p.w - 6, p.y, t.id),
    ));
  }

  // ---- external nodes
  for (const ext of GRAPH.externals) {
    const p = pos.get(ext.id);
    if (!p) continue;
    const cls = ["node", "external"];
    if (isDim(ext.id)) cls.push("dimmed");
    if (sel === ext.id) cls.push("selected");
    scene.append(S("g", {
      class: cls.join(" "),
      "data-sel": JSON.stringify({ key: ext.id, id: ext.id }),
    },
      S("rect", { class: "body", x: p.x, y: p.y, width: p.w, height: p.h, rx: 6 }),
      S("text", { class: "title", x: p.x + p.w / 2, y: p.y + 19, "text-anchor": "middle" },
        trunc(ext.name, 24)),
      S("text", { class: "subtitle", x: p.x + p.w / 2, y: p.y + 34, "text-anchor": "middle" },
        "external system"),
      S("title", {}, "external: " + ext.name + "\nthe modeled system ends here"),
    ));
  }

  // ---- client node
  if (GRAPH.client) {
    const p = pos.get(GRAPH.client.id);
    const cls = ["node", "client"];
    if (isDim(GRAPH.client.id)) cls.push("dimmed");
    if (sel === GRAPH.client.id) cls.push("selected");
    scene.append(S("g", {
      class: cls.join(" "),
      "data-sel": JSON.stringify({ key: GRAPH.client.id, id: GRAPH.client.id }),
    },
      S("rect", { class: "body", x: p.x, y: p.y, width: p.w, height: p.h, rx: 10 }),
      S("text", { class: "title", x: p.x + p.w / 2, y: p.y + 24, "text-anchor": "middle" },
        "clients"),
      S("text", { class: "subtitle", x: p.x + p.w / 2, y: p.y + 40, "text-anchor": "middle" },
        "unmodeled callers"),
      S("title", {}, "request inputs no modeled operation invokes"),
    ));
  }

  if (!GRAPH.operations.length && !GRAPH.topics.length) {
    emptyNote.hidden = false;
    emptyNote.textContent = "model declares no operations or topics";
  }

  const css = getComputedStyle(document.documentElement);
  const c = (v) => css.getPropertyValue(v).trim();
  const rows = [
    legendLine(c("--edge-publish"), "publication"),
    legendLine(c("--edge-subscribe"), "subscription"),
  ];
  if (GRAPH.edges.some((e) => e.kind === "request")) {
    rows.push(legendLine(c("--edge-request"), "request"));
  }
  if (GRAPH.externals.length) {
    rows.push(legendLine(c("--edge-external"), "external effect"));
  }
  if (GRAPH.client) rows.push(legendLine(c("--edge-client"), "client request"));
  rows.push(legendLine(c("--text-dim"), "declared, unexecuted", true));
  if (REPORT && overlayOn) {
    rows.push(legendChip("--proven", "proven"));
    rows.push(legendChip("--disproven", "disproven"));
    rows.push(legendChip("--unknown", "unknown"));
  }
  return rows;
}

function midpointOf(p1, p2) {
  return { x: (p1.x + p2.x) / 2, y: (p1.y + p2.y) / 2 };
}

function edgeShortLabel(e) {
  switch (e.kind) {
    case "publish": return shortId(e.schema);
    case "subscribe": return e.schemas.map(shortId).join(", ");
    case "request": return shortId(e.schema);
    case "client": return shortId(e.schema);
    case "external": return "external";
    default: return "";
  }
}

function edgeTooltip(e) {
  switch (e.kind) {
    case "publish":
      return `publication ${e.effect}\n${e.schema} → ${e.to}` +
        (e.via_transition ? `\nvia transition ${e.via_transition.transition}` : "") +
        (e.executed_by.length ? "" : "\ndeclared but not executed by any flow");
    case "subscribe":
      return `subscription ${e.input}\n${e.schemas.join(", ")}\n` +
        `delivery: ${e.delivery} · routing: ${e.routing} · lane: ${e.lane_concurrency}`;
    case "request":
      return `request ${e.effect} → ${e.to} (${e.input})\nretry: ${e.retry}` +
        (e.via_transition ? `\nvia transition ${e.via_transition.transition}` : "") +
        (e.executed_by.length ? "" : "\ndeclared but not executed by any flow");
    case "external":
      return `external effect ${e.effect}\nidempotency: ${e.idempotency}`;
    case "client":
      return `request input ${e.input}\nschema: ${e.schema}`;
    default: return e.id;
  }
}

/* ================= operation view ================= */

const OPV = {
  COL_W: 300, COL_GAP: 48, CARD_H: 58, NEST_H: 56,
  GAP: 16, NEST_INDENT: 18, HEADER_GAP: 40,
};

function renderOperationView(scene) {
  const opId = state.route.id;
  const op = MODEL.operations[opId];
  if (!op) {
    emptyNote.hidden = false;
    emptyNote.textContent = "unknown operation " + opId;
    return [];
  }

  const sel = state.selection;
  let y = 0;

  // ---- header
  scene.append(S("text", {
    x: 0, y, "font-size": 17, "font-weight": 700, fill: "var(--text)",
  }, shortId(opId)));
  scene.append(S("text", {
    x: 0, y: y + 20, "font-size": 11, fill: "var(--text-dim)",
  }, opId + "  ·  service: " + op.service +
     "  ·  concurrency: " + concurrencyText(op.execution.concurrency)));
  y += 34;

  if (op.description) {
    for (const line of wrapText(op.description, 96)) {
      scene.append(S("text", {
        x: 0, y, "font-size": 12, fill: "var(--text-dim)",
        "font-family": "var(--sans)",
      }, line));
      y += 17;
    }
  }
  y += 6;

  // ---- requirement chips
  const chips = [];
  const reqs = op.requirements;
  const addChip = (prop, i, label) => chips.push({ prop, i, label });
  reqs.serialization.forEach((_, i) => addChip("serialization", i, "serialization #" + i));
  reqs.ordering.forEach((_, i) => addChip("ordering", i, "ordering #" + i));
  reqs.idempotency.forEach((_, i) => addChip("idempotency", i, "idempotency #" + i));
  reqs.recoverability.forEach((rr, i) =>
    addChip("recoverability", i, "recoverability #" + i + " (" + rr.completion + ")"));

  let cx = 0;
  for (const chip of chips) {
    const obs = obligationsAt(opId).filter((ob) =>
      ob.subject.kind === "operation" &&
      ob.subject.requirement === chip.i &&
      obPropertyMatches(ob.property, chip.prop));
    const status = obs.length ? worstStatus(obs) : null;
    const text = chip.label + (status ? " " + STATUS_GLYPH[status] : "");
    const w = text.length * 6.6 + 18;
    const key = `req:${chip.prop}:${chip.i}`;
    const color = status ? `var(--${status})` : "var(--line)";
    scene.append(S("g", {
      class: "step-card" + (sel === key ? " selected" : ""),
      "data-sel": JSON.stringify({
        key, id: opId, req: { prop: chip.prop, index: chip.i },
      }),
    },
      S("rect", {
        x: cx, y, width: w, height: 24, rx: 12,
        fill: "var(--bg-raised)", stroke: color, "stroke-width": 1.2,
      }),
      S("text", {
        x: cx + w / 2, y: y + 16, "text-anchor": "middle",
        "font-size": 11, fill: status ? `var(--${status})` : "var(--text-dim)",
      }, text),
    ));
    cx += w + 10;
  }
  if (chips.length) y += 40;

  // ---- inputs
  const inputIds = Object.keys(op.inputs);
  if (inputIds.length) {
    scene.append(S("text", {
      x: 0, y: y + 12, "font-size": 10, fill: "var(--text-faint)",
      "letter-spacing": "0.09em",
    }, "INPUTS"));
    y += 20;
    let ix = 0;
    for (const inputId of inputIds) {
      const input = op.inputs[inputId];
      const note = input.kind === "request"
        ? "request · " + shortId(input.schema)
        : "⇦ " + shortId(input.topic) + " · " + input.delivery +
          " · " + input.dispatch.routing;
      const w = 270;
      scene.append(stepCard({
        x: ix, y, w, h: 52, cls: "effect",
        kind: input.kind === "request" ? "request input" : "subscription",
        title: shortId(inputId), note,
        selKey: "in:" + inputId, selId: inputId,
      }));
      ix += w + 16;
    }
    y += 52 + OPV.HEADER_GAP;
  } else {
    y += OPV.HEADER_GAP / 2;
  }

  // ---- flows
  const flowIds = Object.keys(op.flows);
  if (!flowIds.length) {
    emptyNote.hidden = false;
    emptyNote.textContent = "operation declares no flows";
  }

  let fx = 0;
  const flowTop = y;
  for (const flowId of flowIds) {
    const flow = op.flows[flowId];
    let fy = flowTop;

    const flowObs = obligationsAt(opId + "/" + flowId);
    scene.append(S("g", {
      class: "step-card",
      "data-sel": JSON.stringify({ key: "flow:" + flowId, id: flowId }),
    },
      S("text", { class: "flow-col-title", x: fx, y: fy },
        "flow: " + shortId(flowId) +
        (flowObs.length ? "  " + statusChipText(statusCounts(flowObs)) : "")),
    ));
    fy += 16;

    const cards = [];
    flow.steps.forEach((step, si) => {
      const expandKey = flowId + "/" + si;
      if (step.kind === "transaction") {
        const tx = op.transactions[step.transaction];
        const expanded = state.expandedTx.has(expandKey);
        cards.push({
          type: "tx", cls: "tx", kind: "transaction",
          title: shortId(step.transaction),
          note: tx
            ? `${tx.steps.length} steps · ${tx.isolation} · ${tx.idempotency.kind}`
            : "unresolved",
          selKey: "tx:" + step.transaction, selId: step.transaction,
          statusKey: opId + "/" + step.transaction,
          expandKey, expanded,
          hint: tx ? (expanded ? "▾ collapse" : "▸ expand " + tx.steps.length + " steps") : null,
        });
        if (expanded && tx) {
          tx.steps.forEach((ts, ti) => {
            cards.push(txStepCard(ts, ti, step.transaction, opId));
          });
        }
      } else if (step.kind === "execute_effect") {
        cards.push({
          type: "step", cls: "effect", kind: "execute effect",
          title: shortId(step.effect),
          note: effectSummary(step.effect),
          selKey: "fx:" + si + ":" + step.effect, selId: step.effect,
        });
      } else if (step.kind === "execute_effect_intent") {
        const intent = op.effect_intents[step.intent];
        const eff = intent ? intent.effect : null;
        const owner = eff ? IX.get(eff) : null;
        const viaTransition = owner && owner.machine;
        cards.push({
          type: "step", cls: "intent", kind: "execute effect intent",
          title: shortId(step.intent),
          note: (eff ? effectSummary(eff) : "unresolved") +
            (viaTransition ? " · via transition" : ""),
          selKey: "fi:" + si + ":" + step.intent, selId: step.intent,
        });
      }
    });

    if (flow.response) {
      const resp = op.responses[flow.response];
      cards.push({
        type: "step", cls: "response", kind: "terminal response",
        title: shortId(flow.response),
        note: resp ? shortId(resp.schema) + " · source: " + resp.source.kind : "",
        selKey: "resp:" + flow.response, selId: flow.response,
      });
    }

    let prevBottom = null;
    for (const card of cards) {
      const nested = card.type === "nested";
      const x = fx + (nested ? OPV.NEST_INDENT : 0);
      const w = OPV.COL_W - (nested ? OPV.NEST_INDENT : 0);
      const h = nested ? OPV.NEST_H : OPV.CARD_H;

      if (prevBottom !== null) {
        const gap = nested ? 8 : OPV.GAP;
        fy = prevBottom + gap;
        if (!nested) {
          scene.append(S("path", {
            class: "flow-arrow",
            d: `M${fx + OPV.COL_W / 2},${prevBottom + 2} L${fx + OPV.COL_W / 2},${fy - 2}`,
            "marker-end": "url(#arr-faint)",
          }));
        }
      }

      scene.append(stepCard({
        x, y: fy, w, h,
        cls: card.cls + (nested ? " txstep" : ""),
        kind: card.kind, title: card.title, note: card.note,
        selKey: card.selKey, selId: card.selId, selExtra: card.selExtra,
        statusKey: card.statusKey,
        hint: card.hint, expandKey: card.expandKey,
        navTo: card.navTo,
      }));

      prevBottom = fy + h;
    }

    fx += OPV.COL_W + OPV.COL_GAP;
  }

  return [
    legendLine("var(--edge-request)", "transaction (click to expand)"),
    legendLine("var(--edge-publish)", "effect execution"),
    legendLine("var(--edge-publish)", "effect-intent execution", true),
    legendLine("var(--edge-client)", "terminal response"),
  ];
}

function obPropertyMatches(property, prop) {
  if (property.kind === prop) return true;
  // response_replay obligations anchor to the idempotency requirement.
  if (prop === "idempotency" && property.kind === "response_replay") return true;
  return false;
}

function txStepCard(ts, ti, txId, opId) {
  const base = {
    type: "nested", cls: "txstep", kind: "tx step " + (ti + 1),
    selKey: `ts:${txId}:${ti}`,
    selId: txId,
    selExtra: { txStep: ti, tx: txId, op: opId },
  };
  switch (ts.kind) {
    case "read":
      return { ...base, kind: "read", title: shortId(ts.result),
        note: shortId(ts.target.object) + " where " + trunc(predText(ts.target.predicate), 34) };
    case "write":
      return { ...base, kind: "write", title: shortId(ts.target.object),
        note: ts.fields.map(pathText).join(", ") + " · " + ts.values.kind };
    case "insert":
      return { ...base, kind: "insert", title: shortId(ts.object),
        note: "values: " + ts.values.kind };
    case "delete":
      return { ...base, kind: "delete", title: shortId(ts.target.object),
        note: "where " + trunc(predText(ts.target.predicate), 36) };
    case "lock":
      return { ...base, kind: "lock", title: shortId(ts.target.object),
        note: ts.mode + " · order " + ts.order.kind };
    case "transition":
      return { ...base, cls: "txstep transition", kind: "state transition",
        title: shortId(ts.transition),
        note: shortId(ts.machine) + " · open machine ⇢",
        navTo: "#/machine/" + encodeURIComponent(ts.machine) +
          "?t=" + encodeURIComponent(ts.transition) };
    case "establish_effect_intent":
      return { ...base, kind: "establish intent", title: shortId(ts.intent),
        note: "values: " + ts.values.kind };
    case "establish_invocation_result":
      return { ...base, kind: "establish result", title: shortId(ts.result),
        note: "values: " + ts.values.kind };
    default:
      return { ...base, kind: ts.kind, title: "", note: "" };
  }
}

function stepCard(opts) {
  const sel = state.selection === opts.selKey;
  const g = S("g", {
    class: "step-card " + opts.cls + (sel ? " selected" : ""),
    "data-sel": JSON.stringify({
      key: opts.selKey, id: opts.selId, extra: opts.selExtra || null,
    }),
  });
  if (opts.statusKey) {
    const ring = statusRing(opts.x, opts.y, opts.w, opts.h, 8, opts.statusKey);
    if (ring) g.append(ring);
  }
  g.append(S("rect", {
    class: "body", x: opts.x, y: opts.y, width: opts.w, height: opts.h, rx: 8,
  }));
  g.append(S("text", { class: "kind", x: opts.x + 12, y: opts.y + 15 }, opts.kind));
  g.append(S("text", { class: "title", x: opts.x + 12, y: opts.y + 31 },
    trunc(opts.title, Math.floor((opts.w - 24) / 7.2))));
  if (opts.note) {
    g.append(S("text", { class: "note", x: opts.x + 12, y: opts.y + opts.h - 10 },
      trunc(opts.note, Math.floor((opts.w - 24) / 6))));
  }
  if (opts.hint && opts.expandKey) {
    g.append(S("text", {
      class: "expand-hint", x: opts.x + opts.w - 12, y: opts.y + 15,
      "text-anchor": "end",
      "data-act": JSON.stringify({ type: "toggle-tx", key: opts.expandKey }),
    }, opts.hint));
  }
  if (opts.navTo) {
    g.append(S("rect", {
      x: opts.x + opts.w - 26, y: opts.y + opts.h - 26, width: 22, height: 22,
      rx: 5, fill: "transparent",
      "data-nav": opts.navTo,
    }));
    g.append(S("text", {
      x: opts.x + opts.w - 15, y: opts.y + opts.h - 11, "text-anchor": "middle",
      class: "expand-hint", "data-nav": opts.navTo, "font-size": 12,
    }, "⇢"));
  }
  if (opts.statusKey) {
    const chip = statusChip(opts.x + opts.w - 6, opts.y, opts.statusKey);
    if (chip) g.append(chip);
  }
  return g;
}

/* ================= state machine view ================= */

function renderMachineView(scene) {
  const mId = state.route.id;
  const machine = MODEL.state_machines[mId];
  if (!machine) {
    emptyNote.hidden = false;
    emptyNote.textContent = "unknown state machine " + mId;
    return [];
  }

  const highlight = state.route.highlight;
  const sel = state.selection;

  // header
  scene.append(S("text", {
    x: 0, y: -70, "font-size": 17, "font-weight": 700, fill: "var(--text)",
  }, shortId(mId)));
  const subj = machine.subject;
  scene.append(S("text", {
    x: 0, y: -50, "font-size": 11, fill: "var(--text-dim)",
  }, `subject: ${subj.object} · state field: ${pathText(subj.state)}`));

  // BFS layering from initial state
  const succ = new Map(machine.states.map((s) => [s, new Set()]));
  for (const t of Object.values(machine.transitions)) {
    for (const f of t.from) succ.get(f)?.add(t.to);
  }
  const layerOf = new Map();
  let frontier = [machine.initial];
  let depth = 0;
  while (frontier.length) {
    const next = [];
    for (const s of frontier) {
      if (layerOf.has(s)) continue;
      layerOf.set(s, depth);
      for (const n of succ.get(s) || []) next.push(n);
    }
    frontier = next;
    depth += 1;
  }
  const maxLayer = Math.max(0, ...layerOf.values());
  for (const s of machine.states) {
    if (!layerOf.has(s)) layerOf.set(s, maxLayer + 1);
  }

  const layers = [];
  for (const [s, l] of layerOf) {
    (layers[l] = layers[l] || []).push(s);
  }
  layers.forEach((l) => l.sort());

  const XGAP = 230, YGAP = 140, H = 44;
  const pos = new Map();
  layers.forEach((states, li) => {
    states.forEach((s, i) => {
      const label = shortId(s);
      const w = Math.max(120, label.length * 7.4 + 28);
      pos.set(s, {
        x: (i - (states.length - 1) / 2) * XGAP - w / 2,
        y: li * YGAP, w, h: H, label,
      });
    });
  });

  // Rightmost node edge; upward bows route beyond it so they clear
  // every layer they pass.
  const rightmost = Math.max(
    0, ...[...pos.values()].map((p) => p.x + p.w));

  // transition edges: one per (transition, from-state)
  const pairSeen = new Map();
  for (const [tId, t] of Object.entries(machine.transitions)) {
    for (const from of t.from) {
      const a = pos.get(from), b = pos.get(t.to);
      if (!a || !b) continue;
      const pk = from + "→" + t.to;
      const n = pairSeen.get(pk) || 0;
      pairSeen.set(pk, n + 1);
      const offset = n * 26;

      const isSel = sel === "t:" + tId;
      const isHl = highlight === tId;
      const tObs = obligationsAt(mId + "/" + tId);
      const status = tObs.length ? worstStatus(tObs) : null;

      let d, lx, ly;
      if (from === t.to) {
        // Self loop on the right side; parallel loops nest outward and
        // their labels follow the bow, staggered so they stay apart.
        const cx = a.x + a.w, cy = a.y + a.h / 2;
        const bow = 60 + offset;
        d = `M${cx},${cy - 10} C${cx + bow},${cy - 34 - offset} ${cx + bow},${cy + 34 + offset} ${cx},${cy + 10}`;
        lx = cx + bow + 8; ly = cy - offset * 0.6;
      } else if (b.y > a.y) {
        // Downward: bottom port to top port.
        const p1 = { x: a.x + a.w / 2 + offset, y: a.y + a.h };
        const p2 = { x: b.x + b.w / 2 + offset, y: b.y };
        const dy = Math.max(34, (p2.y - p1.y) * 0.35);
        d = `M${p1.x},${p1.y} C${p1.x},${p1.y + dy} ${p2.x},${p2.y - dy} ${p2.x},${p2.y}`;
        lx = (p1.x + p2.x) / 2 + 10;
        ly = (p1.y + p2.y) / 2 - offset;
      } else if (b.y === a.y) {
        // Same layer: sag beneath the row so the edge cannot run
        // through intervening states. Opposite directions get
        // different sags so an A→B / B→A pair stays distinguishable.
        const dir = b.x > a.x ? 1 : -1;
        const p1 = { x: a.x + a.w / 2 + dir * 14, y: a.y + a.h };
        const p2 = { x: b.x + b.w / 2 - dir * 14, y: b.y + b.h };
        const sag = 46 + offset + (dir < 0 ? 24 : 0);
        d = `M${p1.x},${p1.y} C${p1.x},${p1.y + sag} ${p2.x},${p2.y + sag} ${p2.x},${p2.y}`;
        lx = (p1.x + p2.x) / 2 + 10;
        ly = (p1.y + p2.y) / 2 + sag * 0.75 + 4;
      } else {
        // Upward: bow around the right side, clearing every layer the
        // edge passes; anchor the label on the bow's apex.
        const x1 = a.x + a.w, y1 = a.y + a.h / 2;
        const x2 = b.x + b.w, y2 = b.y + b.h / 2;
        const cxr = rightmost + 40 + offset;
        d = `M${x1},${y1} C${cxr},${y1} ${cxr},${y2} ${x2},${y2}`;
        lx = (x1 + x2 + 6 * cxr) / 8 + 8;
        ly = (y1 + y2) / 2;
      }

      const stroke = isHl || isSel
        ? "var(--accent)"
        : status ? `var(--${status})` : null;

      const fxCount = Object.keys(t.side_effects).length;
      scene.append(S("g", {
        "data-sel": JSON.stringify({
          key: "t:" + tId, id: tId, machine: mId,
        }),
      },
        S("path", {
          class: "sm-edge" + (isSel ? " selected" : ""), d,
          style: stroke ? `stroke:${stroke}` : null,
          "marker-end": isHl || isSel ? "url(#arr-accent)" : "url(#arr-neutral)",
        }),
        S("path", { class: "edge-hit", d }),
        S("text", {
          class: "sm-edge-label" + (isSel ? " selected" : ""),
          x: lx, y: ly,
          style: stroke ? `fill:${stroke}` : null,
        },
          shortId(tId),
          fxCount ? S("tspan", { class: "fx" }, ` ⚡${fxCount}`) : null,
        ),
        S("title", {}, tId + "\nfrom: " + t.from.join(", ") + "\nto: " + t.to +
          (fxCount ? "\nside effects: " + Object.keys(t.side_effects).join(", ") : "")),
      ));
    }
  }

  // state nodes
  for (const s of machine.states) {
    const p = pos.get(s);
    const isInitial = s === machine.initial;
    const isSel = sel === "s:" + s;
    scene.append(S("g", {
      class: "node state-node" + (isInitial ? " initial" : "") +
        (isSel ? " selected" : ""),
      "data-sel": JSON.stringify({ key: "s:" + s, id: s, machine: mId }),
    },
      S("rect", { class: "body", x: p.x, y: p.y, width: p.w, height: p.h, rx: 16 }),
      S("text", { class: "title", x: p.x + p.w / 2, y: p.y + 22, "text-anchor": "middle" },
        p.label),
      isInitial
        ? S("text", { class: "init-mark", x: p.x + p.w / 2, y: p.y + 36, "text-anchor": "middle" },
            "● initial")
        : S("text", { class: "subtitle", x: p.x + p.w / 2, y: p.y + 36, "text-anchor": "middle",
            "font-size": 9, fill: "var(--text-faint)" }, ""),
      S("title", {}, s),
    ));
  }

  return [
    legendLine("var(--text-dim)", "transition"),
    legendLine("var(--edge-publish)", "⚡ = transition side effects"),
    legendChip("--client-line", "initial state"),
  ];
}

/* ================= detail panel ================= */

function showDetail(kindLabel, contentNodes) {
  detailKind.textContent = kindLabel;
  detailBody.replaceChildren(...contentNodes);
  detailPane.hidden = false;
}

function idLink(id) {
  return H("a", { class: "id-link", "data-id": id, href: "#" }, id);
}

function navLink(text, hash, selKey) {
  return H("a", {
    class: "id-link", href: "#", "data-nav": hash,
    "data-selkey": selKey || null,
  }, text);
}

function kv(pairs) {
  const grid = H("div", { class: "d-kv" });
  for (const [k, v] of pairs) {
    if (v === null || v === undefined) continue;
    grid.append(H("span", { class: "k" }, k));
    grid.append(H("span", { class: "v" }, v));
  }
  return grid;
}

function section(title) { return H("div", { class: "d-section" }, title); }

function tag(text, cls) {
  return H("span", { class: "tag" + (cls ? " " + cls : "") }, text);
}

function list(items) {
  return H("ul", { class: "d-list" }, items.map((i) => H("li", {}, i)));
}

function refText(ref) {
  return H("span", {},
    H("span", { class: "plain-id" }, ref.source.kind + "("),
    idLink(ref.source.id),
    H("span", { class: "plain-id" }, ")." + pathText(ref.path)));
}

function predText(p) {
  switch (p.kind) {
    case "all": return "all instances";
    case "eq": {
      const v = p.value.kind === "value"
        ? p.value.value.source.kind + "(" + shortId(p.value.value.source.id) +
          ")." + pathText(p.value.value.path)
        : JSON.stringify(p.value.value.value);
      return pathText(p.field) + " = " + v;
    }
    case "and": return p.predicates.map(predText).join(" ∧ ");
    default: return p.kind;
  }
}

function predNode(p) {
  return H("span", { class: "plain-id" }, predText(p));
}

function derivationNodes(d) {
  if (!d || d.kind !== "deterministic") {
    return [tag(d ? d.kind : "unspecified", "warn")];
  }
  return [tag("deterministic"), list(d.from.map(refText))];
}

function concurrencyText(c) {
  return c.kind === "bounded" ? `bounded(${c.value})` : c.kind;
}

function obligationSection(key) {
  if (!REPORT) return [];
  const obs = OB_INDEX.get(key) || [];
  if (!obs.length) return [];
  return [
    section("prover obligations"),
    ...obs.map((ob) => obligationCard(ob, false)),
  ];
}

function showDetailById(id, ctx) {
  const entry = IX.get(id);
  ctx = ctx || {};

  // Requirement chips route through extra context.
  if (ctx.req) return showRequirementDetail(id, ctx.req);
  if (ctx.extra && ctx.extra.txStep !== undefined) {
    return showTxStepDetail(ctx.extra.op, ctx.extra.tx, ctx.extra.txStep);
  }
  if (ctx.edge) {
    const e = GRAPH.edges.find((x) => x.id === id);
    if (e) return showEdgeDetail(e);
  }
  if (id === "@client") return showClientDetail();
  if (id.startsWith("@external:")) return showExternalDetail(id.slice(10));
  if (!entry) {
    return showDetail("unknown", [H("div", { class: "d-title" }, id)]);
  }

  switch (entry.kind) {
    case "service": return showServiceDetail(id);
    case "operation": return showOperationDetail(id);
    case "topic": return showTopicDetail(id);
    case "schema": return showSchemaDetail(id);
    case "data_model": return showDataModelDetail(id);
    case "object": return showObjectDetail(entry.dataModel, id);
    case "machine": return showMachineDetail(id);
    case "state": return showStateDetail(entry.machine, id);
    case "transition": return showTransitionDetail(entry.machine, id);
    case "input": return showInputDetail(entry.op, id);
    case "effect": return showEffectDetail(entry, id);
    case "intent": return showIntentDetail(entry.op, id);
    case "result": return showResultDetail(entry.op, id);
    case "response": return showResponseDetail(entry.op, id);
    case "transaction": return showTransactionDetail(entry.op, id);
    case "flow": return showFlowDetail(entry.op, id);
    default:
      return showDetail(entry.kind, [H("div", { class: "d-title" }, id)]);
  }
}

function showServiceDetail(id) {
  const svc = MODEL.services[id];
  const ops = GRAPH.operations.filter((o) => o.service === id);
  showDetail("service", [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, svc.kind + " service · boundary of " +
      ops.length + " operation" + (ops.length === 1 ? "" : "s")),
    section("operations"),
    list(ops.map((o) => navLink(o.id, "#/op/" + encodeURIComponent(o.id)))),
  ]);
}

function showOperationDetail(id) {
  const op = MODEL.operations[id];
  const node = GRAPH.operations.find((o) => o.id === id);
  const content = [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "operation on ", idLink(op.service)),
  ];
  if (op.description) {
    content.push(H("div", { class: "d-desc" }, op.description));
  }
  content.push(
    navLink("open flows →", "#/op/" + encodeURIComponent(id)),
    section("execution"),
    kv([["concurrency", concurrencyText(op.execution.concurrency)]]),
  );
  const inputs = Object.entries(op.inputs);
  if (inputs.length) {
    content.push(section("inputs"), list(inputs.map(([iid, input]) =>
      H("span", {}, idLink(iid), " ",
        tag(input.kind === "request" ? "request" : "sub ← " + shortId(input.topic))))));
  }
  const effects = Object.keys(op.effects);
  if (effects.length) {
    content.push(section("declared effects"), list(effects.map((eid) =>
      H("span", {}, idLink(eid), H("div", { class: "d-sub" }, effectSummary(eid))))));
  }
  if (node && node.machines.length) {
    content.push(section("state machines"), list(node.machines.map((m) =>
      navLink(m, "#/machine/" + encodeURIComponent(m)))));
  }
  const reqs = op.requirements;
  const reqRows = [];
  reqs.serialization.forEach((r, i) =>
    reqRows.push(H("span", {}, tag("serialization"), " key ", refText(r.key))));
  reqs.ordering.forEach((r) =>
    reqRows.push(H("span", {}, tag("ordering"), " key ", refText(r.key))));
  reqs.idempotency.forEach((r) =>
    reqRows.push(H("span", {}, tag("idempotency"),
      r.response === "replay_consistent" ? tag("replay_consistent") : null,
      " key ", ...r.key.components.flatMap((c, i) =>
        i ? [" + ", refText(c)] : [refText(c)]))));
  reqs.recoverability.forEach((r) =>
    reqRows.push(H("span", {}, tag("recoverability"), tag(r.completion),
      " key ", ...r.key.components.flatMap((c, i) =>
        i ? [" + ", refText(c)] : [refText(c)]))));
  if (reqRows.length) content.push(section("requirements"), list(reqRows));
  content.push(...obligationSection(id));
  showDetail("operation", content);
}

function showTopicDetail(id) {
  const topic = MODEL.topics[id];
  const content = [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "topic · ordering: " + topic.ordering.kind),
    section("message schemas"),
    list(topic.messages.map(idLink)),
  ];
  if (topic.ordering.kind === "keyed") {
    content.push(section("ordering key mapping"),
      kv(Object.entries(topic.ordering.mapping).map(([schema, path]) =>
        [shortId(schema), pathText(path)])));
  }
  const pubs = GRAPH.edges.filter((e) => e.kind === "publish" && e.to === id);
  const subs = GRAPH.edges.filter((e) => e.kind === "subscribe" && e.from === id);
  if (pubs.length) {
    content.push(section("publishers"), list(pubs.map((e) =>
      H("span", {}, idLink(e.operation), " ", tag(shortId(e.schema))))));
  }
  if (subs.length) {
    content.push(section("subscribers"), list(subs.map((e) =>
      H("span", {}, idLink(e.operation), " ",
        tag(e.delivery), tag(e.routing)))));
  }
  content.push(...obligationSection(id));
  showDetail("topic", content);
}

function typeText(ty) {
  if (ty.kind === "scalar") return ty.value;
  if (ty.kind === "schema") return null; // rendered as link
  if (ty.kind === "list") {
    const inner = typeText(ty.value);
    return inner === null ? null : "list<" + inner + ">";
  }
  return ty.kind;
}

function typeNode(ty) {
  const plain = typeText(ty);
  if (plain !== null) return H("span", { class: "plain-id" }, plain);
  if (ty.kind === "schema") return idLink(ty.value);
  if (ty.kind === "list") {
    return H("span", {}, H("span", { class: "plain-id" }, "list<"),
      typeNode(ty.value), H("span", { class: "plain-id" }, ">"));
  }
  return H("span", {}, ty.kind);
}

function showSchemaDetail(id) {
  const schema = MODEL.schemas[id];
  const content = [H("div", { class: "d-title" }, id)];
  if (schema.kind === "canonical") {
    content.push(H("div", { class: "d-sub" },
      "canonical schema · " + schema.completeness));
    if (schema.description) {
      content.push(H("div", { class: "d-desc" }, schema.description));
    }
    content.push(section("fields"));
    const rows = Object.entries(schema.fields).map(([name, f]) =>
      H("span", {}, H("span", { class: "plain-id" }, name + ": "),
        typeNode(f.ty), f.optional ? tag("optional") : null));
    content.push(list(rows));
  } else {
    content.push(H("div", { class: "d-sub" }, "schema fragment of ",
      idLink(schema.source)));
    content.push(section("mapping"),
      kv(Object.entries(schema.mapping).map(([name, path]) =>
        [name, pathText(path)])));
  }
  showDetail("schema", content);
}

function showDataModelDetail(id) {
  const dm = MODEL.data_models[id];
  showDetail("data model", [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "transactional state boundary"),
    section("objects"),
    list(Object.keys(dm.objects).map(idLink)),
  ]);
}

function showObjectDetail(dmId, id) {
  const obj = MODEL.data_models[dmId].objects[id];
  const content = [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "persistent object in ", idLink(dmId)),
    kv([
      ["schema", idLink(obj.schema)],
      ["identity", obj.identity.map(pathText).join(", ")],
    ]),
  ];
  if (obj.requirements.history.length) {
    content.push(section("history requirements"),
      list(obj.requirements.history.map((h) => tag(h))));
  }
  content.push(...obligationSection(dmId + "/" + id));
  showDetail("data object", content);
}

function showMachineDetail(id) {
  const m = MODEL.state_machines[id];
  showDetail("state machine", [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "governs ", idLink(m.subject.object),
      " · field " + pathText(m.subject.state)),
    navLink("open state graph →", "#/machine/" + encodeURIComponent(id)),
    section("states"),
    list(m.states.map((s) => H("span", {},
      H("span", { class: "plain-id" }, s),
      s === m.initial ? tag("initial") : null))),
    section("transitions"),
    list(Object.keys(m.transitions).map((t) =>
      navLink(t, "#/machine/" + encodeURIComponent(id) +
        "?t=" + encodeURIComponent(t), "t:" + t))),
    ...obligationSection(id),
  ]);
}

function showStateDetail(mId, id) {
  const m = MODEL.state_machines[mId];
  const into = [], outof = [];
  for (const [tId, t] of Object.entries(m.transitions)) {
    if (t.to === id) into.push(tId);
    if (t.from.includes(id)) outof.push(tId);
  }
  showDetail("state", [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "state of ", idLink(mId),
      id === m.initial ? " · initial" : ""),
    outof.length ? section("transitions out") : null,
    outof.length ? list(outof.map(idLink)) : null,
    into.length ? section("transitions in") : null,
    into.length ? list(into.map(idLink)) : null,
  ].filter(Boolean));
}

function showTransitionDetail(mId, id) {
  const t = MODEL.state_machines[mId].transitions[id];
  const content = [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "transition of ", idLink(mId)),
    kv([
      ["from", t.from.join(", ")],
      ["to", t.to],
    ]),
  ];
  const fx = Object.entries(t.side_effects);
  if (fx.length) {
    content.push(section("side effects"));
    content.push(list(fx.map(([eid, e]) => H("span", {},
      idLink(eid),
      H("div", { class: "d-sub" },
        e.kind === "publication"
          ? H("span", {}, "publish ", idLink(e.schema), " → ", idLink(e.topic))
          : H("span", {}, "request → ", idLink(e.target.operation))),
      ...intentExecutors(eid).map((x) =>
        H("div", { class: "d-sub" }, "executed by ", idLink(x.op),
          " via ", idLink(x.intent))),
    ))));
  }
  const refs = GRAPH.transition_refs[mId + "/" + id] || [];
  if (refs.length) {
    content.push(section("taken by transactions"));
    content.push(list(refs.map((r) => H("span", {},
      idLink(r.transaction), " step " + (r.step + 1) + " in ",
      navLink(shortId(r.operation), "#/op/" + encodeURIComponent(r.operation))))));
  }
  content.push(...obligationSection(mId + "/" + id));
  showDetail("transition", content);
}

function showInputDetail(opId, id) {
  const input = MODEL.operations[opId].inputs[id];
  const content = [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "input of ", idLink(opId)),
  ];
  if (input.kind === "request") {
    content.push(kv([["kind", "request"], ["schema", idLink(input.schema)]]));
  } else {
    const schemas = input.messages.kind === "all"
      ? [H("span", {}, tag("all topic messages"))]
      : input.messages.schemas.map(idLink);
    content.push(kv([
      ["kind", "subscription"],
      ["topic", idLink(input.topic)],
      ["delivery", input.delivery],
      ["routing", input.dispatch.routing],
      ["lane concurrency", concurrencyText(input.dispatch.lane_concurrency)],
    ]));
    content.push(section("consumed messages"), list(schemas));
  }
  showDetail("input", content);
}

function propagationNodes(props) {
  if (!props || !props.length) return [];
  return [
    section("idempotency key propagation"),
    list(props.map((p) => H("span", {},
      H("div", {}, "from: ", ...p.source.components.flatMap((c, i) =>
        i ? [" + ", refText(c)] : [refText(c)])),
      H("div", {}, "to: ", ...p.target.components.flatMap((c, i) =>
        i ? [" + ", refText(c)] : [refText(c)])),
    ))),
  ];
}

function showEffectDetail(entry, id) {
  const def = effectDef(id);
  const e = def.effect;
  const owner = entry.op
    ? H("span", {}, "declared by ", idLink(entry.op))
    : H("span", {}, "owned by transition ", idLink(entry.transition),
        " of ", idLink(entry.machine));
  const content = [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, owner),
  ];
  if (e.kind === "publication") {
    content.push(kv([
      ["kind", "publication"],
      ["topic", idLink(e.topic)],
      ["schema", idLink(e.schema)],
    ]));
    content.push(...propagationNodes(e.idempotency_key_propagation));
  } else if (e.kind === "request") {
    content.push(kv([
      ["kind", "request"],
      ["operation", idLink(e.target.operation)],
      ["input", idLink(e.target.input)],
      ["schema", idLink(e.schema)],
      ["retry", e.retry],
    ]));
    content.push(...propagationNodes(e.idempotency_key_propagation));
  } else {
    content.push(kv([
      ["kind", "external"],
      ["name", e.name],
      ["idempotency", e.idempotency.kind],
    ]));
  }
  const executors = intentExecutors(id);
  if (executors.length) {
    content.push(section("executed via intents"),
      list(executors.map((x) => H("span", {},
        idLink(x.intent), " in ", idLink(x.op)))));
  }
  showDetail("effect", content);
}

function showIntentDetail(opId, id) {
  const intent = MODEL.operations[opId].effect_intents[id];
  const owner = IX.get(intent.effect);
  showDetail("effect intent", [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "intent of ", idLink(opId)),
    kv([["effect", idLink(intent.effect)]]),
    owner && owner.machine
      ? H("div", { class: "d-desc" },
          "The effect is owned by transition ", idLink(owner.transition),
          "; a successful transition implicitly establishes this intent.")
      : null,
    section("resolved effect"),
    H("div", { class: "d-sub" }, effectSummary(intent.effect)),
  ].filter(Boolean));
}

function showResultDetail(opId, id) {
  const r = MODEL.operations[opId].invocation_results[id];
  showDetail("invocation result", [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "logical artifact of ", idLink(opId)),
    kv([["schema", idLink(r.schema)]]),
  ]);
}

function showResponseDetail(opId, id) {
  const r = MODEL.operations[opId].responses[id];
  showDetail("response", [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "response of ", idLink(opId)),
    kv([
      ["request", idLink(r.request)],
      ["schema", idLink(r.schema)],
      ["source", r.source.kind === "invocation_result"
        ? idLink(r.source.result) : r.source.kind],
    ]),
  ]);
}

function showTransactionDetail(opId, id) {
  const tx = MODEL.operations[opId].transactions[id];
  const content = [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "transaction of ", idLink(opId)),
    kv([
      ["data model", tx.data_model ? idLink(tx.data_model) : "none (framework artifacts only)"],
      ["isolation", tx.isolation],
      ["idempotency", tx.idempotency.kind],
    ]),
  ];
  if (tx.idempotency.kind === "deduplicated_by") {
    content.push(section("dedup key"),
      list(tx.idempotency.key.components.map(refText)));
  }
  content.push(section("steps"),
    list(tx.steps.map((s, i) => H("span", {},
      tag(String(i + 1)), " " + s.kind + " ",
      s.kind === "transition" ? idLink(s.transition) : null))));
  content.push(...obligationSection(opId + "/" + id));
  showDetail("transaction", content);
}

function showFlowDetail(opId, id) {
  const flow = MODEL.operations[opId].flows[id];
  showDetail("invocation flow", [
    H("div", { class: "d-title" }, id),
    H("div", { class: "d-sub" }, "flow of ",
      navLink(shortId(opId), "#/op/" + encodeURIComponent(opId))),
    section("steps"),
    list(flow.steps.map((s, i) => H("span", {}, tag(String(i + 1)),
      " " + s.kind + " ",
      idLink(s.transaction || s.effect || s.intent)))),
    kv([["terminal response", flow.response ? idLink(flow.response) : "none"]]),
    ...obligationSection(opId + "/" + id),
  ]);
}

function showRequirementDetail(opId, req) {
  const op = MODEL.operations[opId];
  const r = op.requirements[req.prop][req.index];
  const content = [
    H("div", { class: "d-title" }, req.prop + " requirement #" + req.index),
    H("div", { class: "d-sub" }, "declared on ", idLink(opId)),
  ];
  if (req.prop === "serialization" || req.prop === "ordering") {
    content.push(section("key"), list([refText(r.key)]));
  } else {
    content.push(section("key components"),
      list(r.key.components.map(refText)));
    if (req.prop === "idempotency") {
      content.push(kv([["response", r.response]]));
    } else {
      content.push(kv([["completion", r.completion]]));
    }
  }
  if (REPORT) {
    const obs = (OB_INDEX.get(opId) || []).filter((ob) =>
      ob.subject.kind === "operation" &&
      ob.subject.requirement === req.index &&
      obPropertyMatches(ob.property, req.prop));
    if (obs.length) {
      content.push(section("prover obligations"),
        ...obs.map((ob) => obligationCard(ob, false)));
    }
  }
  showDetail("requirement", content);
}

function showTxStepDetail(opId, txId, stepIdx) {
  const step = MODEL.operations[opId].transactions[txId].steps[stepIdx];
  const content = [
    H("div", { class: "d-title" }, step.kind + " · step " + (stepIdx + 1)),
    H("div", { class: "d-sub" }, "in ", idLink(txId), " of ", idLink(opId)),
  ];
  const target = step.target;
  switch (step.kind) {
    case "read":
      content.push(kv([["result id", step.result], ["object", idLink(target.object)]]));
      content.push(section("predicate"), predNode(target.predicate));
      content.push(section("fields"),
        step.fields.kind === "all"
          ? H("div", {}, tag("all fields"))
          : list(step.fields.fields.map((f) =>
              H("span", { class: "plain-id" }, pathText(f)))));
      break;
    case "write":
      content.push(kv([["object", idLink(target.object)]]));
      content.push(section("predicate"), predNode(target.predicate));
      content.push(section("fields written"),
        list(step.fields.map((f) => H("span", { class: "plain-id" }, pathText(f)))));
      content.push(section("value provenance"), ...derivationNodes(step.values));
      break;
    case "insert":
      content.push(kv([["object", idLink(step.object)]]));
      content.push(section("value provenance"), ...derivationNodes(step.values));
      break;
    case "delete":
      content.push(kv([["object", idLink(target.object)]]));
      content.push(section("predicate"), predNode(target.predicate));
      break;
    case "lock":
      content.push(kv([
        ["object", idLink(target.object)],
        ["mode", step.mode],
        ["order", step.order.kind],
      ]));
      content.push(section("predicate"), predNode(target.predicate));
      break;
    case "transition":
      content.push(kv([
        ["machine", idLink(step.machine)],
        ["transition", idLink(step.transition)],
        ["subject object", idLink(step.subject.object)],
      ]));
      content.push(section("subject predicate"), predNode(step.subject.predicate));
      content.push(H("div", { style: "margin-top:10px" },
        navLink("view in state machine →",
          "#/machine/" + encodeURIComponent(step.machine) +
          "?t=" + encodeURIComponent(step.transition))));
      break;
    case "establish_effect_intent":
      content.push(kv([["intent", idLink(step.intent)]]));
      content.push(section("value provenance"), ...derivationNodes(step.values));
      break;
    case "establish_invocation_result":
      content.push(kv([["result", idLink(step.result)]]));
      content.push(section("value provenance"), ...derivationNodes(step.values));
      break;
  }
  showDetail("transaction step", content);
}

function showEdgeDetail(e) {
  const content = [];
  switch (e.kind) {
    case "publish":
      content.push(
        H("div", { class: "d-title" }, "publication"),
        H("div", { class: "d-sub" },
          idLink(e.operation), " → ", idLink(e.to)),
        kv([
          ["effect", idLink(e.effect)],
          ["schema", idLink(e.schema)],
        ]));
      if (e.via_transition) {
        content.push(H("div", { class: "d-desc" },
          "Owned by transition ", idLink(e.via_transition.transition),
          " of ", navLink(shortId(e.via_transition.machine),
            "#/machine/" + encodeURIComponent(e.via_transition.machine)),
          "; the publication becomes intended when the transition commits."));
      }
      break;
    case "subscribe":
      content.push(
        H("div", { class: "d-title" }, "subscription"),
        H("div", { class: "d-sub" },
          idLink(e.from), " → ", idLink(e.operation)),
        kv([
          ["input", idLink(e.input)],
          ["delivery", e.delivery],
          ["routing", e.routing],
          ["lane concurrency", e.lane_concurrency],
        ]),
        section("consumed messages"),
        list(e.schemas.map(idLink)));
      break;
    case "request":
      content.push(
        H("div", { class: "d-title" }, "request"),
        H("div", { class: "d-sub" },
          idLink(e.operation), " → ", idLink(e.to)),
        kv([
          ["effect", idLink(e.effect)],
          ["target input", idLink(e.input)],
          ["schema", idLink(e.schema)],
          ["retry", e.retry],
        ]));
      if (e.via_transition) {
        content.push(H("div", { class: "d-desc" },
          "Owned by transition ", idLink(e.via_transition.transition),
          " of ", navLink(shortId(e.via_transition.machine),
            "#/machine/" + encodeURIComponent(e.via_transition.machine)),
          "; the request becomes intended when the transition commits."));
      }
      break;
    case "external":
      content.push(
        H("div", { class: "d-title" }, "external effect"),
        H("div", { class: "d-sub" }, idLink(e.operation), " → ", e.to.slice(10)),
        kv([
          ["effect", idLink(e.effect)],
          ["idempotency", e.idempotency],
        ]),
        H("div", { class: "d-desc" },
          "The modeled system ends here; the checker cannot inspect the external implementation."));
      break;
    case "client":
      content.push(
        H("div", { class: "d-title" }, "client request"),
        H("div", { class: "d-sub" }, "→ ", idLink(e.operation)),
        kv([
          ["input", idLink(e.input)],
          ["schema", idLink(e.schema)],
        ]),
        H("div", { class: "d-desc" },
          "No modeled operation issues this request; it enters the system from unmodeled callers."));
      break;
  }
  if ("executed_by" in e) {
    content.push(section("executed by flows"),
      e.executed_by.length
        ? list(e.executed_by.map(idLink))
        : H("div", { class: "d-sub" },
            tag("declared, not executed", "warn"),
            " — no declared flow executes this effect."));
  }
  showDetail("edge", content);
}

function showClientDetail() {
  const edges = GRAPH.edges.filter((e) => e.kind === "client");
  showDetail("clients", [
    H("div", { class: "d-title" }, "unmodeled callers"),
    H("div", { class: "d-desc" },
      "Request inputs that no modeled operation invokes; they are the system's entry points."),
    section("entry points"),
    list(edges.map((e) => H("span", {},
      idLink(e.operation), " ", tag(shortId(e.schema))))),
  ]);
}

function showExternalDetail(name) {
  const edges = GRAPH.edges.filter((e) =>
    e.kind === "external" && e.to === "@external:" + name);
  showDetail("external system", [
    H("div", { class: "d-title" }, name),
    H("div", { class: "d-desc" },
      "External dependency; the modeled system ends here."),
    section("invoked by"),
    list(edges.map((e) => H("span", {},
      idLink(e.operation), " via ", idLink(e.effect),
      " ", tag(e.idempotency)))),
  ]);
}

/* ================= obligations panel ================= */

function renderObligations() {
  obFilters.replaceChildren();
  obList.replaceChildren();
  if (!REPORT) return;

  const counts = statusCounts(REPORT.obligations);
  for (const s of ["disproven", "unknown", "proven"]) {
    const btn = H("button", {
      class: "ob-filter " + s + (state.obFilter[s] ? " active" : ""),
    }, `${STATUS_GLYPH[s]} ${s} (${counts[s] || 0})`);
    btn.addEventListener("click", () => {
      state.obFilter[s] = !state.obFilter[s];
      renderObligations();
    });
    obFilters.append(btn);
  }

  const order = { disproven: 0, unknown: 1, proven: 2 };
  const obs = [...REPORT.obligations]
    .filter((ob) => state.obFilter[ob.status])
    .sort((a, b) => order[a.status] - order[b.status] ||
      a.id.localeCompare(b.id));

  for (const ob of obs) obList.append(obligationCard(ob, true));

  if (!obs.length) {
    obList.append(H("div", { class: "d-sub" }, "no obligations match the filters"));
  }
}

function subjectText(s) {
  switch (s.kind) {
    case "operation":
      return s.operation + (s.requirement !== null && s.requirement !== undefined
        ? " · requirement #" + s.requirement : "");
    case "flow": return s.operation + " · " + s.flow;
    case "transaction": return s.operation + " · " + s.transaction;
    case "object": return s.data_model + " · " + s.object;
    case "state_machine":
      return s.machine + (s.transition ? " · " + s.transition : "");
    case "topic": return s.topic;
    default: return "";
  }
}

function focusSubject(ob) {
  const s = ob.subject;
  switch (s.kind) {
    case "operation":
      if (s.requirement !== null && s.requirement !== undefined) {
        const prop = ob.property.kind === "response_replay"
          ? "idempotency" : ob.property.kind;
        pendingSelection = `req:${prop}:${s.requirement}`;
      }
      navigate("#/op/" + encodeURIComponent(s.operation));
      break;
    case "flow":
      pendingSelection = "flow:" + s.flow;
      navigate("#/op/" + encodeURIComponent(s.operation));
      break;
    case "transaction":
      pendingSelection = "tx:" + s.transaction;
      navigate("#/op/" + encodeURIComponent(s.operation));
      break;
    case "state_machine":
      navigate("#/machine/" + encodeURIComponent(s.machine) +
        (s.transition ? "?t=" + encodeURIComponent(s.transition) : ""));
      break;
    case "topic":
      pendingSelection = s.topic;
      navigate("#/system");
      break;
    case "object":
      showDetailById(s.object, {});
      break;
  }
}

function obligationCard(ob, interactive) {
  const card = H("div", { class: "ob-card " + ob.status },
    H("div", {},
      H("span", { class: "ob-prop" }, ob.property.kind === "custom"
        ? ob.property.name : ob.property.kind),
      H("span", { class: "ob-status" }, STATUS_GLYPH[ob.status] + " " + ob.status)),
    H("div", { class: "ob-summary" }, ob.summary),
    H("div", { class: "ob-subject" }, subjectText(ob.subject)),
  );

  const expanded = state.obExpanded.has(ob.id);
  if (expanded) {
    const d = H("div", { class: "ob-detail" });
    if (ob.assumptions && ob.assumptions.length) {
      d.append(section("relies on declared facts"),
        list(ob.assumptions.map((a) => H("span", {}, a))));
    }
    if (ob.evidence && ob.evidence.length) {
      d.append(section("evidence"),
        list(ob.evidence.map((ev) => H("span", {},
          ev.subject ? H("span", {}, idLink(ev.subject), " — ") : null,
          ev.message))));
    }
    if (ob.counterexample && ob.counterexample.trace) {
      d.append(section("counterexample trace"));
      d.append(H("ol", { class: "trace" },
        ob.counterexample.trace.map((t) => H("li", {},
          t.actor ? H("span", { class: "actor" }, shortId(t.actor) + ": ") : null,
          t.description))));
    }
    const go = H("a", { class: "id-link", href: "#" }, "focus subject →");
    go.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      focusSubject(ob);
    });
    d.append(H("div", { style: "margin-top:10px" }, go));
    card.append(d);
  }

  if (interactive) {
    card.addEventListener("click", (e) => {
      if (e.target.closest("a")) return;
      if (state.obExpanded.has(ob.id)) state.obExpanded.delete(ob.id);
      else state.obExpanded.add(ob.id);
      renderObligations();
    });
  }
  return card;
}

/* ================= boot ================= */

state.route = parseHash();
render();
// Refit once real layout dimensions exist.
requestAnimationFrame(() => view.fit());

})();
