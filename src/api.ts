import { invoke } from "@tauri-apps/api/core";
import type {
  KillOutcome,
  ProcessDetail,
  ProcessHistory,
  Settings,
  Snapshot,
  SystemHistory,
} from "./types";

export const api = {
  snapshot: (since: number) => invoke<Snapshot | null>("get_snapshot", { since }),
  processHistory: (key: string) => invoke<ProcessHistory | null>("get_process_history", { key }),
  appHistory: (label: string) => invoke<ProcessHistory | null>("get_app_history", { label }),
  systemHistory: () => invoke<SystemHistory>("get_system_history"),
  processDetail: (key: string) => invoke<ProcessDetail | null>("get_process_detail", { key }),
  kill: (key: string, tree: boolean) => invoke<KillOutcome>("kill_process", { key, tree }),
  reveal: (path: string) => invoke<void>("reveal_path", { path }),
  settings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
};
