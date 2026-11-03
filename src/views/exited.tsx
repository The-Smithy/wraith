import { useMemo, useState } from "preact/hooks";
import { Empty, Panel } from "../components/ui";
import { bytes, CATEGORY_LABEL, duration } from "../format";
import type { Snapshot } from "../types";

export function Exited(props: { snapshot: Snapshot }) {
  const { snapshot } = props;
  const [query, setQuery] = useState("");
  const rows = useMemo(() => {
    const q = query.toLowerCase();
    return snapshot.exited.filter(
      (e) => !q || e.name.toLowerCase().includes(q) || e.role.toLowerCase().includes(q) || e.appLabel.toLowerCase().includes(q) || (e.project?.name.toLowerCase().includes(q) ?? false),
    );
  }, [snapshot.exited, query]);

  return (
    <div class="h-full flex flex-col p-4 gap-3">
      <div class="flex items-center gap-3">
        <input
          type="search"
          aria-label="Search exited processes"
          placeholder="Search exited processes"
          value={query}
          onInput={(e) => setQuery((e.target as HTMLInputElement).value)}
          class="w-80 rounded-md border border-line bg-surface px-2.5 py-1.5 placeholder:text-ink-3"
        />
        <span class="ml-auto text-ink-2 tabular">{rows.length} processes</span>
      </div>
      <Panel class="flex-1 min-h-0 overflow-auto">
        {rows.length === 0 ? (
          <Empty title="Nothing has exited yet" />
        ) : (
          <table class="w-full border-collapse min-w-[860px]">
            <thead class="sticky top-0 bg-surface z-10">
              <tr class="text-xs text-ink-2 text-left">
                <th class="font-normal px-4 py-2">Ended</th>
                <th class="font-normal px-2 py-2">Process</th>
                <th class="font-normal px-2 py-2">Type</th>
                <th class="font-normal px-2 py-2">Tied to</th>
                <th class="font-normal px-2 py-2">Project</th>
                <th class="font-normal px-2 py-2 text-right">Ran for</th>
                <th class="font-normal px-4 py-2 text-right">Peak private</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((e) => (
                <tr key={`${e.key}-${e.ended}`} class="border-t border-line/40">
                  <td class="px-4 py-1 tabular text-ink-2 whitespace-nowrap">{duration(snapshot.t - e.ended)} ago</td>
                  <td class="px-2 py-1 max-w-[300px] truncate">
                    {e.role} <span class="text-ink-3 text-xs">{e.name}</span>
                  </td>
                  <td class="px-2 py-1 text-ink-2 whitespace-nowrap">{CATEGORY_LABEL[e.category]}</td>
                  <td class="px-2 py-1 text-ink-2 max-w-[180px] truncate">{e.appLabel}</td>
                  <td class="px-2 py-1 text-ink-2 max-w-[160px] truncate">{e.project?.name ?? ""}</td>
                  <td class="px-2 py-1 text-right tabular text-ink-2">{e.startKnown === false ? "—" : duration(e.ended - e.started)}</td>
                  <td class="px-4 py-1 text-right tabular">{bytes(e.peakPrivate)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
    </div>
  );
}
