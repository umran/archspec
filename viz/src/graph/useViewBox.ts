import { useCallback, useRef, useState } from "react";
import type { PointerEvent, WheelEvent } from "react";

export interface ViewBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

const MIN_W = 120;
const MAX_W = 50000;

/** Pan/zoom state for an SVG canvas, expressed as its viewBox. */
export function useViewBox(svgRef: React.RefObject<SVGSVGElement | null>) {
  const [viewBox, setViewBox] = useState<ViewBox>({ x: 0, y: 0, w: 1000, h: 700 });
  const drag = useRef<{
    sx: number;
    sy: number;
    vb: ViewBox;
    moved: boolean;
    pointerId: number;
  } | null>(null);
  const suppressClick = useRef(false);

  const clientToWorld = useCallback(
    (cx: number, cy: number, vb: ViewBox) => {
      const rect = svgRef.current?.getBoundingClientRect();
      if (!rect) return { x: vb.x, y: vb.y };
      return {
        x: vb.x + ((cx - rect.left) / rect.width) * vb.w,
        y: vb.y + ((cy - rect.top) / rect.height) * vb.h,
      };
    },
    [svgRef],
  );

  const onWheel = useCallback(
    (e: WheelEvent<SVGSVGElement>) => {
      e.preventDefault();
      setViewBox((vb) => {
        const factor = Math.exp(e.deltaY * 0.0015);
        const next = Math.min(MAX_W, Math.max(MIN_W, vb.w * factor));
        const real = next / vb.w;
        const p = clientToWorld(e.clientX, e.clientY, vb);
        return {
          x: p.x - (p.x - vb.x) * real,
          y: p.y - (p.y - vb.y) * real,
          w: vb.w * real,
          h: vb.h * real,
        };
      });
    },
    [clientToWorld],
  );

  const onPointerDown = useCallback(
    (e: PointerEvent<SVGSVGElement>) => {
      if (e.button !== 0) return;
      drag.current = { sx: e.clientX, sy: e.clientY, vb: viewBox, moved: false, pointerId: e.pointerId };
    },
    [viewBox],
  );

  const onPointerMove = useCallback(
    (e: PointerEvent<SVGSVGElement>) => {
      const d = drag.current;
      if (!d) return;
      if (!d.moved) {
        if (Math.abs(e.clientX - d.sx) + Math.abs(e.clientY - d.sy) <= 5) return;
        d.moved = true;
        svgRef.current?.setPointerCapture(d.pointerId);
      }
      const rect = svgRef.current?.getBoundingClientRect();
      if (!rect) return;
      const dx = ((e.clientX - d.sx) / rect.width) * d.vb.w;
      const dy = ((e.clientY - d.sy) / rect.height) * d.vb.h;
      setViewBox({ ...d.vb, x: d.vb.x - dx, y: d.vb.y - dy });
    },
    [svgRef],
  );

  const onPointerUp = useCallback(() => {
    if (drag.current?.moved) {
      suppressClick.current = true;
      setTimeout(() => {
        suppressClick.current = false;
      }, 0);
    }
    drag.current = null;
  }, []);

  /** Fits the given scene bounds into the canvas with padding. */
  const fitBounds = useCallback(
    (b: { x: number; y: number; width: number; height: number }, pad = 60) => {
      if (b.width === 0 && b.height === 0) return;
      const rect = svgRef.current?.getBoundingClientRect();
      const aspect = rect && rect.width > 1 && rect.height > 1 ? rect.width / rect.height : 16 / 9;
      let w = b.width + pad * 2;
      let h = b.height + pad * 2;
      if (w / h < aspect) w = h * aspect;
      else h = w / aspect;
      setViewBox({ x: b.x + b.width / 2 - w / 2, y: b.y + b.height / 2 - h / 2, w, h });
    },
    [svgRef],
  );

  return {
    viewBox,
    isDragging: () => drag.current?.moved ?? false,
    wasDragged: () => suppressClick.current,
    handlers: { onWheel, onPointerDown, onPointerMove, onPointerUp, onPointerCancel: onPointerUp },
    fitBounds,
  };
}
