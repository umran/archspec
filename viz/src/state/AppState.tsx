import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import { buildIndex, flowContaining, type ModelIndex } from "../lib/index";
import { buildObligationIndex, type ObligationIndex } from "../lib/obligations";
import { hashes, impliedSubject, navigate, routeKey, useRoute, type Route } from "../lib/route";
import type { Graph } from "../types/graph";
import type { Id, Model, RequirementKind } from "../types/model";
import type { PageData } from "../types/page";
import type { Obligation, ProverReport } from "../types/report";

export interface DetailContext {
  req?: { prop: RequirementKind; index: number };
  txStep?: { op: Id; tx: Id; index: number };
  edge?: boolean;
}

export interface DetailTarget {
  id: string;
  ctx: DetailContext;
}

export type Theme = "dark" | "light";

interface AppState {
  data: PageData;
  model: Model;
  graph: Graph;
  report: ProverReport | null;
  index: ModelIndex;
  obligations: ObligationIndex;
  route: Route;

  selection: string | null;
  detail: DetailTarget | null;
  expandedTx: ReadonlySet<string>;
  search: string;
  obligationsOpen: boolean;
  theme: Theme;
  fitRequest: number;

  /** Selects a graph element and, when given, shows its detail. */
  select: (key: string | null, detail?: DetailTarget) => void;
  openDetail: (id: string, ctx?: DetailContext) => void;
  closeDetail: () => void;
  toggleTx: (key: string) => void;
  setSearch: (value: string) => void;
  setObligationsOpen: (value: boolean) => void;
  setTheme: (value: Theme) => void;
  requestFit: () => void;
  /** Navigates to a view, applying a selection once it has rendered. */
  navigateTo: (hash: string, selection?: string) => void;
  focusSubject: (obligation: Obligation) => void;
}

const Context = createContext<AppState | null>(null);

const THEME_KEY = "archspec-viz-theme";

function initialTheme(): Theme {
  const stored = window.localStorage.getItem(THEME_KEY);
  return stored === "light" ? "light" : "dark";
}

export function AppStateProvider({ data, children }: { data: PageData; children: ReactNode }) {
  const route = useRoute();
  const key = routeKey(route);
  const implied = impliedSubject(route);

  const index = useMemo(() => buildIndex(data.model), [data.model]);
  const obligations = useMemo(() => buildObligationIndex(data.report), [data.report]);

  const [selection, setSelection] = useState<string | null>(null);
  const [detail, setDetail] = useState<DetailTarget | null>(null);
  const [expandedTx, setExpandedTx] = useState<ReadonlySet<string>>(() => new Set());
  const [search, setSearch] = useState("");
  const [obligationsOpen, setObligationsOpen] = useState(false);
  const [theme, setTheme] = useState<Theme>(initialTheme);
  const [fitRequest, setFitRequest] = useState(0);

  const pendingSelection = useRef<string | null>(null);

  // A route change resets the selection to whatever is pending from a
  // cross-view focus, else to the subject the route itself names. The
  // detail panel survives so links keep their context, but when the
  // route names a subject an open panel is retargeted to it, so history
  // navigation and deep links show what the address bar says.
  useEffect(() => {
    const pending = pendingSelection.current;
    pendingSelection.current = null;
    setSelection(pending ?? (implied ? `t:${implied}` : null));
    if (pending === null && implied) {
      setDetail((current) => (current ? { id: implied, ctx: {} } : current));
    }
  }, [key, implied]);

  useEffect(() => {
    const root = document.documentElement;
    if (theme === "dark") root.setAttribute("data-mode", "dark");
    else root.removeAttribute("data-mode");
    window.localStorage.setItem(THEME_KEY, theme);
  }, [theme]);

  useEffect(() => {
    document.title = `${data.title} · archspec`;
  }, [data.title]);

  const select = useCallback((next: string | null, target?: DetailTarget) => {
    setSelection(next);
    if (target) setDetail(target);
  }, []);

  const openDetail = useCallback((id: string, ctx: DetailContext = {}) => {
    setDetail({ id, ctx });
  }, []);

  const closeDetail = useCallback(() => {
    setDetail(null);
    setSelection(null);
    // The address bar must not keep naming a selection the page no
    // longer shows.
    if (route.view === "machine" && route.highlight) navigate(hashes.machine(route.id));
  }, [route]);

  const toggleTx = useCallback((txKey: string) => {
    setExpandedTx((current) => {
      const next = new Set(current);
      if (next.has(txKey)) next.delete(txKey);
      else next.add(txKey);
      return next;
    });
  }, []);

  const requestFit = useCallback(() => setFitRequest((n) => n + 1), []);

  const navigateTo = useCallback((hash: string, nextSelection?: string) => {
    if (window.location.hash === hash) {
      // Already there: no route change will apply the selection for us.
      if (nextSelection !== undefined) setSelection(nextSelection);
      return;
    }
    pendingSelection.current = nextSelection ?? null;
    navigate(hash);
  }, []);

  const focusSubject = useCallback(
    (ob: Obligation) => {
      const s = ob.subject;
      switch (s.kind) {
        case "operation": {
          const prop = ob.property.kind === "response_replay" ? "idempotency" : ob.property.kind;
          navigateTo(
            hashes.op(s.operation),
            s.requirement !== undefined ? `req:${prop}:${s.requirement}` : undefined,
          );
          break;
        }
        case "flow":
          navigateTo(hashes.op(s.operation, s.flow), `flow:${s.flow}`);
          break;
        case "transaction": {
          const operation = data.model.operations[s.operation];
          const flow = operation ? flowContaining(operation, s.transaction) : null;
          navigateTo(hashes.op(s.operation, flow), `tx:${s.transaction}`);
          break;
        }
        case "state_machine":
          navigateTo(hashes.machine(s.machine, s.transition));
          break;
        case "topic":
          navigateTo(hashes.system(), s.topic);
          break;
        case "object":
          openDetail(s.object);
          break;
      }
    },
    [navigateTo, openDetail, data.model],
  );

  const value = useMemo<AppState>(
    () => ({
      data,
      model: data.model,
      graph: data.graph,
      report: data.report,
      index,
      obligations,
      route,
      selection,
      detail,
      expandedTx,
      search,
      obligationsOpen,
      theme,
      fitRequest,
      select,
      openDetail,
      closeDetail,
      toggleTx,
      setSearch,
      setObligationsOpen,
      setTheme,
      requestFit,
      navigateTo,
      focusSubject,
    }),
    [
      data, index, obligations, route, selection, detail, expandedTx, search,
      obligationsOpen, theme, fitRequest, select, openDetail, closeDetail, toggleTx,
      requestFit, navigateTo, focusSubject,
    ],
  );

  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function useApp(): AppState {
  const value = useContext(Context);
  if (!value) throw new Error("useApp must be used within AppStateProvider");
  return value;
}

/** Obligations anchored to a graph entity, or none without a report. */
export function useObligationsAt(key: string): Obligation[] {
  const { obligations } = useApp();
  return obligations.get(key) ?? [];
}
