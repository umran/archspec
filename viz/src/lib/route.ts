import { useSyncExternalStore } from "react";

export type Route =
  | { view: "system" }
  | { view: "op"; id: string; flow: string | null }
  | { view: "machine"; id: string; highlight: string | null };

export function parseHash(hash: string): Route {
  const h = decodeURIComponent(hash || "");
  let m: RegExpMatchArray | null;
  if ((m = h.match(/^#\/op\/([^?]+)(?:\?flow=(.+))?$/))) {
    return { view: "op", id: m[1], flow: m[2] ?? null };
  }
  if ((m = h.match(/^#\/machine\/([^?]+)(?:\?t=(.+))?$/))) {
    return { view: "machine", id: m[1], highlight: m[2] ?? null };
  }
  return { view: "system" };
}

export function routeKey(route: Route): string {
  if (route.view === "system") return "system";
  if (route.view === "op") return `op:${route.id}` + (route.flow ? `?${route.flow}` : "");
  return `machine:${route.id}` + (route.highlight ? `?${route.highlight}` : "");
}

/** The subject a route names by itself: a machine route's transition.
 *  The page selects it, so deep links and history navigation agree with
 *  what the address bar says. */
export function impliedSubject(route: Route): string | null {
  return route.view === "machine" ? route.highlight : null;
}

export const hashes = {
  system: () => "#/system",
  op: (id: string, flow?: string | null) =>
    `#/op/${encodeURIComponent(id)}` + (flow ? `?flow=${encodeURIComponent(flow)}` : ""),
  machine: (id: string, transition?: string) =>
    `#/machine/${encodeURIComponent(id)}` +
    (transition ? `?t=${encodeURIComponent(transition)}` : ""),
};

const listeners = new Set<() => void>();

function subscribe(listener: () => void) {
  listeners.add(listener);
  window.addEventListener("hashchange", listener);
  return () => {
    listeners.delete(listener);
    window.removeEventListener("hashchange", listener);
  };
}

function snapshot() {
  return window.location.hash;
}

export function useRoute(): Route {
  const hash = useSyncExternalStore(subscribe, snapshot);
  return parseHash(hash);
}

export function navigate(hash: string) {
  if (window.location.hash === hash) {
    for (const listener of listeners) listener();
  } else {
    window.location.hash = hash;
  }
}
