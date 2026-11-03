// Types mirrored from the lab crates — the single source of truth for the UI.

export type ArgType =
  | "str" | "int" | "float" | "bool" | "enum"
  | "ipv4" | "mac" | "url" | "path" | "json";

export type Danger = "safe" | "attention" | "destructive";
export type CommandKind = "action" | "query" | "config";
export type EventSource = "poll" | "gena" | "websocket" | "mqtt" | "bus";
export type LimitKind = "budget" | "rate" | "pool" | "concurrency";
export type Confidence = "reported" | "inferred";
export type ConformanceTier = "must" | "should" | "may";
export type Invariant = "idempotent";

export interface ArgSpec {
  name: string;
  type: ArgType;
  required: boolean;
  options?: string[];
  default?: unknown;
  help?: string;
  example?: unknown;
  min?: number;
  max?: number;
  min_len?: number;
  max_len?: number;
}

export interface CommandSpec {
  id: string;
  label: string;
  kind: CommandKind;
  danger: Danger;
  source_enum?: string;
  variant?: string;
  args: ArgSpec[];
  timeout_ms: number;
  rate_limit?: { per_second: number; burst: number };
  emits: string[];
  conformance?: ConformanceTier;
  invariants?: Invariant[];
  mock?: string;
  help?: string;
}

export interface EventSpec {
  id: string;
  source: EventSource;
  payload: string;
  confidence?: Confidence;
  help?: string;
}

export interface ServerSpec {
  id: string;
  crate: string;
  default_port: number;
  health_path?: string;
  routes: string[];
}

export interface LimitSpec {
  id: string;
  kind: LimitKind;
  applies_to: string;
  description?: string;
}

export interface DeviceClass {
  id: string;
  mock_backend: string;
  capability_source: string;
  capabilities: string[];
}

export interface Manifest {
  schema_version: number;
  workspace: {
    name: string; prefix: string; repo: string;
    facade: string; core: string; version: string;
  };
  errors: { source?: string; mapping: { matches: string; to: string; note?: string }[] };
  device_classes: DeviceClass[];
  commands: CommandSpec[];
  events: EventSpec[];
  servers: ServerSpec[];
  limits: LimitSpec[];
  quirks?: { file: string; feature: string };
}

export interface WorkspaceSummary {
  name: string; prefix: string; version: string;
  facade: string; core: string; repo: string;
  commands: number; events: number; servers: number; limits: number;
  device_classes: { id: string; capabilities: string[] }[];
}

export interface LabErrorValue {
  kind: string;
  message: string;
  retryable: boolean;
  remedy: { action: string; after_ms?: number };
  severity: "info" | "warn" | "fatal";
}

export interface SinkOutcome {
  value: unknown;
  events: { id: string; payload: unknown }[];
  duration_ms: number;
  state: Record<string, unknown>;
}

export interface StepResult {
  name: string;
  command?: string;
  status: "pass" | "fail" | "skipped";
  outcome?: string;
  detail?: string;
  duration_ms: number;
  events: { id: string; payload: unknown }[];
}

export interface ScenarioReport {
  name: string;
  ok: boolean;
  steps: StepResult[];
  duration_ms: number;
}

export interface HeatRow {
  workspace: string; command: string; label: string; danger: string;
  buckets_covered: number; runs: number; pass: number; refused: number;
  failed: number; timeout: number; rate_limited: number;
  status: "pass" | "partial" | "failed" | "uncovered";
}

export interface UrlProbe {
  ok: boolean;
  status: number | null;
  latency_ms: number;
  content_type?: string | null;
  error?: string;
}

export interface FleetProvider {
  id: string;
  name: string;
  role: string;
  credential_ready: boolean;
  selected: boolean;
  kind: string;
  note: string;
  state: string;
  probe_supported: boolean;
  probe_destination?: string | null;
  probe_ok?: boolean | null;
  probe_checked_at?: number | null;
  available_models: string[];
  selected_model: string;
}

export interface FleetRun {
  task: string;
  target: string;
  mode: string;
  source_files: number;
  doc_candidates: number;
  report: string;
  completed_at: number;
  provider?: string;
  model?: string;
  source_bytes?: number;
  usage?: Record<string, number>;
}

export interface FleetLivePreview {
  provider: string;
  model: string;
  destination: string;
  target: string;
  files: string[];
  source_bytes: number;
  output_limit_tokens: number;
  retry_policy: string;
  route: { provider: string; model: string; destination: string; requests_left_today: number }[];
}

export interface FleetStatus {
  control_center: string;
  target: string;
  target_path: string;
  target_available: boolean;
  pilot: { workspace: string; crates: { id: string; package: string }[]; batch_size: number; tasks: string[] };
  selected_provider: string;
  providers: FleetProvider[];
  runs: FleetRun[];
  usage: { day?: string; requests?: Record<string, number>; tokens?: Record<string, number> };
  daily_caps: Record<string, number>;
  safe_mode: {
    provider_calls: boolean;
    network: boolean;
    github_writes: boolean;
    target_writes: boolean;
  };
}
