import { TooltipProvider } from "@cloudflare/kumo/components/tooltip";

import { TopBar } from "./chrome/TopBar";
import { MachineView } from "./graph/MachineView";
import { OperationView } from "./graph/OperationView";
import { SystemView } from "./graph/SystemView";
import { DetailPanel } from "./panels/DetailPanel";
import { ObligationsPanel } from "./panels/ObligationsPanel";
import { AppStateProvider, useApp, type Theme } from "./state/AppState";
import type { PageData } from "./types/page";

export interface AppProps {
  data: PageData;

  /** Colour mode, when a host owns it. Given one, the app follows it and
   *  offers no toggle of its own; left out, it manages its own, as the
   *  self-contained page `conseqa-viz` writes does. */
  theme?: Theme;
}

export function App({ data, theme }: AppProps) {
  return (
    <TooltipProvider>
      <AppStateProvider data={data} theme={theme}>
        <Shell />
      </AppStateProvider>
    </TooltipProvider>
  );
}

function Shell() {
  const { route, detail, obligationsOpen, report } = useApp();
  const showObligations = obligationsOpen && !!report;

  return (
    <div className="relative flex h-full bg-kumo-canvas text-kumo-default">
      <main className="flex min-w-0 flex-1 flex-col">
        <TopBar />
        <div className="relative min-h-0 flex-1">
          {route.view === "system" && <SystemView />}
          {route.view === "op" && <OperationView id={route.id} />}
          {route.view === "machine" && <MachineView id={route.id} highlight={route.highlight} />}
        </div>
      </main>
      {(detail || showObligations) && (
        <>
        {detail && (
          <aside
            className={`absolute inset-y-0 z-10 w-[380px] shrink-0 border-l border-kumo-hairline bg-kumo-base shadow-xl xl:static xl:shadow-none ${showObligations ? "right-[400px]" : "right-0"}`}
          >
            <DetailPanel />
          </aside>
        )}
        {showObligations && (
          <aside className="absolute inset-y-0 right-0 z-10 w-[400px] shrink-0 border-l border-kumo-hairline bg-kumo-base shadow-xl xl:static xl:shadow-none">
            <ObligationsPanel />
          </aside>
        )}
        </>
      )}
    </div>
  );
}
