# archspec-viz

`archspec-viz` renders an Archspec model as a single self-contained,
interactive HTML file. The output opens directly from disk, makes no
network requests, and can be attached to a review, a PR, or a design
document as-is.

```
cargo run --bin archspec-viz -- tests/fixtures/flash_checkout.yaml
# wrote tests/fixtures/flash_checkout.html

cargo run --bin archspec-viz -- model.yaml --verify \
    --out model.html \
    --title "checkout architecture"
```

Analyzer validation runs before rendering; diagnostics go to stderr and
rendering proceeds anyway, so imperfect models can still be inspected
(`--no-validate` silences the pass).

`--verify` runs the model checker and overlays its obligation report.
`--report <PATH>` overlays a report produced earlier by
`archspec model.yaml --report <PATH>`; the two are mutually exclusive.

## Views

**System view** (`#/system`). Services are drawn as boundary boxes with
their operations inside; topics, external systems, and a synthetic
"clients" vertex (for request inputs no modeled operation invokes) sit
around them. Edges are the model's information routes:

- publication effects: operation → topic
- subscription inputs: topic → operation
- request effects: operation → operation
- external effects: operation → external system
- client requests: clients → operation, for request inputs no modeled
  operation invokes

A dashed edge is a *declared but unexecuted* capability: the effect
exists on the operation but no declared flow executes it. Effects owned
by state-machine transitions are attributed to the operations that
execute them through intents and marked "via transition". Click
anything for a structured detail panel; double-click an operation to
drill in. The filter box dims non-matching vertices.

**Operation view** (`#/op/<id>`). Inputs and requirements across the
top, then the operation's invocation flows as tabs, one flow at a
time — flows are alternative execution paths (an invocation takes
exactly one), so they are never shown side by side. The active tab is
part of the route (`#/op/<id>?flow=<flow id>`), so a flow can be
deep-linked, and clicking a flow in the operation's detail panel or an
obligation's subject lands directly on it. Within a flow, steps run in
order: transaction steps expand in place into their transaction's
steps (reads, writes, inserts, deletes, locks, transitions, artifact
establishments), each with its selector, provenance, and a full detail
panel; execute-effect steps are badged with the effect's kind
(publication, request, external). Transition steps link into the
owning state machine. Requirement chips carry per-requirement verdicts
when a report is loaded.

**State machine view** (`#/machine/<id>`). The state graph: legal
states, initial state, transitions (with ⚡ badges for transition-owned
side effects). Selecting a transition shows its from/to sets, its side
effects resolved to topics/operations, the operations that execute
those effects via intents, and every transaction step that takes the
transition. `?t=<transition id>` deep-links with a highlight.

## Panels

**Detail panel.** Every model entity — service, operation, topic,
schema, data object, state machine, state, transition, input, effect,
intent, result, response, transaction, transaction step, flow,
requirement, or graph edge — opens a detail panel organized into
collapsible, counted sections (execution, inputs, declared effects,
requirements, obligations, …) with key/value grids, typed badges, and
clickable ids that open the referenced entity in place.

**Obligations panel.** The checker's obligations, grouped by the
operation (or data model, machine, topic) they anchor to, with a
segmented status filter (all / unknown / proven / disproven), a text
filter, per-group status counts, and cards that expand to the declared
facts a proof relies on, the checker's evidence, or a counterexample
trace — each with a "focus subject" action that navigates to the
entity. With a report loaded, vertices gain status rings and rollup
chips (worst status wins: disproven > unknown > proven), requirement
chips in the operation view are colored by their obligation status, and
transitions in the machine view inherit theirs.

## The obligation report

The report format is `archspec::analyzer::report` (`ProverReport`):
one obligation per declared requirement — serialization, ordering,
idempotency, response replay, recoverability, object history — with
status `proven`, `disproven`, or `unknown`. Unknown is epistemic: the
checker could not establish the property, typically because a
required fact is `unspecified` or no V1 verifier attempts that family.
It is never evidence of a violation.

Each obligation carries its `summary`, `subject`, `assumptions` (the
declared facts a proof relies on — conditional, per §25 of the
semantics contract), `evidence` (the checker's obstacles), and, for
disproofs, a `counterexample` trace.

```
archspec model.yaml --report proof.json       # produce a report
archspec-viz model.yaml --report proof.json   # overlay it
archspec-viz model.yaml --verify              # produce and overlay in one step
archspec-viz model.yaml --example-report      # scaffold, every status unknown
```

`tests/fixtures/flash_checkout.report.json` is generated by the checker
(see `tests/report.rs`), and records the fixture's genuine findings:
the non-deduplicated, read-dependent `tx.reserve_inventory` is neither
idempotent nor recoverable, and the external card charge is explicitly
not deduplicated.

## Front end

The presentation layer is a React + TypeScript application in `viz/`,
built with Vite, styled with Tailwind CSS v4 and Cloudflare's
[Kumo](https://kumo-ui.com/) design system (dark and light modes via
`data-mode`). The graph views are SVG rendered by React; the layout
algorithms live in `viz/src/graph/layout*.ts`.

```
cd viz
npm install
npm run data    # page data for the video-streaming example → public/archspec.json
npm run dev     # Vite dev server on http://localhost:5173
npm run build   # typecheck + single-file bundle → dist/index.html
```

The production build is one `dist/index.html` with every script and
stylesheet inlined (`vite-plugin-singlefile`). `archspec-viz` embeds
that file at compile time (`include_str!`) and injects the page data —
title, model, derived graph, report — as `window.ARCHSPEC`, so `cargo`
needs no Node toolchain. **Rebuild and commit `viz/dist/index.html`
after changing the front end.** During development the app fetches
`public/archspec.json` instead; regenerate it with `npm run data`, or
directly with `archspec-viz <model> --verify --json --out <path>`.

## Layout of the implementation

```
src/bin/viz/
  main.rs      CLI: parse, validate, verify or load a report, render
  graph.rs     Model → system graph (vertices, edges, indexes);
               resolves intents, transition ownership, message
               selectors so the front end never re-implements them
  report.rs    re-export of archspec::analyzer::report
  render.rs    embeds viz/dist/index.html and injects the page data

viz/
  src/types/   TypeScript mirrors of the model, graph, and report JSON
  src/lib/     id index, obligation index, routing, text helpers
  src/state/   app state (selection, detail target, filters, theme)
  src/graph/   SVG canvas (pan/zoom), layouts, the three views
  src/panels/  detail panel, obligations panel, shared Kumo parts
  src/chrome/  top bar
  dist/        committed single-file production bundle
```
