# archspec-viz

`archspec-viz` renders an Archspec model as a single self-contained,
interactive HTML file. The output opens directly from disk, makes no
network requests, and can be attached to a review, a PR, or a design
document as-is.

```
cargo run --bin archspec-viz -- tests/fixtures/flash_checkout.yaml
# wrote tests/fixtures/flash_checkout.html

cargo run --bin archspec-viz -- model.yaml \
    --report proof.json \
    --out model.html \
    --title "checkout architecture"
```

Analyzer validation runs before rendering; diagnostics go to stderr and
rendering proceeds anyway, so imperfect models can still be inspected
(`--no-validate` silences the pass).

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

**Operation view** (`#/op/<id>`). One column per declared invocation
flow, steps in order. Transaction steps expand in place into their
transaction's steps (reads, writes, inserts, deletes, locks,
transitions, artifact establishments), each with its selector,
provenance, and a full detail panel. Transition steps link into the
owning state machine. Requirement chips across the top carry
per-requirement prover statuses when a report is loaded.

**State machine view** (`#/machine/<id>`). The state graph: legal
states, initial state, transitions (with ⚡ badges for transition-owned
side effects). Selecting a transition shows its from/to sets, its side
effects resolved to topics/operations, the operations that execute
those effects via intents, and every transaction step that takes the
transition. `?t=<transition id>` deep-links with a highlight.

## Prover report overlay

The prover and model checker do not exist yet. The tool nevertheless
accepts a report in a **provisional format** (defined in
`src/bin/viz/report.rs`) so the presentation layer can grow with them;
when the real formats land, that module is the single adaptation point.

```
archspec-viz model.yaml --example-report        # scaffold to stdout
archspec-viz model.yaml --report proof.json     # overlay a report
```

`--example-report` emits the report the eventual prover is expected to
fill in: one obligation per declared requirement (serialization,
ordering, idempotency, response replay, recoverability, object
history), each with status `unknown`.

A report is an object with a `format` version (currently `1`), an
optional `model_revision` (mismatches with the rendered model produce a
warning), and an `obligations` list. Unknown fields are rejected, so
start from `--example-report` rather than writing one from scratch.
Each obligation carries:

- `id` — stable identity of the obligation within the report.
- `summary` — one-line human-readable statement of the obligation.
- `property` — what is being discharged; mirrors the requirement kinds,
  plus `custom` for solver-specific analyses.
- `subject` — the model entity the verdict anchors to: an operation
  (optionally a specific requirement index), flow, transaction, data
  object, state machine transition, or topic.
- `status` — `proven`, `disproven`, or `unknown`. Unknown is epistemic:
  the solver could not decide, typically because a required fact is
  `unspecified`. It is never evidence of a violation.
- `assumptions` — the declared facts the verdict is conditional on.
- `evidence` — model facts explaining the verdict.
- `counterexample` — for disproofs, a trace of an admitted execution
  that violates the property.

With a report loaded, vertices gain status rings and rollup chips
(worst status wins: disproven > unknown > proven), requirement chips in
the operation view are colored by their obligation status, transitions
in the machine view inherit theirs, and the obligations panel lists
everything with filtering, inline evidence and counterexample traces,
and focus-navigation to each obligation's subject.

`tests/fixtures/flash_checkout.report.json` is a hand-written example
of what a finished report could look like against the flash-checkout
fixture, including genuine findings (the non-deduplicated,
read-dependent `tx.reserve_inventory` is not idempotent under
at-least-once delivery; the external card charge can double-execute).

## Layout of the implementation

```
src/bin/viz/
  main.rs      CLI: parse, validate, load report, write output
  graph.rs     Model → system graph (vertices, edges, indexes);
               resolves intents, transition ownership, message
               selectors so the front end never re-implements them
  report.rs    provisional prover-report types + scaffold generator
  render.rs    inlines data + assets into the HTML template
  assets/      template.html, style.css, app.js (vanilla JS/SVG)
```

The page consumes `window.ARCHSPEC = { title, model, graph, report }`:
the full serialized model for detail panes, the derived graph for the
system view, and the optional report. Anything new the prover reports
can be surfaced by extending `report.rs` and the overlay code in
`assets/app.js` without touching the extraction pipeline.
