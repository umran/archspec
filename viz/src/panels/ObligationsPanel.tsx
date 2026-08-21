import { Badge } from "@cloudflare/kumo/components/badge";
import { Button } from "@cloudflare/kumo/components/button";
import { Collapsible } from "@cloudflare/kumo/components/collapsible";
import { Empty } from "@cloudflare/kumo/components/empty";
import { Input } from "@cloudflare/kumo/components/input";
import { Tabs } from "@cloudflare/kumo/components/tabs";
import { Text } from "@cloudflare/kumo/components/text";
import { CaretRightIcon, ListChecksIcon, XIcon } from "@phosphor-icons/react";
import { useMemo, useState } from "react";

import { shortId } from "../lib/ids";
import { STATUS_GLYPH, STATUS_ORDER, statusCounts, subjectGroup, subjectText, worstStatus } from "../lib/obligations";
import { useApp } from "../state/AppState";
import { propertyName, type Obligation, type Status } from "../types/report";
import { ObligationCard } from "./ObligationCard";
import { StatusBadge } from "./parts";

type Filter = "all" | Status;

export function ObligationsPanel() {
  const { report, setObligationsOpen } = useApp();
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");

  const all = report?.obligations ?? [];
  const counts = statusCounts(all);

  const groups = useMemo(() => {
    const q = query.trim().toLowerCase();
    const visible = all.filter(
      (ob) =>
        (filter === "all" || ob.status === filter) &&
        (!q ||
          ob.id.toLowerCase().includes(q) ||
          ob.summary.toLowerCase().includes(q) ||
          subjectText(ob.subject).toLowerCase().includes(q) ||
          propertyName(ob.property).includes(q)),
    );
    const byGroup = new Map<string, Obligation[]>();
    for (const ob of visible) {
      const g = subjectGroup(ob.subject);
      const list = byGroup.get(g);
      if (list) list.push(ob);
      else byGroup.set(g, [ob]);
    }
    return [...byGroup.entries()]
      .map(([id, obs]) => ({
        id,
        obs: [...obs].sort((a, b) => STATUS_ORDER[a.status] - STATUS_ORDER[b.status] || a.id.localeCompare(b.id)),
      }))
      .sort((a, b) => STATUS_ORDER[worstStatus(a.obs)!] - STATUS_ORDER[worstStatus(b.obs)!] || a.id.localeCompare(b.id));
  }, [all, filter, query]);

  if (!report) return null;

  const tabs = [
    { value: "all", label: `all ${all.length}` },
    { value: "unknown", label: `${STATUS_GLYPH.unknown} ${counts.unknown ?? 0}` },
    { value: "proven", label: `${STATUS_GLYPH.proven} ${counts.proven ?? 0}` },
  ];
  if (counts.disproven) tabs.splice(1, 0, { value: "disproven", label: `${STATUS_GLYPH.disproven} ${counts.disproven}` });

  return (
    <div className="flex h-full flex-col">
      <header className="flex shrink-0 items-center justify-between border-b border-kumo-hairline px-4 py-2">
        <span className="flex items-center gap-2">
          <ListChecksIcon size={16} className="text-kumo-subtle" />
          <span className="uppercase tracking-wider">
            <Text variant="secondary" size="xs" as="span">
              obligations
            </Text>
          </span>
        </span>
        <Button variant="ghost" size="xs" shape="square" icon={XIcon} aria-label="Close" onClick={() => setObligationsOpen(false)} />
      </header>
      <div className="shrink-0 space-y-2 border-b border-kumo-hairline px-4 py-2.5">
        <Tabs variant="segmented" size="sm" tabs={tabs} value={filter} onValueChange={(v) => setFilter(v as Filter)} />
        <Input size="sm" placeholder="filter obligations…" value={query} onChange={(e) => setQuery(e.target.value)} />
      </div>
      <div className="flex-1 space-y-3 overflow-y-auto px-4 py-3">
        {groups.length === 0 && (
          <Empty size="sm" title="no obligations match" description="Adjust the status filter or the search." />
        )}
        {groups.map((group) => (
          <ObligationGroup key={group.id} id={group.id} obs={group.obs} />
        ))}
      </div>
    </div>
  );
}

function ObligationGroup({ id, obs }: { id: string; obs: Obligation[] }) {
  const [open, setOpen] = useState(true);
  const counts = statusCounts(obs);
  return (
    <Collapsible.Root open={open} onOpenChange={setOpen}>
      <Collapsible.Trigger className="flex w-full cursor-pointer items-center justify-between gap-2 rounded-md px-1 py-1 text-left hover:bg-kumo-tint">
        <span className="flex min-w-0 items-center gap-1.5">
          <CaretRightIcon size={12} className={`shrink-0 text-kumo-inactive transition-transform ${open ? "rotate-90" : ""}`} />
          <span className="truncate font-mono text-[12px] font-semibold text-kumo-strong">{shortId(id)}</span>
        </span>
        <span className="flex shrink-0 items-center gap-1">
          {(["disproven", "unknown", "proven"] as const).map((s) =>
            counts[s] ? (
              <Badge key={s} variant={s === "proven" ? "success" : s === "disproven" ? "error" : "warning"} appearance="dot">
                {counts[s]}
              </Badge>
            ) : null,
          )}
        </span>
      </Collapsible.Trigger>
      <Collapsible.Panel>
        <div className="space-y-2 pl-1 pt-2">
          {obs.map((ob) => (
            <ObligationCard key={ob.id} ob={ob} />
          ))}
        </div>
      </Collapsible.Panel>
    </Collapsible.Root>
  );
}

export function ObligationsSummaryBadge() {
  const { report } = useApp();
  if (!report) return null;
  const counts = statusCounts(report.obligations);
  return (
    <span className="flex items-center gap-1">
      {(["disproven", "unknown", "proven"] as const).map((s) =>
        counts[s] ? <StatusBadge key={s} status={s} /> : null,
      )}
    </span>
  );
}
