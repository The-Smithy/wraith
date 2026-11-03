import { Sparkline } from "../components/chart";
import { Button, Chip, ConfidenceBadge, Empty, Panel } from "../components/ui";
import { duration, METRIC_LABEL, metricValue, shortRole } from "../format";
import type { LeakReport, Pattern, Settings, Snapshot } from "../types";

const PATTERN: Record<Pattern, { label: string; title: string }> = {
  steadyClimb: { label: "Steady climb", title: "Rising at a consistent rate" },
  stairStep: { label: "Climbs in steps", title: "Often follows a repeated action" },
  sawtoothRisingFloor: { label: "Rising floor", title: "Memory gets reclaimed, but less comes back each time" },
  plateau: { label: "Levelled off", title: "Grew, then stopped. Probably a cache that filled up, not a leak" },
};

function Report(props: { report: LeakReport; onSelect: (key: string) => void; onIgnore: (name: string) => void }) {
  const { report: r, onSelect, onIgnore } = props;
  const format = (v: number) => metricValue(r.metric, v);
  const pattern = PATTERN[r.pattern];
  const ignoreName = r.subject === "app" ? r.appLabel : r.name;
  const runsOut = r.hoursToExhaustion !== null && r.hoursToExhaustion < 72 ? duration(r.hoursToExhaustion * 3600) : null;
  return (
    <div class="grid grid-cols-[minmax(0,1fr)_160px_136px_auto] items-center gap-4 px-4 py-2 border-t border-line/50">
      <div class="min-w-0">
        <div class="font-semibold truncate">{shortRole(r)}</div>
        <div class="text-ink-3 text-xs truncate">
          {r.appLabel}
          {r.project ? ` · ${r.project.name}` : ""}
        </div>
        <div class="flex flex-wrap gap-1 mt-1">
          <Chip>{METRIC_LABEL[r.metric]}</Chip>
          <span title={pattern.title}><Chip>{pattern.label}</Chip></span>
          {runsOut && <Chip tone="warn">RAM gone in {runsOut}</Chip>}
        </div>
      </div>
      <Sparkline points={r.series} format={format} />
      <div class="text-right tabular">
        <div class="font-semibold">+{format(r.ratePerHour)}/hour</div>
        <div class="text-ink-2 text-xs">
          {format(r.startValue)} → {format(r.currentValue)}
        </div>
        <ConfidenceBadge confidence={r.confidence} />
      </div>
      <div class="flex gap-1">
        {r.key && <Button onClick={() => onSelect(r.key!)}>Inspect</Button>}
        {r.subject !== "system" && (
          <Button title={`Stop checking ${ignoreName} for leaks`} onClick={() => onIgnore(ignoreName)}>
            Ignore
          </Button>
        )}
      </div>
    </div>
  );
}

function Section(props: { title: string; reports: LeakReport[]; onSelect: (key: string) => void; onIgnore: (name: string) => void }) {
  if (props.reports.length === 0) return null;
  return (
    <Panel title={`${props.title} (${props.reports.length})`}>
      {props.reports.map((r) => (
        <Report key={r.id} report={r} onSelect={props.onSelect} onIgnore={props.onIgnore} />
      ))}
    </Panel>
  );
}

function Status(props: { label: string; value: string }) {
  return (
    <div class="min-w-0">
      <div class="text-ink-3 text-xs">{props.label}</div>
      <div class="font-semibold tabular truncate">{props.value}</div>
    </div>
  );
}

export function Leaks(props: {
  snapshot: Snapshot;
  settings: Settings;
  onSelect: (key: string) => void;
  onIgnore: (name: string) => void;
}) {
  const { snapshot, settings, onSelect, onIgnore } = props;
  const { leaks } = snapshot;
  const suspected = leaks.filter((l) => l.confidence === "high" || l.confidence === "medium" || l.confidence === "low");
  const watching = leaks.filter((l) => l.confidence === "watching");
  const plateaued = leaks.filter((l) => l.confidence === "plateaued");
  const watched = snapshot.t - snapshot.monitorSince;
  const needed = (settings.leakMinMinutes + settings.leakWarmupMinutes) * 60;

  return (
    <div class="h-full overflow-y-auto p-4 space-y-3">
      <Panel>
        <div class="px-4 py-2.5 flex items-center gap-8">
          <Status label="Monitoring for" value={duration(watched)} />
          <Status label="Can flag after" value={duration(needed)} />
          <Status label="Confident after" value={duration(settings.leakMinMinutes * 240)} />
          <Status label="Ignoring" value={String(settings.ignored.length)} />
          <details class="ml-auto text-right relative">
            <summary class="cursor-pointer text-accent text-xs">How detection works</summary>
            <ul class="absolute right-0 top-6 z-20 w-96 text-left m-0 p-3 pl-6 list-disc space-y-1 rounded-md border border-line bg-raised shadow-lg text-ink-2">
              <li>Startup growth is ignored</li>
              <li>Tracks the lowest value in each stretch of time, which is what survives garbage collection</li>
              <li>Flags a process only if that floor keeps rising, by enough to matter</li>
              <li>A rise that stops is shown as levelled off, not as a leak</li>
              <li>Memory, handles, threads, and GDI and USER objects are all checked</li>
            </ul>
          </details>
        </div>
      </Panel>

      {leaks.length === 0 && (
        <Panel>
          <Empty title={watched < needed ? "Still gathering data" : "No leaks detected"}>
            {watched < needed ? `First results in about ${duration(needed - watched)}. Keep Wraith running in the tray.` : "Nothing has a floor that keeps rising."}
          </Empty>
        </Panel>
      )}

      <Section title="Suspected leaks" reports={suspected} onSelect={onSelect} onIgnore={onIgnore} />
      <Section title="Watching" reports={watching} onSelect={onSelect} onIgnore={onIgnore} />
      <Section title="Levelled off" reports={plateaued} onSelect={onSelect} onIgnore={onIgnore} />
    </div>
  );
}
