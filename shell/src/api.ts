// Thin invoke wrappers — every backend call in one file.
//
// These go straight to the Rust engine in the Tauri webview. There is no
// browser fallback on purpose: the engine is the only source of truth, so the
// UI can never show data the lab did not actually produce.
import { invoke } from "@tauri-apps/api/core";
import type { HeatRow, LabErrorValue, Manifest, ScenarioReport, SinkOutcome, WorkspaceSummary } from "./types";

export const api = {
  workspaces: (): Promise<WorkspaceSummary[]> => invoke("workspaces"),

  manifest: (workspace: string): Promise<Manifest> => invoke("manifest_detail", { workspace }),

  backendInfo: (): Promise<{ backend: string; manifests: string[]; note: string }> => invoke("devices"),

  runCommand: (
    workspace: string,
    commandId: string,
    args: Record<string, unknown>,
    confirm: boolean,
  ): Promise<SinkOutcome> => invoke("run_command", { workspace, commandId, args, confirm }),

  runScenario: (scenarioToml: string, confirm: boolean): Promise<ScenarioReport> =>
    invoke("run_scenario", { scenarioToml, confirm }),

  coverage: (workspace: string): Promise<{ workspace: string; coverage_percent: number; rows: HeatRow[] }> =>
    invoke("coverage", { workspace }),

  discoverLocal: (root: string): Promise<{ name: string; root: string; members: { name: string; version: string; path: string }[] }> =>
    invoke("discover_local", { root }),

  setBackend: (backend: string): Promise<never> => invoke("set_backend", { backend }),
};

export function errorToLines(e: LabErrorValue | string): string[] {
  if (typeof e === "string") return [e];
  const lines = [`${e.kind}: ${e.message}`];
  if (e.remedy?.action) lines.push(`remedy: ${e.remedy.action}`);
  return lines;
}
