import type { Graph } from "./graph";
import type { Model } from "./model";
import type { ProverReport } from "./report";

// The page data `archspec-viz` injects as `window.ARCHSPEC`, or serves
// as `archspec.json` during development.
export interface PageData {
  title: string;
  model: Model;
  graph: Graph;
  report: ProverReport | null;
}

declare global {
  interface Window {
    ARCHSPEC?: PageData;
  }
}

export async function loadPageData(): Promise<PageData> {
  if (window.ARCHSPEC) return window.ARCHSPEC;

  const response = await fetch("archspec.json");

  if (!response.ok) {
    throw new Error(
      `no page data: ${response.status} ${response.statusText}. ` +
        "Run `npm run data` to generate public/archspec.json, or open a " +
        "file produced by archspec-viz.",
    );
  }

  return (await response.json()) as PageData;
}
