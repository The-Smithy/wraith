import type { ComponentChildren } from "preact";
import type { Impact, Risk } from "../impact";
import type { Confidence, Severity } from "../types";

export function Panel(props: { title?: string; aside?: ComponentChildren; children: ComponentChildren; class?: string }) {
  return (
    <section class={`rounded-lg bg-surface border border-line ${props.class ?? ""}`}>
      {(props.title || props.aside) && (
        <header class="flex items-center justify-between gap-3 px-4 pt-3 pb-2">
          <h2 class="text-[13px] font-semibold">{props.title}</h2>
          {props.aside}
        </header>
      )}
      {props.children}
    </section>
  );
}

export function Stat(props: { label: string; value: string; note?: string; bar?: number }) {
  const bar = props.bar;
  return (
    <div class="rounded-lg bg-surface border border-line px-4 py-3 min-w-0">
      <div class="text-ink-2 text-xs">{props.label}</div>
      <div class="text-[22px] font-semibold leading-tight mt-0.5 truncate">{props.value}</div>
      {bar !== undefined && (
        <div class="h-1 rounded-full bg-raised mt-2 overflow-hidden">
          <div
            class={`h-full rounded-full ${bar >= 90 ? "bg-critical" : "bg-accent"}`}
            style={{ width: `${Math.min(100, bar)}%` }}
          />
        </div>
      )}
      {props.note && <div class="text-ink-3 text-xs mt-1.5 truncate">{props.note}</div>}
    </div>
  );
}

export function Chip(props: { children: ComponentChildren; tone?: "default" | "accent" | "warn" }) {
  const tone =
    props.tone === "accent"
      ? "bg-accent-wash text-accent"
      : props.tone === "warn"
        ? "bg-critical/12 text-critical-ink"
        : "bg-raised text-ink-2";
  return (
    <span class={`inline-flex items-center rounded px-1.5 py-px text-[11px] whitespace-nowrap ${tone}`}>
      {props.children}
    </span>
  );
}

export function Empty(props: { title: string; children?: ComponentChildren }) {
  return (
    <div class="px-6 py-10 text-center">
      <div class="font-semibold">{props.title}</div>
      {props.children && <div class="text-ink-2 mt-1 max-w-[52ch] mx-auto">{props.children}</div>}
    </div>
  );
}

export function Segmented<T extends string>(props: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  label: string;
}) {
  return (
    <div role="group" aria-label={props.label} class="inline-flex rounded-md bg-raised p-0.5">
      {props.options.map((o) => (
        <button
          key={o.value}
          type="button"
          aria-pressed={o.value === props.value}
          onClick={() => props.onChange(o.value)}
          class={`px-2.5 py-1 rounded text-xs border-0 ${
            o.value === props.value ? "bg-accent-wash text-accent font-semibold" : "bg-transparent text-ink-2 hover:text-ink"
          }`}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Button(props: {
  children: ComponentChildren;
  onClick: () => void;
  tone?: "default" | "danger" | "primary";
  disabled?: boolean;
  title?: string;
}) {
  const tone =
    props.tone === "danger"
      ? "bg-critical text-white border-transparent hover:brightness-110"
      : props.tone === "primary"
        ? "bg-accent text-surface border-transparent hover:brightness-110"
        : "bg-surface text-ink border-line hover:bg-raised";
  return (
    <button
      type="button"
      title={props.title}
      disabled={props.disabled}
      onClick={props.onClick}
      class={`rounded-md border px-2.5 py-1 text-xs font-medium disabled:opacity-40 disabled:cursor-not-allowed ${tone}`}
    >
      {props.children}
    </button>
  );
}

const SEVERITY: Record<Severity, { label: string; icon: string; color: string }> = {
  critical: { label: "Critical", icon: "◆", color: "text-critical" },
  warning: { label: "Warning", icon: "▲", color: "text-warning" },
  info: { label: "Tip", icon: "●", color: "text-ink-3" },
};

export function SeverityBadge(props: { severity: Severity }) {
  const s = SEVERITY[props.severity];
  return (
    <span class="inline-flex items-center gap-1 text-[11px] text-ink-2 whitespace-nowrap">
      <span class={s.color} aria-hidden="true">{s.icon}</span>
      {s.label}
    </span>
  );
}

const CONFIDENCE: Record<Confidence, { label: string; icon: string; color: string }> = {
  high: { label: "High confidence", icon: "◆", color: "text-critical" },
  medium: { label: "Medium confidence", icon: "▲", color: "text-serious" },
  low: { label: "Low confidence", icon: "▲", color: "text-warning" },
  watching: { label: "Watching", icon: "●", color: "text-ink-3" },
  plateaued: { label: "Levelled off", icon: "■", color: "text-good" },
};

export function ConfidenceBadge(props: { confidence: Confidence }) {
  const c = CONFIDENCE[props.confidence];
  return (
    <span class="inline-flex items-center gap-1 text-[11px] text-ink-2 whitespace-nowrap">
      <span class={c.color} aria-hidden="true">{c.icon}</span>
      {c.label}
    </span>
  );
}

const RISK: Record<Risk, { icon: string; color: string }> = {
  restarts: { icon: "↻", color: "text-good" },
  safe: { icon: "●", color: "text-good" },
  caution: { icon: "▲", color: "text-warning" },
  high: { icon: "◆", color: "text-critical" },
  blocked: { icon: "■", color: "text-ink-3" },
};

export function ImpactBadge(props: { impact: Impact }) {
  const r = RISK[props.impact.risk];
  return (
    <span class="inline-flex items-center gap-1.5 min-w-0 text-ink-2">
      <span class={`${r.color} shrink-0 w-3 text-center`} aria-hidden="true">{r.icon}</span>
      <span class="truncate">{props.impact.label}</span>
    </span>
  );
}

/** The full explanation: what stops, then how to recover. */
export function ImpactDetails(props: { impact: Impact }) {
  const { impact } = props;
  return (
    <div>
      <ul class="m-0 p-0 list-none space-y-0.5">
        {impact.effects.map((effect) => (
          <li key={effect} class="flex gap-1.5">
            <span class="text-ink-3" aria-hidden="true">–</span>
            <span>{effect}</span>
          </li>
        ))}
      </ul>
      <div class="mt-1 text-ink-3 text-xs">{impact.recovery}</div>
    </div>
  );
}

export function Toggle(props: { checked: boolean; onChange: (v: boolean) => void; label: string; note?: string }) {
  return (
    <label class="flex items-start gap-2.5 cursor-pointer">
      <input
        type="checkbox"
        checked={props.checked}
        onChange={(e) => props.onChange((e.target as HTMLInputElement).checked)}
        class="mt-0.5 accent-[var(--accent)]"
      />
      <span>
        <span class="block">{props.label}</span>
        {props.note && <span class="block text-ink-3 text-xs">{props.note}</span>}
      </span>
    </label>
  );
}
