import { Badge } from "@cloudflare/kumo/components/badge";
import { Breadcrumbs } from "@cloudflare/kumo/components/breadcrumbs";
import { Button } from "@cloudflare/kumo/components/button";
import { Input } from "@cloudflare/kumo/components/input";
import { Tooltip } from "@cloudflare/kumo/components/tooltip";
import { ArrowsOutIcon, EyeIcon, EyeSlashIcon, ListChecksIcon, MoonIcon, SunIcon } from "@phosphor-icons/react";

import { shortId } from "../lib/ids";
import { STATUS_GLYPH, statusCounts } from "../lib/obligations";
import { hashes } from "../lib/route";
import { useApp } from "../state/AppState";

export function TopBar() {
  const app = useApp();
  const { data, model, report, route, search, overlay, obligationsOpen, theme } = app;
  const counts = report ? statusCounts(report.obligations) : null;
  const machines = Object.keys(model.state_machines);

  return (
    <header className="flex h-12 shrink-0 items-center gap-4 border-b border-kumo-hairline bg-kumo-base px-4">
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

      <div className="flex shrink-0 items-center gap-2">
        <Input
          size="sm"
          className="w-44"
          placeholder="filter ids…"
          value={search}
          onChange={(e) => app.setSearch(e.target.value)}
          aria-label="Filter ids"
        />
        {report && (
          <>
            <Tooltip
              content={overlay ? "Hide verdict overlay" : "Show verdict overlay"}
              render={
                <Button
                  variant={overlay ? "primary" : "secondary"}
                  size="sm"
                  shape="square"
                  icon={overlay ? EyeIcon : EyeSlashIcon}
                  aria-label="Toggle verdict overlay"
                  onClick={() => app.setOverlay(!overlay)}
                />
              }
            />
            <Button
              variant={obligationsOpen ? "primary" : "secondary"}
              size="sm"
              icon={ListChecksIcon}
              onClick={() => app.setObligationsOpen(!obligationsOpen)}
            >
              {counts
                ? (["disproven", "unknown", "proven"] as const)
                    .filter((s) => counts[s])
                    .map((s) => `${STATUS_GLYPH[s]}${counts[s]}`)
                    .join(" ")
                : "obligations"}
            </Button>
          </>
        )}
        <Tooltip
          content="Fit graph to view"
          render={<Button variant="secondary" size="sm" shape="square" icon={ArrowsOutIcon} aria-label="Fit graph to view" onClick={app.requestFit} />}
        />
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
    </header>
  );
}
