import { useState } from "preact/hooks";
import { Panel, Segmented, Toggle } from "../components/ui";
import type { Settings as SettingsData } from "../types";

export type Theme = "system" | "light" | "dark";

function NumberField(props: { label: string; note: string; value: number; min: number; max: number; unit: string; onChange: (v: number) => void }) {
  const { label, note, value, min, max, unit, onChange } = props;
  return (
    <label class="block">
      <span class="block">{label}</span>
      <span class="block text-ink-3 text-xs">{note}</span>
      <span class="flex items-center gap-1.5 mt-1">
        <input
          type="number"
          min={min}
          max={max}
          value={value}
          onChange={(e) => {
            const next = Number((e.target as HTMLInputElement).value);
            if (Number.isFinite(next)) onChange(Math.min(max, Math.max(min, Math.round(next))));
          }}
          class="w-20 rounded-md border border-line bg-surface px-2 py-1 tabular"
        />
        <span class="text-ink-2 text-xs">{unit}</span>
      </span>
    </label>
  );
}

export function SettingsView(props: {
  settings: SettingsData;
  onChange: (next: SettingsData) => void;
  theme: Theme;
  onTheme: (theme: Theme) => void;
}) {
  const { settings: s, onChange, theme, onTheme } = props;
  const [draft, setDraft] = useState("");
  const set = (patch: Partial<SettingsData>) => onChange({ ...s, ...patch });
  const addIgnored = () => {
    const name = draft.trim();
    if (name && !s.ignored.some((i) => i.toLowerCase() === name.toLowerCase())) {
      set({ ignored: [...s.ignored, name] });
    }
    setDraft("");
  };

  return (
    <div class="h-full overflow-y-auto p-4 space-y-3 max-w-[820px]">
      <Panel title="Appearance">
        <div class="px-4 pb-4">
          <Segmented
            label="Theme"
            value={theme}
            onChange={onTheme}
            options={[
              { value: "system", label: "Match Windows" },
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
            ]}
          />
        </div>
      </Panel>

      <Panel title="Sampling">
        <div class="px-4 pb-4 grid grid-cols-2 gap-4">
          <NumberField label="Refresh every" note="Live numbers" value={s.intervalSecs} min={1} max={60} unit="seconds" onChange={(intervalSecs) => set({ intervalSecs })} />
          <NumberField
            label="History detail"
            note="One long term data point"
            value={s.bucketSecs}
            min={s.intervalSecs}
            max={600}
            unit="seconds"
            onChange={(bucketSecs) => set({ bucketSecs })}
          />
        </div>
      </Panel>

      <Panel title="Leak detection">
        <div class="px-4 pb-4 grid grid-cols-2 gap-4">
          <NumberField
            label="Minimum evidence"
            note="Before flagging. Confident at 4×."
            value={s.leakMinMinutes}
            min={1}
            max={1440}
            unit="minutes"
            onChange={(leakMinMinutes) => set({ leakMinMinutes })}
          />
          <NumberField
            label="Startup grace period"
            note="Growth after launch is not counted"
            value={s.leakWarmupMinutes}
            min={0}
            max={240}
            unit="minutes"
            onChange={(leakWarmupMinutes) => set({ leakWarmupMinutes })}
          />
        </div>
        <div class="px-4 pb-4">
          <div>Ignored processes and apps</div>
          <div class="text-ink-3 text-xs">A process name like node.exe, or an app like VS Code</div>
          <div class="flex gap-2 mt-1.5">
            <input
              type="text"
              aria-label="Process or app to ignore"
              value={draft}
              onInput={(e) => setDraft((e.target as HTMLInputElement).value)}
              onKeyDown={(e) => e.key === "Enter" && addIgnored()}
              class="w-64 rounded-md border border-line bg-surface px-2 py-1"
            />
            <button type="button" onClick={addIgnored} class="rounded-md border border-line bg-surface px-2.5 py-1 text-xs font-medium hover:bg-raised">
              Add
            </button>
          </div>
          {s.ignored.length > 0 && (
            <ul class="m-0 p-0 list-none flex flex-wrap gap-1 mt-2">
              {s.ignored.map((name) => (
                <li key={name} class="inline-flex items-center gap-1 rounded bg-raised px-1.5 py-0.5 text-xs">
                  {name}
                  <button
                    type="button"
                    aria-label={`Stop ignoring ${name}`}
                    onClick={() => set({ ignored: s.ignored.filter((i) => i !== name) })}
                    class="bg-transparent border-0 p-0 text-ink-3 hover:text-ink"
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </Panel>

      <Panel title="Alerts and background">
        <div class="px-4 pb-4 space-y-3">
          <Toggle label="Notify me about likely leaks" note="Medium confidence or higher" checked={s.notifyLeaks} onChange={(notifyLeaks) => set({ notifyLeaks })} />
          <Toggle label="Notify me when RAM runs low" checked={s.notifyPressure} onChange={(notifyPressure) => set({ notifyPressure })} />
          <NumberField label="RAM warning level" note="Share of RAM in use" value={s.pressurePct} min={50} max={99} unit="percent" onChange={(pressurePct) => set({ pressurePct })} />
          <Toggle
            label="Keep running in the tray when I close the window"
            note="Leak detection needs long runs. Quit from the tray icon."
            checked={s.closeToTray}
            onChange={(closeToTray) => set({ closeToTray })}
          />
        </div>
      </Panel>
    </div>
  );
}
