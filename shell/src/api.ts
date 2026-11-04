// Thin invoke wrappers — every backend call in one file.
//
// These go straight to the Rust engine in the Tauri webview. There is no
// browser fallback on purpose: the engine is the only source of truth, so the
// UI can never show data the lab did not actually produce.
import { invoke } from "@tauri-apps/api/core";
import type { CircleCiDashboard, CrateKnowledgeSearch, CrateResearchPrompt, FleetDashboardData, FleetLivePreview, FleetReport, FleetReportContent, FleetRun, FleetSettingsSnapshot, FleetStatus, HeatRow, LabErrorValue, Manifest, ScenarioReport, SinkOutcome, UrlProbe, WorkspaceSummary } from "./types";

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

  probeUrl: (url: string): Promise<UrlProbe> => invoke("probe_url", { url }),

  manifestErrors: (): Promise<string[]> => invoke("manifest_errors"),

  fleetStatus: (): Promise<FleetStatus> => invoke("fleet_status"),
  fleetSelectRepository: (repository: string): Promise<FleetStatus> => invoke("fleet_select_repository", { repository }),

  fleetGetSettings: (): Promise<FleetSettingsSnapshot> => invoke("fleet_get_settings"),
  fleetSaveSettings: (updates: Record<string, string>): Promise<FleetSettingsSnapshot> => invoke("fleet_save_settings", { updates }),
  fleetRunTool: (action: string): Promise<{ ok: boolean; action: string; output: string; error: string }> => invoke("fleet_run_tool", { action }),
  fleetCircleciDashboard: (): Promise<CircleCiDashboard> => invoke("fleet_circleci_dashboard"),
  fleetCrateResearchPrompt: (crateId: string, task: string): Promise<CrateResearchPrompt> => invoke("fleet_crate_research_prompt", { crateId, task }),
  fleetSearchCrateKnowledge: (crateId: string, query: string): Promise<CrateKnowledgeSearch> => invoke("fleet_search_crate_knowledge", { crateId, query }),

  fleetSelectProvider: (provider: string): Promise<FleetStatus> =>
    invoke("fleet_select_provider", { provider }),

  fleetDryRun: (task: string, crateId: string, caseContext = ""): Promise<FleetRun> => invoke("fleet_dry_run", { task, crateId, caseContext }),

  fleetProbeProvider: (provider: string): Promise<{ provider: string; ok: boolean; status: number; checked_at: number; models: string[]; summary: string }> =>
    invoke("fleet_probe_provider", { provider }),

  fleetSelectModel: (provider: string, model: string): Promise<FleetStatus> =>
    invoke("fleet_select_model", { provider, model }),

  fleetPreviewLive: (provider: string, model: string, crateId: string, task: string, projectId: string, caseContext = "", learningNotes = ""): Promise<FleetLivePreview> =>
    invoke("fleet_preview_live", { provider, model, crateId, task, projectId, caseContext, learningNotes }),

  fleetGenerateAudit: (provider: string, model: string, crateId: string, task: string, projectId: string, caseContext = "", learningNotes = ""): Promise<{ run: FleetRun; answer: string; preview: FleetLivePreview }> =>
    invoke("fleet_generate_audit", { provider, model, crateId, task, projectId, caseContext, learningNotes }),

  fleetReports: (): Promise<FleetReport[]> => invoke("fleet_reports"),

  fleetReadReport: (name: string): Promise<FleetReportContent> => invoke("fleet_read_report", { name }),

  fleetOpenReport: (name: string): Promise<{ ok: boolean; path: string }> => invoke("fleet_open_report", { name }),

  fleetDashboard: (): Promise<FleetDashboardData> => invoke("fleet_dashboard"),
};

export function errorToLines(e: LabErrorValue | string): string[] {
  if (typeof e === "string") return [e];
  const lines = [`${e.kind}: ${e.message}`];
  if (e.remedy?.action) lines.push(`remedy: ${e.remedy.action}`);
  return lines;
}
