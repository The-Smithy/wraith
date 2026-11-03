import { ProcessTable } from "../components/process-table";
import { Segmented } from "../components/ui";
import type { Filter, ViewMode } from "../tree";
import type { Snapshot } from "../types";

const FILTERS: { value: Filter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "dev", label: "Dev tooling" },
  { value: "editor", label: "Editors" },
  { value: "browser", label: "Browsers" },
  { value: "app", label: "Apps" },
  { value: "service", label: "Services" },
  { value: "system", label: "System" },
];

export interface ProcessViewState {
  query: string;
  filter: Filter;
  mode: ViewMode;
}

export function Processes(props: {
  snapshot: Snapshot;
  state: ProcessViewState;
  onState: (next: ProcessViewState) => void;
  selected: string | null;
  onSelect: (key: string) => void;
}) {
  const { snapshot, state, onState, selected, onSelect } = props;
  return (
    <div class="h-full flex flex-col p-4 gap-3">
      <div class="flex flex-wrap items-center gap-3">
        <input
          type="search"
          aria-label="Search processes"
          placeholder="Search name, project, port, PID, command"
          value={state.query}
          onInput={(e) => onState({ ...state, query: (e.target as HTMLInputElement).value })}
          class="w-80 rounded-md border border-line bg-surface px-2.5 py-1.5 placeholder:text-ink-3"
        />
        <Segmented label="Filter processes" value={state.filter} options={FILTERS} onChange={(filter) => onState({ ...state, filter })} />
        <div class="ml-auto">
          <Segmented
            label="Table layout"
            value={state.mode}
            options={[
              { value: "apps", label: "Grouped by app" },
              { value: "flat", label: "All processes" },
            ]}
            onChange={(mode) => onState({ ...state, mode })}
          />
        </div>
      </div>
      <ProcessTable
        processes={snapshot.processes}
        now={snapshot.t}
        mode={state.mode}
        filter={state.filter}
        query={state.query}
        selected={selected}
        onSelect={onSelect}
      />
    </div>
  );
}
