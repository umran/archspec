import { statusChipText, statusCounts, worstStatus } from "../lib/obligations";
import { useApp, useObligationsAt } from "../state/AppState";

/** A dashed ring around a node, colored by its worst obligation status. */
export function StatusRing({
  x, y, w, h, rx, obKey,
}: { x: number; y: number; w: number; h: number; rx: number; obKey: string }) {
  const { overlay } = useApp();
  const obs = useObligationsAt(obKey);
  if (!overlay || !obs.length) return null;
  return (
    <rect
      className={`arch-status-ring ${worstStatus(obs)}`}
      x={x - 3}
      y={y - 3}
      width={w + 6}
      height={h + 6}
      rx={rx + 3}
    />
  );
}

/** A small count chip at a node's top-right corner. */
export function StatusChip({ x, y, obKey }: { x: number; y: number; obKey: string }) {
  const { overlay } = useApp();
  const obs = useObligationsAt(obKey);
  if (!overlay || !obs.length) return null;
  const text = statusChipText(statusCounts(obs));
  const w = text.length * 6.4 + 12;
  const color = `var(--arch-${worstStatus(obs)})`;
  return (
    <g className="arch-status-chip">
      <rect x={x - w} y={y - 9} width={w} height={18} rx={9} fill="var(--color-kumo-base)" stroke={color} strokeWidth={1.2} />
      <text x={x - w / 2} y={y + 3.5} textAnchor="middle" fill={color}>
        {text}
      </text>
    </g>
  );
}
