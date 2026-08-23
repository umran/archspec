import { Badge } from "@cloudflare/kumo/components/badge";
import { Button } from "@cloudflare/kumo/components/button";
import { Text } from "@cloudflare/kumo/components/text";
import { ArrowSquareOutIcon, XIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";

import { pathText, shortId } from "../lib/ids";
import { effectDef, effectSummary, flowContaining, intentExecutors } from "../lib/index";
import { propertyMatchesRequirement } from "../lib/obligations";
import { hashes } from "../lib/route";
import { concurrencyText } from "../lib/text";
import { useApp, useObligationsAt, type DetailTarget } from "../state/AppState";
import { CLIENT_NODE_ID, EXTERNAL_PREFIX, type Edge } from "../types/graph";
import type { Id, IdempotencyKeyPropagation, RequirementKind } from "../types/model";
import { ObligationCard } from "./ObligationCard";
import {
  DerivationView, IdLink, KeyComponents, KeyValue, List, Mono, Muted, NavLink, PredicateView,
  RefText, Section, StatusChips, Tag, TypeView,
} from "./parts";

/** Chrome shared by every detail: kind label, close button, title block. */
function Frame({
  kind, title, subtitle, description, children,
}: { kind: string; title: ReactNode; subtitle?: ReactNode; description?: ReactNode; children?: ReactNode }) {
  const { closeDetail } = useApp();
  return (
    <div className="flex h-full flex-col">
      <header className="flex shrink-0 items-center justify-between border-b border-kumo-hairline px-4 py-2">
        <span className="uppercase tracking-wider">
          <Text variant="secondary" size="xs" as="span">
            {kind}
          </Text>
        </span>
        <Button variant="ghost" size="xs" shape="square" icon={XIcon} aria-label="Close" onClick={closeDetail} />
      </header>
      <div className="flex-1 space-y-3 overflow-y-auto px-4 py-3">
        <div className="space-y-1">
          <div className="break-all font-mono text-[13px] font-semibold text-kumo-strong">{title}</div>
          {subtitle && <div className="text-sm text-kumo-subtle">{subtitle}</div>}
          {description && <div className="text-sm leading-relaxed text-kumo-default">{description}</div>}
        </div>
        {children}
      </div>
    </div>
  );
}

function Obligations({ obKey, filter }: { obKey: string; filter?: (ob: ReturnType<typeof useObligationsAt>[number]) => boolean }) {
  const all = useObligationsAt(obKey);
  const obs = filter ? all.filter(filter) : all;
  if (!obs.length) return null;
  return (
    <Section title="prover obligations" count={obs.length}>
      {obs.map((ob) => (
        <ObligationCard key={ob.id} ob={ob} />
      ))}
    </Section>
  );
}

function Propagation({ items }: { items: IdempotencyKeyPropagation[] }) {
  if (!items.length) return null;
  return (
    <Section title="idempotency key propagation" count={items.length}>
      <List
        items={items.map((p, i) => (
          <div key={i} className="space-y-0.5">
            <div><Muted>from</Muted> <KeyComponents value={p.source} /></div>
            <div><Muted>to</Muted> <KeyComponents value={p.target} /></div>
          </div>
        ))}
      />
    </Section>
  );
}

export function DetailPanel() {
  const { detail } = useApp();
  if (!detail) return null;
  return <Dispatch target={detail} />;
}

function Dispatch({ target }: { target: DetailTarget }) {
  const { index, graph } = useApp();
  const { id, ctx } = target;

  if (ctx.req) return <RequirementDetail opId={id} prop={ctx.req.prop} reqIndex={ctx.req.index} />;
  if (ctx.txStep) return <TxStepDetail opId={ctx.txStep.op} txId={ctx.txStep.tx} stepIndex={ctx.txStep.index} />;
  if (ctx.edge) {
    const edge = graph.edges.find((e) => e.id === id);
    if (edge) return <EdgeDetail edge={edge} />;
  }
  if (id === CLIENT_NODE_ID) return <ClientDetail />;
  if (id.startsWith(EXTERNAL_PREFIX)) return <ExternalDetail name={id.slice(EXTERNAL_PREFIX.length)} />;

  const entry = index.get(id);
  if (!entry) return <Frame kind="unknown" title={id} />;

  switch (entry.kind) {
    case "service": return <ServiceDetail id={id} />;
    case "operation": return <OperationDetail id={id} />;
    case "topic": return <TopicDetail id={id} />;
    case "schema": return <SchemaDetail id={id} />;
    case "data_model": return <DataModelDetail id={id} />;
    case "object": return <ObjectDetail dmId={entry.dataModel} id={id} />;
    case "machine": return <MachineDetail id={id} />;
    case "state": return <StateDetail mId={entry.machine} id={id} />;
    case "transition": return <TransitionDetail mId={entry.machine} id={id} />;
    case "input": return <InputDetail opId={entry.op} id={id} />;
    case "effect": return <EffectDetail id={id} />;
    case "intent": return <IntentDetail opId={entry.op} id={id} />;
    case "result": return <ResultDetail opId={entry.op} id={id} />;
    case "response": return <ResponseDetail opId={entry.op} id={id} />;
    case "transaction": return <TransactionDetail opId={entry.op} id={id} />;
    case "flow": return <FlowDetail opId={entry.op} id={id} />;
  }
}

function ServiceDetail({ id }: { id: Id }) {
  const { model, graph } = useApp();
  const svc = model.services[id];
  const ops = graph.operations.filter((o) => o.service === id);
  return (
    <Frame kind="service" title={id} subtitle={<span><Tag>{svc.kind}</Tag> boundary of {ops.length} operation{ops.length === 1 ? "" : "s"}</span>}>
      <Section title="operations" count={ops.length}>
        <List items={ops.map((o) => <NavLink key={o.id} hash={hashes.op(o.id)}>{o.id}</NavLink>)} />
      </Section>
    </Frame>
  );
}

function FlowSummary({ opId, flowId }: { opId: Id; flowId: Id }) {
  const { model, index, navigateTo } = useApp();
  const op = model.operations[opId];
  const flow = op.flows[flowId];
  const effectKind = (effectId: Id) => effectDef(model, index, effectId)?.effect.kind ?? null;
  const kindBadge = (kind: string | null) =>
    kind ? <Badge variant={kind === "publication" ? "purple" : kind === "request" ? "orange" : "warning"}>{kind}</Badge> : null;
  return (
    <div className="space-y-1.5 rounded-md border border-kumo-hairline bg-kumo-elevated/40 p-2.5">
      <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1">
        <button
          type="button"
          className="inline-flex min-w-0 cursor-pointer items-center gap-1.5 text-left font-mono text-[12px] text-kumo-link hover:underline"
          title="Open this flow in the operation view"
          onClick={() => navigateTo(hashes.op(opId, flowId), `flow:${flowId}`)}
        >
          <span className="break-all">{shortId(flowId)}</span>
          <ArrowSquareOutIcon size={12} className="shrink-0" />
        </button>
        <StatusChips obKey={`${opId}/${flowId}`} />
      </div>
      {/* Each step is a fixed number column beside a text column, so a
          long name wraps within its column instead of under the number. */}
      <ol className="space-y-1.5">
        {flow.steps.map((s, i) => {
          const [kind, badge, target, note] =
            s.kind === "transaction"
              ? [
                  "transaction",
                  null,
                  s.transaction,
                  (() => {
                    const n = op.transactions[s.transaction]?.steps.length;
                    return n === undefined ? "? steps" : `${n} step${n === 1 ? "" : "s"}`;
                  })(),
                ]
              : s.kind === "execute_effect"
                ? ["execute effect", kindBadge(effectKind(s.effect)), s.effect, null]
                : [
                    "execute intent",
                    kindBadge(op.effect_intents[s.intent] ? effectKind(op.effect_intents[s.intent].effect) : null),
                    s.intent,
                    null,
                  ];
          return (
            <li key={i} className="flex items-start gap-2 text-xs">
              <span className="shrink-0"><Tag>{i + 1}</Tag></span>
              <div className="min-w-0 flex-1 space-y-0.5">
                <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                  <span className="text-kumo-subtle">{kind}</span>
                  {badge}
                </div>
                <div className="min-w-0">
                  <IdLink id={target}>{shortId(target)}</IdLink>
                  {note && <span className="ml-1.5 text-kumo-inactive">{note}</span>}
                </div>
              </div>
            </li>
          );
        })}
      </ol>
      <div className="text-xs text-kumo-subtle">
        terminal response: {flow.response ? <IdLink id={flow.response}>{shortId(flow.response)}</IdLink> : "none"}
      </div>
    </div>
  );
}

function OperationDetail({ id }: { id: Id }) {
  const { model, graph, index, navigateTo } = useApp();
  const op = model.operations[id];
  const node = graph.operations.find((o) => o.id === id);
  const inputs = Object.entries(op.inputs);
  const effects = Object.keys(op.effects);
  const flows = Object.keys(op.flows);
  const reqs = op.requirements;
  const reqRows: ReactNode[] = [];
  reqs.serialization.forEach((r, i) => reqRows.push(
    <div key={`s${i}`} className="flex flex-wrap items-center gap-1.5"><Tag variant="blue">serialization</Tag><RefText value={r.key} /></div>));
  reqs.ordering.forEach((r, i) => reqRows.push(
    <div key={`o${i}`} className="flex flex-wrap items-center gap-1.5"><Tag variant="purple">ordering</Tag><RefText value={r.key} /></div>));
  reqs.idempotency.forEach((r, i) => reqRows.push(
    <div key={`i${i}`} className="flex flex-wrap items-center gap-1.5">
      <Tag variant="orange">idempotency</Tag>{r.response === "replay_consistent" && <Tag variant="info">replay_consistent</Tag>}
      <KeyComponents value={r.key} />
    </div>));
  reqs.recoverability.forEach((r, i) => reqRows.push(
    <div key={`r${i}`} className="flex flex-wrap items-center gap-1.5">
      <Tag variant="success">recoverability</Tag><Tag>{r.completion}</Tag><KeyComponents value={r.key} />
    </div>));

  return (
    <Frame
      kind="operation"
      title={
        <button
          type="button"
          className="inline-flex max-w-full cursor-pointer items-center gap-1.5 text-left text-kumo-link hover:underline"
          title="Open the operation page"
          onClick={() => navigateTo(hashes.op(id))}
        >
          <span className="break-all">{id}</span>
          <ArrowSquareOutIcon size={13} className="shrink-0" />
        </button>
      }
      subtitle={<span>operation on <IdLink id={op.service} /></span>}
      description={op.description}
    >
      <Button variant="secondary" size="xs" icon={ArrowSquareOutIcon} onClick={() => navigateTo(hashes.op(id))}>
        open operation page
      </Button>
      <Section title="execution">
        <KeyValue rows={[["concurrency", concurrencyText(op.execution.concurrency)]]} />
      </Section>
      {flows.length > 0 && (
        <Section title="flows" count={flows.length}>
          {flows.map((flowId) => (
            <FlowSummary key={flowId} opId={id} flowId={flowId} />
          ))}
        </Section>
      )}
      {inputs.length > 0 && (
        <Section title="inputs" count={inputs.length}>
          <List items={inputs.map(([iid, input]) => (
            <span key={iid} className="flex flex-wrap items-center gap-1.5">
              <IdLink id={iid} />
              {input.kind === "request" ? <Tag variant="info">request</Tag> : <Tag variant="blue">sub ← {shortId(input.topic)}</Tag>}
            </span>
          ))} />
        </Section>
      )}
      {effects.length > 0 && (
        <Section title="declared effects" count={effects.length}>
          <List items={effects.map((eid) => (
            <div key={eid}><IdLink id={eid} /><div className="text-xs text-kumo-subtle">{effectSummary(model, index, eid)}</div></div>
          ))} />
        </Section>
      )}
      {node && node.machines.length > 0 && (
        <Section title="state machines" count={node.machines.length}>
          <List items={node.machines.map((m) => <NavLink key={m} hash={hashes.machine(m)}>{m}</NavLink>)} />
        </Section>
      )}
      {reqRows.length > 0 && (
        <Section title="requirements" count={reqRows.length}>
          <List items={reqRows} />
        </Section>
      )}
      <Obligations obKey={id} />
    </Frame>
  );
}

function TopicDetail({ id }: { id: Id }) {
  const { model, graph } = useApp();
  const topic = model.topics[id];
  const pubs = graph.edges.filter((e): e is Extract<Edge, { kind: "publish" }> => e.kind === "publish" && e.to === id);
  const subs = graph.edges.filter((e): e is Extract<Edge, { kind: "subscribe" }> => e.kind === "subscribe" && e.from === id);
  return (
    <Frame kind="topic" title={id} subtitle={<span>topic · ordering <Tag>{topic.ordering.kind}</Tag> · identity <Tag>{topic.message_identity.kind}</Tag></span>}>
      <Section title="message schemas" count={topic.messages.length}>
        <List items={topic.messages.map((s) => <IdLink key={s} id={s} />)} />
      </Section>
      {topic.ordering.kind === "keyed" && (
        <Section title="ordering key mapping">
          <KeyValue rows={Object.entries(topic.ordering.mapping).map(([schema, path]) => [shortId(schema), <Mono key={schema}>{pathText(path)}</Mono>])} />
        </Section>
      )}
      {topic.message_identity.kind === "keyed" && (
        <Section title="message identity mapping">
          <KeyValue rows={Object.entries(topic.message_identity.mapping).map(([schema, tuple]) => [shortId(schema), <Mono key={schema}>{tuple.map(pathText).join(", ")}</Mono>])} />
        </Section>
      )}
      {pubs.length > 0 && (
        <Section title="publishers" count={pubs.length}>
          <List items={pubs.map((e) => <span key={e.id} className="flex flex-wrap items-center gap-1.5"><IdLink id={e.operation} /><Tag variant="purple">{shortId(e.schema)}</Tag></span>)} />
        </Section>
      )}
      {subs.length > 0 && (
        <Section title="subscribers" count={subs.length}>
          <List items={subs.map((e) => <span key={e.id} className="flex flex-wrap items-center gap-1.5"><IdLink id={e.operation} /><Tag>{e.delivery}</Tag><Tag>{e.routing}</Tag></span>)} />
        </Section>
      )}
      <Obligations obKey={id} />
    </Frame>
  );
}

function SchemaDetail({ id }: { id: Id }) {
  const { model } = useApp();
  const schema = model.schemas[id];
  if (schema.kind === "canonical") {
    const fields = Object.entries(schema.fields);
    return (
      <Frame kind="schema" title={id} subtitle={<span>canonical schema · <Tag>{schema.completeness}</Tag></span>} description={schema.description}>
        <Section title="fields" count={fields.length}>
          <List items={fields.map(([name, f]) => (
            <span key={name} className="flex flex-wrap items-center gap-1.5">
              <Mono>{name}</Mono><span className="text-kumo-inactive">:</span><TypeView ty={f.ty} />
              {f.optional && <Tag>optional</Tag>}
            </span>
          ))} />
        </Section>
      </Frame>
    );
  }
  return (
    <Frame kind="schema" title={id} subtitle={<span>fragment of <IdLink id={schema.source} /></span>}>
      <Section title="mapping">
        <KeyValue rows={Object.entries(schema.mapping).map(([name, path]) => [name, <Mono key={name}>{pathText(path)}</Mono>])} />
      </Section>
    </Frame>
  );
}

function DataModelDetail({ id }: { id: Id }) {
  const { model } = useApp();
  const objects = Object.keys(model.data_models[id].objects);
  return (
    <Frame kind="data model" title={id} subtitle="transactional state boundary">
      <Section title="objects" count={objects.length}>
        <List items={objects.map((o) => <IdLink key={o} id={o} />)} />
      </Section>
    </Frame>
  );
}

function ObjectDetail({ dmId, id }: { dmId: Id; id: Id }) {
  const { model } = useApp();
  const obj = model.data_models[dmId].objects[id];
  return (
    <Frame kind="data object" title={id} subtitle={<span>persistent object in <IdLink id={dmId} /></span>}>
      <KeyValue rows={[["schema", <IdLink key="s" id={obj.schema} />], ["identity", <Mono key="i">{obj.identity.map(pathText).join(", ")}</Mono>]]} />
      {obj.requirements.history.length > 0 && (
        <Section title="history requirements">
          <div className="flex gap-1.5">{obj.requirements.history.map((h) => <Tag key={h} variant="info">{h}</Tag>)}</div>
        </Section>
      )}
      <Obligations obKey={`${dmId}/${id}`} />
    </Frame>
  );
}

function MachineDetail({ id }: { id: Id }) {
  const { model } = useApp();
  const m = model.state_machines[id];
  const transitions = Object.keys(m.transitions);
  return (
    <Frame kind="state machine" title={id} subtitle={<span>governs <IdLink id={m.subject.object} /> · field <Mono>{pathText(m.subject.state)}</Mono></span>}>
      <NavLink hash={hashes.machine(id)}>open state graph →</NavLink>
      <Section title="states" count={m.states.length}>
        <List items={m.states.map((s) => <span key={s} className="flex items-center gap-1.5"><IdLink id={s} />{s === m.initial && <Tag variant="success">initial</Tag>}</span>)} />
      </Section>
      <Section title="transitions" count={transitions.length}>
        <List items={transitions.map((t) => <NavLink key={t} hash={hashes.machine(id, t)} selection={`t:${t}`}>{t}</NavLink>)} />
      </Section>
      <Obligations obKey={id} />
    </Frame>
  );
}

function StateDetail({ mId, id }: { mId: Id; id: Id }) {
  const { model } = useApp();
  const m = model.state_machines[mId];
  const into: Id[] = [];
  const outOf: Id[] = [];
  for (const [tId, t] of Object.entries(m.transitions)) {
    if (t.to === id) into.push(tId);
    if (t.from.includes(id)) outOf.push(tId);
  }
  return (
    <Frame kind="state" title={id} subtitle={<span>state of <IdLink id={mId} />{id === m.initial && <> · <Tag variant="success">initial</Tag></>}</span>}>
      {outOf.length > 0 && <Section title="transitions out" count={outOf.length}><List items={outOf.map((t) => <IdLink key={t} id={t} />)} /></Section>}
      {into.length > 0 && <Section title="transitions in" count={into.length}><List items={into.map((t) => <IdLink key={t} id={t} />)} /></Section>}
    </Frame>
  );
}

function TransitionDetail({ mId, id }: { mId: Id; id: Id }) {
  const { model, graph } = useApp();
  const t = model.state_machines[mId].transitions[id];
  const fx = Object.entries(t.side_effects);
  const refs = graph.transition_refs[`${mId}/${id}`] ?? [];
  return (
    <Frame kind="transition" title={id} subtitle={<span>transition of <IdLink id={mId} /></span>}>
      <KeyValue rows={[["from", <Mono key="f">{t.from.join(", ")}</Mono>], ["to", <Mono key="t">{t.to}</Mono>]]} />
      {fx.length > 0 && (
        <Section title="side effects" count={fx.length}>
          <List items={fx.map(([eid, e]) => (
            <div key={eid} className="space-y-0.5">
              <IdLink id={eid} />
              <div className="text-xs text-kumo-subtle">
                {e.kind === "publication" ? <>publish <IdLink id={e.schema} /> → <IdLink id={e.topic} /></> : <>request → <IdLink id={e.target.operation} /></>}
              </div>
              {intentExecutors(model, eid).map((x) => (
                <div key={x.intent} className="text-xs text-kumo-subtle">executed by <IdLink id={x.op} /> via <IdLink id={x.intent} /></div>
              ))}
            </div>
          ))} />
        </Section>
      )}
      {refs.length > 0 && (
        <Section title="taken by transactions" count={refs.length}>
          <List items={refs.map((r, i) => {
            const owner = model.operations[r.operation];
            const flow = owner ? flowContaining(owner, r.transaction) : null;
            return (
              <span key={i}><IdLink id={r.transaction} /> step {r.step + 1} in <NavLink hash={hashes.op(r.operation, flow)} selection={`tx:${r.transaction}`}>{shortId(r.operation)}</NavLink></span>
            );
          })} />
        </Section>
      )}
      <Obligations obKey={`${mId}/${id}`} />
    </Frame>
  );
}

function InputDetail({ opId, id }: { opId: Id; id: Id }) {
  const { model } = useApp();
  const input = model.operations[opId].inputs[id];
  if (input.kind === "request") {
    return (
      <Frame kind="input" title={id} subtitle={<span>request input of <IdLink id={opId} /></span>}>
        <KeyValue rows={[
          ["schema", <IdLink key="s" id={input.schema} />],
          ["identity", input.identity.kind === "keyed"
            ? <Mono key="i">{input.identity.fields.map(pathText).join(", ")}</Mono>
            : <Tag key="i" variant="warning">unspecified</Tag>],
        ]} />
      </Frame>
    );
  }
  const schemas = input.messages.kind === "all" ? null : input.messages.schemas;
  return (
    <Frame kind="input" title={id} subtitle={<span>subscription of <IdLink id={opId} /></span>}>
      <KeyValue rows={[
        ["topic", <IdLink key="t" id={input.topic} />],
        ["delivery", <Tag key="d">{input.delivery}</Tag>],
        ["routing", <Tag key="r">{input.dispatch.routing}</Tag>],
        ["lane concurrency", concurrencyText(input.dispatch.lane_concurrency)],
      ]} />
      <Section title="consumed messages">
        {schemas ? <List items={schemas.map((s) => <IdLink key={s} id={s} />)} /> : <Tag>all topic messages</Tag>}
      </Section>
    </Frame>
  );
}

function EffectDetail({ id }: { id: Id }) {
  const { model, index } = useApp();
  const def = effectDef(model, index, id);
  if (!def) return <Frame kind="effect" title={id} />;
  const e = def.effect;
  const owner = def.owner.op !== undefined
    ? <span>declared by <IdLink id={def.owner.op} /></span>
    : <span>owned by transition <IdLink id={def.owner.transition!} /> of <IdLink id={def.owner.machine!} /></span>;
  const executors = intentExecutors(model, id);
  return (
    <Frame kind="effect" title={id} subtitle={owner}>
      {e.kind === "publication" && (
        <>
          <KeyValue rows={[["kind", <Tag key="k" variant="purple">publication</Tag>], ["topic", <IdLink key="t" id={e.topic} />], ["schema", <IdLink key="s" id={e.schema} />]]} />
          <Propagation items={e.idempotency_key_propagation} />
        </>
      )}
      {e.kind === "request" && (
        <>
          <KeyValue rows={[["kind", <Tag key="k" variant="orange">request</Tag>], ["operation", <IdLink key="o" id={e.target.operation} />], ["input", <IdLink key="i" id={e.target.input} />], ["schema", <IdLink key="s" id={e.schema} />], ["retry", <Tag key="r">{e.retry}</Tag>]]} />
          <Propagation items={e.idempotency_key_propagation} />
        </>
      )}
      {e.kind === "external" && (
        <>
          <KeyValue rows={[["kind", <Tag key="k" variant="warning">external</Tag>], ["name", <Mono key="n">{e.name}</Mono>], ["idempotency", <Tag key="i">{e.idempotency.kind}</Tag>]]} />
          {e.idempotency.kind === "deduplicated_by" && (
            <Section title="deduplication key"><KeyComponents value={e.idempotency.key} /></Section>
          )}
        </>
      )}
      {executors.length > 0 && (
        <Section title="executed via intents" count={executors.length}>
          <List items={executors.map((x) => <span key={x.intent}><IdLink id={x.intent} /> in <IdLink id={x.op} /></span>)} />
        </Section>
      )}
    </Frame>
  );
}

function IntentDetail({ opId, id }: { opId: Id; id: Id }) {
  const { model, index } = useApp();
  const intent = model.operations[opId].effect_intents[id];
  const owner = index.get(intent.effect);
  const viaTransition = owner && owner.kind === "effect" && owner.machine !== undefined;
  return (
    <Frame kind="effect intent" title={id} subtitle={<span>intent of <IdLink id={opId} /></span>}
      description={viaTransition ? <>The effect is owned by transition <IdLink id={owner.transition!} />; a successful transition implicitly establishes this intent.</> : undefined}>
      <KeyValue rows={[["effect", <IdLink key="e" id={intent.effect} />], ["resolves to", effectSummary(model, index, intent.effect)]]} />
    </Frame>
  );
}

function ResultDetail({ opId, id }: { opId: Id; id: Id }) {
  const { model } = useApp();
  const r = model.operations[opId].invocation_results[id];
  return (
    <Frame kind="invocation result" title={id} subtitle={<span>logical artifact of <IdLink id={opId} /></span>}>
      <KeyValue rows={[["schema", <IdLink key="s" id={r.schema} />]]} />
    </Frame>
  );
}

function ResponseDetail({ opId, id }: { opId: Id; id: Id }) {
  const { model } = useApp();
  const r = model.operations[opId].responses[id];
  return (
    <Frame kind="response" title={id} subtitle={<span>response of <IdLink id={opId} /></span>}>
      <KeyValue rows={[
        ["request", <IdLink key="r" id={r.request} />],
        ["schema", <IdLink key="s" id={r.schema} />],
        ["source", r.source.kind === "invocation_result" ? <IdLink key="src" id={r.source.result} /> : <Tag key="src" variant="warning">unspecified</Tag>],
      ]} />
    </Frame>
  );
}

function TransactionDetail({ opId, id }: { opId: Id; id: Id }) {
  const { model, openDetail } = useApp();
  const tx = model.operations[opId].transactions[id];
  return (
    <Frame kind="transaction" title={id} subtitle={<span>transaction of <IdLink id={opId} /></span>}>
      <KeyValue rows={[
        ["data model", tx.data_model ? <IdLink key="d" id={tx.data_model} /> : <Muted key="d">none (framework artifacts only)</Muted>],
        ["isolation", <Tag key="i">{tx.isolation}</Tag>],
        ["idempotency", <Tag key="k" variant={tx.idempotency.kind === "deduplicated_by" ? "success" : "warning"}>{tx.idempotency.kind}</Tag>],
      ]} />
      {tx.idempotency.kind === "deduplicated_by" && (
        <Section title="commit key"><KeyComponents value={tx.idempotency.key} /></Section>
      )}
      <Section title="steps" count={tx.steps.length}>
        <List items={tx.steps.map((s, i) => (
          <button key={i} type="button" className="flex w-full cursor-pointer items-center gap-2 text-left hover:underline"
            onClick={() => openDetail(id, { txStep: { op: opId, tx: id, index: i } })}>
            <Tag>{i + 1}</Tag>
            <span className="text-sm">{s.kind}</span>
            {s.kind === "transition" && <Mono className="text-kumo-subtle">{shortId(s.transition)}</Mono>}
            {(s.kind === "read" || s.kind === "write" || s.kind === "delete" || s.kind === "lock") && <Mono className="text-kumo-subtle">{shortId(s.target.object)}</Mono>}
            {s.kind === "insert" && <Mono className="text-kumo-subtle">{shortId(s.object)}</Mono>}
          </button>
        ))} />
      </Section>
      <Obligations obKey={`${opId}/${id}`} />
    </Frame>
  );
}

function FlowDetail({ opId, id }: { opId: Id; id: Id }) {
  const { model } = useApp();
  const flow = model.operations[opId].flows[id];
  return (
    <Frame kind="invocation flow" title={id} subtitle={<span>flow of <NavLink hash={hashes.op(opId, id)} selection={`flow:${id}`}>{shortId(opId)}</NavLink></span>}>
      <Section title="steps" count={flow.steps.length}>
        <List items={flow.steps.map((s, i) => (
          <span key={i} className="flex items-center gap-2"><Tag>{i + 1}</Tag><span className="text-sm">{s.kind}</span>
            <IdLink id={s.kind === "transaction" ? s.transaction : s.kind === "execute_effect" ? s.effect : s.intent} />
          </span>
        ))} />
      </Section>
      <KeyValue rows={[["terminal response", flow.response ? <IdLink key="r" id={flow.response} /> : <Muted key="r">none</Muted>]]} />
      <Obligations obKey={`${opId}/${id}`} />
    </Frame>
  );
}

function RequirementDetail({ opId, prop, reqIndex }: { opId: Id; prop: RequirementKind; reqIndex: number }) {
  const { model } = useApp();
  const reqs = model.operations[opId].requirements;
  const rows: ReactNode[] = [];
  if (prop === "serialization" || prop === "ordering") {
    const r = reqs[prop][reqIndex];
    rows.push(<RefText key="k" value={r.key} />);
  } else if (prop === "idempotency") {
    const r = reqs.idempotency[reqIndex];
    rows.push(<KeyComponents key="k" value={r.key} />);
  } else {
    const r = reqs.recoverability[reqIndex];
    rows.push(<KeyComponents key="k" value={r.key} />);
  }
  const extra: [string, ReactNode][] = prop === "idempotency"
    ? [["response", <Tag key="r" variant={reqs.idempotency[reqIndex].response === "replay_consistent" ? "info" : "neutral"}>{reqs.idempotency[reqIndex].response}</Tag>]]
    : prop === "recoverability"
      ? [["completion", <Tag key="c">{reqs.recoverability[reqIndex].completion}</Tag>]]
      : [];
  return (
    <Frame kind="requirement" title={`${prop} #${reqIndex}`} subtitle={<span>declared on <IdLink id={opId} /></span>}>
      <Section title="key"><List items={rows} /></Section>
      {extra.length > 0 && <KeyValue rows={extra} />}
      <Obligations obKey={opId} filter={(ob) => ob.subject.kind === "operation" && ob.subject.requirement === reqIndex && propertyMatchesRequirement(ob.property, prop)} />
    </Frame>
  );
}

function TxStepDetail({ opId, txId, stepIndex }: { opId: Id; txId: Id; stepIndex: number }) {
  const { model } = useApp();
  const step = model.operations[opId].transactions[txId].steps[stepIndex];
  const sub = <span>step {stepIndex + 1} of <IdLink id={txId} /> in <IdLink id={opId} /></span>;
  switch (step.kind) {
    case "read":
      return (
        <Frame kind="transaction step" title={`read · ${step.result}`} subtitle={sub}>
          <KeyValue rows={[["object", <IdLink key="o" id={step.target.object} />], ["predicate", <PredicateView key="p" predicate={step.target.predicate} />]]} />
          <Section title="fields read">
            {step.fields.kind === "all" ? <Tag>all fields</Tag> : <List items={step.fields.fields.map((f, i) => <Mono key={i}>{pathText(f)}</Mono>)} />}
          </Section>
        </Frame>
      );
    case "write":
      return (
        <Frame kind="transaction step" title={`write · ${shortId(step.target.object)}`} subtitle={sub}>
          <KeyValue rows={[["object", <IdLink key="o" id={step.target.object} />], ["predicate", <PredicateView key="p" predicate={step.target.predicate} />]]} />
          <Section title="fields written"><List items={step.fields.map((f, i) => <Mono key={i}>{pathText(f)}</Mono>)} /></Section>
          <Section title="value provenance"><DerivationView value={step.values} /></Section>
        </Frame>
      );
    case "insert":
      return (
        <Frame kind="transaction step" title={`insert · ${shortId(step.object)}`} subtitle={sub}>
          <KeyValue rows={[["object", <IdLink key="o" id={step.object} />]]} />
          <Section title="value provenance"><DerivationView value={step.values} /></Section>
        </Frame>
      );
    case "delete":
      return (
        <Frame kind="transaction step" title={`delete · ${shortId(step.target.object)}`} subtitle={sub}>
          <KeyValue rows={[["object", <IdLink key="o" id={step.target.object} />], ["predicate", <PredicateView key="p" predicate={step.target.predicate} />]]} />
        </Frame>
      );
    case "lock":
      return (
        <Frame kind="transaction step" title={`lock · ${shortId(step.target.object)}`} subtitle={sub}>
          <KeyValue rows={[["object", <IdLink key="o" id={step.target.object} />], ["mode", <Tag key="m">{step.mode}</Tag>], ["order", <Tag key="r">{step.order.kind}</Tag>], ["predicate", <PredicateView key="p" predicate={step.target.predicate} />]]} />
        </Frame>
      );
    case "transition":
      return (
        <Frame kind="transaction step" title={`transition · ${shortId(step.transition)}`} subtitle={sub}>
          <KeyValue rows={[["machine", <IdLink key="m" id={step.machine} />], ["transition", <IdLink key="t" id={step.transition} />], ["subject", <IdLink key="s" id={step.subject.object} />], ["predicate", <PredicateView key="p" predicate={step.subject.predicate} />]]} />
          {Object.keys(step.effect_values).length > 0 && (
            <Section title="side-effect values">
              {Object.entries(step.effect_values).map(([eid, d]) => (
                <div key={eid} className="space-y-1"><IdLink id={eid} /><DerivationView value={d} /></div>
              ))}
            </Section>
          )}
          <NavLink hash={hashes.machine(step.machine, step.transition)}>view in state machine →</NavLink>
        </Frame>
      );
    case "establish_effect_intent":
      return (
        <Frame kind="transaction step" title={`establish intent · ${shortId(step.intent)}`} subtitle={sub}>
          <KeyValue rows={[["intent", <IdLink key="i" id={step.intent} />]]} />
          <Section title="value provenance"><DerivationView value={step.values} /></Section>
        </Frame>
      );
    case "establish_invocation_result":
      return (
        <Frame kind="transaction step" title={`establish result · ${shortId(step.result)}`} subtitle={sub}>
          <KeyValue rows={[["result", <IdLink key="r" id={step.result} />]]} />
          <Section title="value provenance"><DerivationView value={step.values} /></Section>
        </Frame>
      );
  }
}

function EdgeDetail({ edge: e }: { edge: Edge }) {
  const executed = "executed_by" in e ? (
    <Section title="executed by flows" count={e.executed_by.length}>
      {e.executed_by.length
        ? <List items={e.executed_by.map((f) => <IdLink key={f} id={f} />)} />
        : <span className="flex items-center gap-1.5"><Tag variant="warning">declared, not executed</Tag><Muted>no declared flow executes this effect</Muted></span>}
    </Section>
  ) : null;
  const via = "via_transition" in e && e.via_transition ? (
    <div className="text-sm text-kumo-subtle">
      Owned by transition <IdLink id={e.via_transition.transition} /> of <NavLink hash={hashes.machine(e.via_transition.machine)}>{shortId(e.via_transition.machine)}</NavLink>; it becomes intended when the transition commits.
    </div>
  ) : null;
  switch (e.kind) {
    case "publish":
      return (
        <Frame kind="publication edge" title={<span><IdLink id={e.operation} /> → <IdLink id={e.to} /></span>}>
          <KeyValue rows={[["effect", <IdLink key="e" id={e.effect} />], ["schema", <IdLink key="s" id={e.schema} />]]} />
          {via}{executed}
        </Frame>
      );
    case "subscribe":
      return (
        <Frame kind="subscription edge" title={<span><IdLink id={e.from} /> → <IdLink id={e.operation} /></span>}>
          <KeyValue rows={[["input", <IdLink key="i" id={e.input} />], ["delivery", <Tag key="d">{e.delivery}</Tag>], ["routing", <Tag key="r">{e.routing}</Tag>], ["lane concurrency", e.lane_concurrency]]} />
          <Section title="consumed messages" count={e.schemas.length}><List items={e.schemas.map((s) => <IdLink key={s} id={s} />)} /></Section>
        </Frame>
      );
    case "request":
      return (
        <Frame kind="request edge" title={<span><IdLink id={e.operation} /> → <IdLink id={e.to} /></span>}>
          <KeyValue rows={[["effect", <IdLink key="e" id={e.effect} />], ["target input", <IdLink key="i" id={e.input} />], ["schema", <IdLink key="s" id={e.schema} />], ["retry", <Tag key="r">{e.retry}</Tag>]]} />
          {via}{executed}
        </Frame>
      );
    case "external":
      return (
        <Frame kind="external effect edge" title={<span><IdLink id={e.operation} /> → {e.to.slice(EXTERNAL_PREFIX.length)}</span>}
          description="The modeled system ends here; the checker cannot inspect the external implementation.">
          <KeyValue rows={[["effect", <IdLink key="e" id={e.effect} />], ["idempotency", <Tag key="i">{e.idempotency}</Tag>]]} />
          {executed}
        </Frame>
      );
    case "client":
      return (
        <Frame kind="client request" title={<span>clients → <IdLink id={e.operation} /></span>}
          description="No modeled operation issues this request; it enters the system from unmodeled callers.">
          <KeyValue rows={[["input", <IdLink key="i" id={e.input} />], ["schema", <IdLink key="s" id={e.schema} />]]} />
        </Frame>
      );
  }
}

function ClientDetail() {
  const { graph } = useApp();
  const edges = graph.edges.filter((e): e is Extract<Edge, { kind: "client" }> => e.kind === "client");
  return (
    <Frame kind="clients" title="unmodeled callers" description="Request inputs that no modeled operation invokes; they are the system's entry points.">
      <Section title="entry points" count={edges.length}>
        <List items={edges.map((e) => <span key={e.id} className="flex flex-wrap items-center gap-1.5"><IdLink id={e.operation} /><Tag>{shortId(e.schema)}</Tag></span>)} />
      </Section>
    </Frame>
  );
}

function ExternalDetail({ name }: { name: string }) {
  const { graph } = useApp();
  const edges = graph.edges.filter((e): e is Extract<Edge, { kind: "external" }> => e.kind === "external" && e.to === EXTERNAL_PREFIX + name);
  return (
    <Frame kind="external system" title={name} description="External dependency; the modeled system ends here.">
      <Section title="invoked by" count={edges.length}>
        <List items={edges.map((e) => <span key={e.id} className="flex flex-wrap items-center gap-1.5"><IdLink id={e.operation} /> via <IdLink id={e.effect} /><Tag>{e.idempotency}</Tag></span>)} />
      </Section>
    </Frame>
  );
}
