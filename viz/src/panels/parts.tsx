import { Badge } from "@cloudflare/kumo/components/badge";
import { Collapsible } from "@cloudflare/kumo/components/collapsible";
import { Text } from "@cloudflare/kumo/components/text";
import { useState, type ReactNode } from "react";

import { pathText } from "../lib/ids";
import { STATUS_GLYPH, statusCounts } from "../lib/obligations";
import { predicateText, typeText } from "../lib/text";
import { useApp, useObligationsAt } from "../state/AppState";
import type {
  Derivation,
  IdempotencyKey,
  SelectorPredicate,
  TypeRef,
  ValueRef,
} from "../types/model";
import type { Status } from "../types/report";

/** A clickable model id that opens its detail. */
export function IdLink({ id, children }: { id: string; children?: ReactNode }) {
  const { openDetail } = useApp();
  return (
    <button
      type="button"
      className="cursor-pointer break-all text-left font-mono text-[12px] text-kumo-link hover:underline"
      onClick={() => openDetail(id)}
    >
      {children ?? id}
    </button>
  );
}

/** A navigation action into another view, optionally applying a selection there. */
export function NavLink({ hash, selection, children }: { hash: string; selection?: string; children: ReactNode }) {
  const { navigateTo } = useApp();
  return (
    <button
      type="button"
      className="cursor-pointer text-left text-sm text-kumo-link hover:underline"
      onClick={() => navigateTo(hash, selection)}
    >
      {children}
    </button>
  );
}

/** A titled, collapsible block of the panel. */
export function Section({
  title, count, children, defaultOpen = true,
}: { title: string; count?: number; children: ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <Collapsible.Root open={open} onOpenChange={setOpen} className="border-t border-kumo-hairline pt-2">
      <Collapsible.DefaultTrigger className="w-full text-xs font-semibold uppercase tracking-wider text-kumo-subtle">
        <span className="flex items-center gap-2">
          {title}
          {count !== undefined && <Badge variant="neutral">{count}</Badge>}
        </span>
      </Collapsible.DefaultTrigger>
      <Collapsible.DefaultPanel>
        <div className="space-y-2 pb-1 pt-1">{children}</div>
      </Collapsible.DefaultPanel>
    </Collapsible.Root>
  );
}

export function KeyValue({ rows }: { rows: [string, ReactNode | null | undefined][] }) {
  return (
    <dl className="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1.5 text-sm">
      {rows
        .filter(([, v]) => v !== null && v !== undefined)
        .map(([k, v]) => (
          <div key={k} className="contents">
            <dt className="text-xs uppercase tracking-wide text-kumo-inactive">{k}</dt>
            <dd className="min-w-0 break-words text-kumo-default">{v}</dd>
          </div>
        ))}
    </dl>
  );
}

export function List({ items }: { items: ReactNode[] }) {
  return (
    <ul className="divide-y divide-kumo-hairline rounded-md border border-kumo-hairline bg-kumo-elevated/40 text-sm">
      {items.map((item, i) => (
        <li key={i} className="px-2.5 py-1.5">
          {item}
        </li>
      ))}
    </ul>
  );
}

export function Mono({ children, className }: { children: ReactNode; className?: string }) {
  return <span className={`font-mono text-[12px] ${className ?? ""}`}>{children}</span>;
}

export function Muted({ children }: { children: ReactNode }) {
  return (
    <Text variant="secondary" size="sm" as="span">
      {children}
    </Text>
  );
}

export function Tag({ children, variant = "neutral" }: { children: ReactNode; variant?: "neutral" | "warning" | "info" | "success" | "purple" | "blue" | "orange" }) {
  return <Badge variant={variant}>{children}</Badge>;
}

export function StatusBadge({ status }: { status: Status }) {
  const variant = status === "proven" ? "success" : status === "disproven" ? "error" : "warning";
  return (
    <Badge variant={variant} appearance="dot">
      {`${STATUS_GLYPH[status]} ${status}`}
    </Badge>
  );
}

/** Compact per-status counts for the obligations anchored to an entity. */
export function StatusChips({ obKey }: { obKey: string }) {
  const { overlay } = useApp();
  const obs = useObligationsAt(obKey);
  if (!overlay || !obs.length) return null;
  const counts = statusCounts(obs);
  return (
    <span className="inline-flex items-center gap-1">
      {(["disproven", "unknown", "proven"] as const).map((s) =>
        counts[s] ? (
          <Badge key={s} variant={s === "proven" ? "success" : s === "disproven" ? "error" : "warning"} appearance="dot">
            {`${STATUS_GLYPH[s]}${counts[s]}`}
          </Badge>
        ) : null,
      )}
    </span>
  );
}

export function RefText({ value }: { value: ValueRef }) {
  return (
    <Mono>
      <span className="text-kumo-subtle">{value.source.kind}(</span>
      <IdLink id={value.source.id} />
      <span className="text-kumo-subtle">).{pathText(value.path)}</span>
    </Mono>
  );
}

export function KeyComponents({ value }: { value: IdempotencyKey }) {
  if (!value.components.length) return <Muted>empty key</Muted>;
  return (
    <span className="inline-flex flex-wrap items-center gap-1">
      {value.components.map((c, i) => (
        <span key={i} className="inline-flex items-center gap-1">
          {i > 0 && <span className="text-kumo-inactive">+</span>}
          <RefText value={c} />
        </span>
      ))}
    </span>
  );
}

export function DerivationView({ value }: { value: Derivation }) {
  if (value.kind !== "deterministic") {
    return <Tag variant="warning">unspecified provenance</Tag>;
  }
  return (
    <div className="space-y-1.5">
      <Tag variant="info">deterministic</Tag>
      <List items={value.from.map((ref, i) => <RefText key={i} value={ref} />)} />
    </div>
  );
}

export function PredicateView({ predicate }: { predicate: SelectorPredicate }) {
  return <Mono className="text-kumo-subtle">{predicateText(predicate)}</Mono>;
}

export function TypeView({ ty }: { ty: TypeRef }) {
  const plain = typeText(ty);
  if (plain !== null) return <Mono className="text-kumo-subtle">{plain}</Mono>;
  if (ty.kind === "schema") return <IdLink id={ty.value} />;
  if (ty.kind === "list") {
    return (
      <Mono>
        <span className="text-kumo-subtle">list&lt;</span>
        <TypeView ty={ty.value} />
        <span className="text-kumo-subtle">&gt;</span>
      </Mono>
    );
  }
  return <Mono className="text-kumo-subtle">{ty.value}</Mono>;
}
