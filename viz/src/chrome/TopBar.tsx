import { Badge } from "@cloudflare/kumo/components/badge";
import { Breadcrumbs } from "@cloudflare/kumo/components/breadcrumbs";
import { Button } from "@cloudflare/kumo/components/button";
import { Input } from "@cloudflare/kumo/components/input";
import { Switch } from "@cloudflare/kumo/components/switch";
import { Tooltip } from "@cloudflare/kumo/components/tooltip";
import { ListChecksIcon, MoonIcon, SunIcon } from "@phosphor-icons/react";

import { shortId } from "../lib/ids";
import { STATUS_GLYPH, statusCounts } from "../lib/obligations";
import { hashes } from "../lib/route";
import { useApp } from "../state/AppState";

/** The top bar sits in the content column and uses the pages' container,
 *  so its edges line up with the page headers below it. */
export function TopBar() {
  const app = useApp();
  const { data, model, report, route, search, overlay, obligationsOpen, theme } = app;
  const counts = report ? statusCounts(report.obligations) : null;
  const machines = Object.keys(model.state_machines);
  const tally = counts
    ? (["disproven", "unknown", "proven"] as const)
        .filter((s) => counts[s])
        .map((s) => `${STATUS_GLYPH[s]}${counts[s]}`)
        .join(" ")
    : "";

  return (
    <header className="shrink-0 border-b border-kumo-hairline bg-kumo-base">
      <div className="mx-auto flex h-12 max-w-[1240px] items-center gap-4 px-6">
        <div className="flex shrink-0 items-baseline gap-2">
          <span className="font-mono text-sm font-semibold tracking-wide text-kumo-brand">archspec</span>
          <span className="text-sm font-semibold text-kumo-strong">{data.title}</span>
          <Badge variant="neutral">rev {model.revision}</Badge>
          {report && report.model_revision !== null && report.model_revision !== model.revision && (
            <Badge variant="warning">report @ rev {report.model_revision}</Badge>
          )}
        </div>

        <div className="flex min-w-0 flex-1 items-center gap-3 overflow-hidden">
          <Breadcrumbs size="sm">
            {route.view === "system" ? (
              <Breadcrumbs.Current>system</Breadcrumbs.Current>
            ) : (
              <>
                <Breadcrumbs.Link href={hashes.system()}>system</Breadcrumbs.Link>
                <Breadcrumbs.Separator />
                <Breadcrumbs.Current>{route.view === "op" ? "operation" : "machine"}</Breadcrumbs.Current>
                <Breadcrumbs.Separator />
                {route.view === "op" && route.flow ? (
                  <>
                    <Breadcrumbs.Link href={hashes.op(route.id)}>{route.id}</Breadcrumbs.Link>
                    <Breadcrumbs.Separator />
                    <Breadcrumbs.Current>{shortId(route.flow)}</Breadcrumbs.Current>
                  </>
                ) : (
                  <Breadcrumbs.Current>{route.id}</Breadcrumbs.Current>
                )}
              </>
            )}
          </Breadcrumbs>
          {route.view === "system" && machines.length > 0 && (
            <span className="flex items-center gap-1.5 text-xs text-kumo-subtle">
              machines:
              {machines.map((m) => (
                <Button key={m} variant="ghost" size="xs" onClick={() => app.navigateTo(hashes.machine(m))}>
                  {shortId(m)}
                </Button>
              ))}
            </span>
          )}
        </div>

        <div className="flex shrink-0 items-center gap-3">
          {/* The filter dims vertices in the system graph; it has no effect
              on the other pages, so it only appears where it acts. */}
          {route.view === "system" && (
            <Input
              size="sm"
              className="w-44"
              placeholder="filter ids…"
              value={search}
              onChange={(e) => app.setSearch(e.target.value)}
              aria-label="Filter ids"
            />
          )}
          {report && (
            <>
              <span title="Colour operations, transitions and steps by their proof status">
                <Switch
                  size="sm"
                  label="Verdicts"
                  checked={overlay}
                  onCheckedChange={(checked) => app.setOverlay(checked)}
                />
              </span>
              <Button
                variant={obligationsOpen ? "primary" : "secondary"}
                size="sm"
                icon={ListChecksIcon}
                aria-pressed={obligationsOpen}
                aria-expanded={obligationsOpen}
                title={obligationsOpen ? "Close the obligations panel" : "Open the obligations panel"}
                onClick={() => app.setObligationsOpen(!obligationsOpen)}
              >
                Obligations
                {tally && <span className={`ml-1.5 font-mono text-xs ${obligationsOpen ? "opacity-80" : "text-kumo-subtle"}`}>{tally}</span>}
              </Button>
            </>
          )}
          <Tooltip
            content={theme === "dark" ? "Switch to light mode" : "Switch to dark mode"}
            render={
              <Button
                variant="ghost"
                size="sm"
                shape="square"
                icon={theme === "dark" ? SunIcon : MoonIcon}
                aria-label="Toggle color mode"
                onClick={() => app.setTheme(theme === "dark" ? "light" : "dark")}
              />
            }
          />
        </div>
      </div>
    </header>
  );
}
