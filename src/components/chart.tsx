import { useEffect, useMemo, useRef, useState } from "preact/hooks";
import { clock } from "../format";

export interface Series {
  name: string;
  /** A CSS color, always from a token so both themes work. */
  color: string;
  points: [number, number][];
}

/** Rounds an axis maximum up to something a person would pick. */
export function niceMax(value: number): number {
  if (value <= 0) return 1;
  const power = 10 ** Math.floor(Math.log10(value));
  const fraction = value / power;
  const step = fraction <= 1 ? 1 : fraction <= 2 ? 2 : fraction <= 5 ? 5 : 10;
  return step * power;
}

const BYTE_STEPS = [64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536].map((m) => m * 1048576);

function axisMax(value: number, byteScale: boolean): number {
  if (byteScale) return BYTE_STEPS.find((s) => s >= value) ?? niceMax(value);
  return niceMax(value);
}

function useWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setWidth(entry.contentRect.width));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return [ref, width] as const;
}

export function LineChart(props: {
  series: Series[];
  format: (v: number) => string;
  height?: number;
  byteScale?: boolean;
  /** Smallest axis maximum. Whole number counts need 2, so the midline stays whole. */
  minMax?: number;
}) {
  const { series, format, byteScale = false, minMax = 0 } = props;
  const height = props.height ?? 180;
  const [ref, width] = useWidth();
  const [hover, setHover] = useState<number | null>(null);

  const margin = { top: 8, right: 12, bottom: 22, left: 56 };
  const plotW = Math.max(1, width - margin.left - margin.right);
  const plotH = height - margin.top - margin.bottom;

  const longest = series.reduce<[number, number][]>((a, s) => (s.points.length > a.length ? s.points : a), []);
  const { minT, maxT, maxV } = useMemo(() => {
    let minT = Infinity;
    let maxT = -Infinity;
    let maxV = 0;
    for (const s of series) {
      for (const [t, v] of s.points) {
        if (t < minT) minT = t;
        if (t > maxT) maxT = t;
        if (v > maxV) maxV = v;
      }
    }
    return { minT, maxT, maxV: Math.max(minMax, axisMax(maxV, byteScale)) };
  }, [series, byteScale, minMax]);

  if (longest.length < 2) {
    return (
      <div ref={ref} style={{ height }} class="flex items-center justify-center text-ink-3 text-xs">
        Collecting samples
      </div>
    );
  }

  const spanT = Math.max(1, maxT - minT);
  const x = (t: number) => margin.left + ((t - minT) / spanT) * plotW;
  const y = (v: number) => margin.top + plotH - (v / maxV) * plotH;
  const path = (points: [number, number][]) =>
    points.map(([t, v], i) => `${i ? "L" : "M"}${x(t).toFixed(1)} ${y(v).toFixed(1)}`).join("");
  const gridValues = [0, maxV / 2, maxV];

  const onMove = (e: MouseEvent) => {
    const box = (e.currentTarget as SVGElement).getBoundingClientRect();
    const t = minT + ((e.clientX - box.left - margin.left) / plotW) * spanT;
    let best = 0;
    let bestDistance = Infinity;
    longest.forEach(([pt], i) => {
      const d = Math.abs(pt - t);
      if (d < bestDistance) {
        bestDistance = d;
        best = i;
      }
    });
    setHover(best);
  };

  const hoverT = hover !== null ? longest[Math.min(hover, longest.length - 1)]?.[0] : undefined;
  const nearest = (s: Series) =>
    hoverT === undefined
      ? undefined
      : s.points.reduce((a, b) => (Math.abs(b[0] - hoverT) < Math.abs(a[0] - hoverT) ? b : a));
  const tooltipLeft = hoverT !== undefined && x(hoverT) > width - 170;

  return (
    <div ref={ref} class="relative">
      {series.length > 1 && (
        <div class="flex gap-4 px-1 pb-1 text-xs text-ink-2">
          {series.map((s) => (
            <span key={s.name} class="inline-flex items-center gap-1.5">
              <span class="inline-block w-3 h-0.5 rounded" style={{ background: s.color }} />
              {s.name}
            </span>
          ))}
        </div>
      )}
      {width > 0 && (
        <svg
          width={width}
          height={height}
          role="img"
          aria-label={series.map((s) => s.name).join(" and ")}
          onMouseMove={onMove}
          onMouseLeave={() => setHover(null)}
          class="block"
        >
          {gridValues.map((v) => (
            <g key={v}>
              <line
                x1={margin.left}
                x2={width - margin.right}
                y1={y(v)}
                y2={y(v)}
                stroke={v === 0 ? "var(--axis)" : "var(--grid)"}
              />
              <text x={margin.left - 8} y={y(v) + 4} text-anchor="end" font-size="11" fill="var(--ink-3)" class="tabular">
                {format(v)}
              </text>
            </g>
          ))}
          <text x={margin.left} y={height - 6} font-size="11" fill="var(--ink-3)">
            {clock(minT, spanT)}
          </text>
          <text x={width - margin.right} y={height - 6} text-anchor="end" font-size="11" fill="var(--ink-3)">
            {clock(maxT, spanT)}
          </text>
          {series.map((s) => (
            <path
              key={s.name}
              d={path(s.points)}
              fill="none"
              stroke={s.color}
              stroke-width="2"
              stroke-linejoin="round"
              stroke-linecap="round"
            />
          ))}
          {hoverT !== undefined && (
            <g>
              <line x1={x(hoverT)} x2={x(hoverT)} y1={margin.top} y2={margin.top + plotH} stroke="var(--axis)" />
              {series.map((s) => {
                const p = nearest(s);
                return p ? (
                  <circle key={s.name} cx={x(p[0])} cy={y(p[1])} r="4" fill={s.color} stroke="var(--surface)" stroke-width="2" />
                ) : null;
              })}
            </g>
          )}
        </svg>
      )}
      {hoverT !== undefined && (
        <div
          class="absolute top-8 pointer-events-none rounded-md border border-line bg-raised px-2 py-1 text-xs shadow-md whitespace-nowrap z-10"
          style={tooltipLeft ? { right: width - x(hoverT) + 10 } : { left: x(hoverT) + 10 }}
        >
          <div class="text-ink-3">
            {new Date(hoverT * 1000).toLocaleString("en-US", { month: "short", day: "numeric", hour: "numeric", minute: "2-digit", second: "2-digit" })}
          </div>
          {series.map((s) => {
            const p = nearest(s);
            return p ? (
              <div key={s.name} class="flex items-center gap-1.5">
                <span class="inline-block w-2 h-0.5 rounded" style={{ background: s.color }} />
                <span class="text-ink-2">{s.name}</span>
                <span class="font-semibold tabular ml-auto pl-3">{format(p[1])}</span>
              </div>
            ) : null;
          })}
        </div>
      )}
    </div>
  );
}

export function Sparkline(props: {
  points: [number, number][];
  format: (v: number) => string;
  width?: number;
  height?: number;
  color?: string;
}) {
  const { points, format } = props;
  const width = props.width ?? 160;
  const height = props.height ?? 36;
  const color = props.color ?? "var(--series-1)";
  const [hover, setHover] = useState<number | null>(null);
  if (points.length < 2) return <div style={{ width, height }} />;

  const pad = 3;
  const minT = points[0][0];
  const spanT = Math.max(1, points[points.length - 1][0] - minT);
  const values = points.map((p) => p[1]);
  const min = Math.min(...values);
  const range = Math.max(1, Math.max(...values) - min);
  const x = (t: number) => pad + ((t - minT) / spanT) * (width - pad * 2);
  const y = (v: number) => height - pad - ((v - min) / range) * (height - pad * 2);
  const d = points.map(([t, v], i) => `${i ? "L" : "M"}${x(t).toFixed(1)} ${y(v).toFixed(1)}`).join("");
  const active = hover !== null ? points[hover] : null;

  return (
    <div class="relative shrink-0" style={{ width, height }}>
      <svg
        width={width}
        height={height}
        role="img"
        aria-label={`From ${format(values[0])} to ${format(values[values.length - 1])}`}
        onMouseMove={(e) => {
          const box = (e.currentTarget as SVGElement).getBoundingClientRect();
          const ratio = (e.clientX - box.left) / box.width;
          setHover(Math.max(0, Math.min(points.length - 1, Math.round(ratio * (points.length - 1)))));
        }}
        onMouseLeave={() => setHover(null)}
      >
        <path d={d} fill="none" stroke={color} stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
        {active && <circle cx={x(active[0])} cy={y(active[1])} r="4" fill={color} stroke="var(--surface)" stroke-width="2" />}
      </svg>
      {active && (
        <div class="absolute bottom-full right-0 mb-1 pointer-events-none rounded border border-line bg-raised px-1.5 py-0.5 text-[11px] whitespace-nowrap shadow z-10">
          <span class="font-semibold tabular">{format(active[1])}</span>
          <span class="text-ink-3 ml-1.5">{clock(active[0], spanT)}</span>
        </div>
      )}
    </div>
  );
}
