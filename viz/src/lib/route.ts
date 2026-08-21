import { useSyncExternalStore } from "react";

export type Route =
  | { view: "system" }
  | { view: "op"; id: string }
  | { view: "machine"; id: string; highlight: string | null };

export function parseHash(hash: string): Route {
  const h = decodeURIComponent(hash || "");
  let m: RegExpMatchArray | null;
  if ((m = h.match(/^#\/op\/(.+)$/))) return { view: "op", id: m[1] };
  if ((m = h.match(/^#\/machine\/([^?]+)(?:\?t=(.+))?$/))) {
    return { view: "machine", id: m[1], highlight: m[2] ?? null };
  }
  return { view: "system" };
}

export function routeKey(route: Route): string {
  return route.view === "system" ? "system" : `${route.view}:${route.id}`;
}

export const hashes = {
  system: () => "#/system",
  op: (id: string) => `#/op/${encodeURIComponent(id)}`,
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
