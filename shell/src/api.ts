// Thin invoke wrappers — every backend call in one file.
//
// These go straight to the Rust engine in the Tauri webview. There is no
// browser fallback on purpose: the engine is the only source of truth, so the
// UI can never show data the lab did not actually produce.
import { invoke } from "@tauri-apps/api/core";
import type { CircleCiDashboard, CrateKnowledgeSearch, CrateResearchPrompt, ForgeCatalog, ForgeDashboardData, ForgeLivePreview, ForgeReport, ForgeReportContent, ForgeRun, ForgeSettingsSnapshot, ForgeStatus, HeatRow, LabErrorValue, Manifest, ScenarioReport, SinkOutcome, UrlProbe, WorkspaceSummary } from "./types";

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

  forgeStatus: (): Promise<ForgeStatus> => invoke("forge_status"),
  forgeCatalog: (): Promise<ForgeCatalog> => invoke("forge_catalog"),
  forgeSelectRepository: (repository: string): Promise<ForgeStatus> => invoke("forge_select_repository", { repository }),

  forgeGetSettings: (): Promise<ForgeSettingsSnapshot> => invoke("forge_get_settings"),
  forgeSaveSettings: (updates: Record<string, string>): Promise<ForgeSettingsSnapshot> => invoke("forge_save_settings", { updates }),
  forgeRunTool: (action: string): Promise<{ ok: boolean; action: string; output: string; error: string }> => invoke("forge_run_tool", { action }),
  forgeCircleciDashboard: (): Promise<CircleCiDashboard> => invoke("forge_circleci_dashboard"),
  forgeCrateResearchPrompt: (crateId: string, task: string): Promise<CrateResearchPrompt> => invoke("forge_crate_research_prompt", { crateId, task }),
  forgeSearchCrateKnowledge: (crateId: string, query: string): Promise<CrateKnowledgeSearch> => invoke("forge_search_crate_knowledge", { crateId, query }),

  forgeSelectProvider: (provider: string): Promise<ForgeStatus> =>
    invoke("forge_select_provider", { provider }),

  forgeDryRun: (task: string, crateId: string, caseContext = ""): Promise<ForgeRun> => invoke("forge_dry_run", { task, crateId, caseContext }),

  forgeProbeProvider: (provider: string): Promise<{ provider: string; ok: boolean; status: number; checked_at: number; models: string[]; summary: string }> =>
    invoke("forge_probe_provider", { provider }),

  forgeSelectModel: (provider: string, model: string): Promise<ForgeStatus> =>
    invoke("forge_select_model", { provider, model }),

  forgePreviewLive: (provider: string, model: string, crateId: string, task: string, projectId: string, caseContext = "", learningNotes = ""): Promise<ForgeLivePreview> =>
    invoke("forge_preview_live", { provider, model, crateId, task, projectId, caseContext, learningNotes }),

  forgeGenerateAudit: (provider: string, model: string, crateId: string, task: string, projectId: string, caseContext = "", learningNotes = ""): Promise<{ run: ForgeRun; answer: string; preview: ForgeLivePreview }> =>
    invoke("forge_generate_audit", { provider, model, crateId, task, projectId, caseContext, learningNotes }),

  forgeReports: (): Promise<ForgeReport[]> => invoke("forge_reports"),

  forgeReadReport: (name: string): Promise<ForgeReportContent> => invoke("forge_read_report", { name }),

  forgeOpenReport: (name: string): Promise<{ ok: boolean; path: string }> => invoke("forge_open_report", { name }),

  forgeDashboard: (): Promise<ForgeDashboardData> => invoke("forge_dashboard"),
};

export function errorToLines(e: LabErrorValue | string): string[] {
  if (typeof e === "string") return [e];
  const lines = [`${e.kind}: ${e.message}`];
  if (e.remedy?.action) lines.push(`remedy: ${e.remedy.action}`);
  return lines;
}
