<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { CircleCiDashboard, CrateKnowledgeSearch, CrateResearchPrompt, ForgeProvider, ForgeSettingsSnapshot, ForgeStatus, ScenarioReport } from "../types";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import * as Card from "$lib/components/ui/card";
  import * as Table from "$lib/components/ui/table";
  import * as Select from "$lib/components/ui/select";
  import { Label } from "$lib/components/ui/label";
  import { cn } from "$lib/utils";

  let status = $state<ForgeStatus | null>(null);
  let settings = $state<ForgeSettingsSnapshot | null>(null);
  let circleCi = $state<CircleCiDashboard | null>(null);
  let busy = $state(false);
  let loading = $state(true);
  let error = $state("");
  let result = $state("");
  let scenario = $state<ScenarioReport | null>(null);
  let pendingProbe = $state<ForgeProvider | null>(null);
  let circleCiReview = $state(false);
  let selectedCrate = $state("");
  let internalCrate = $state("lab-engine");
  let promptPack = $state<CrateResearchPrompt | null>(null);
  let crateSearch = $state<CrateKnowledgeSearch | null>(null);
  let crateQuery = $state("What are this crate's contracts, invariants, error behavior, and tests?");
  let confirmKnowledgeAction = $state<"upload" | "deploy" | "health" | "search" | "kaggle-status" | "kaggle-submit" | null>(null);
  const knowledgeToolActions = {
    upload: "upload-crate-intelligence-index",
    deploy: "deploy-crate-intelligence-search",
    health: "check-crate-intelligence-search",
    "kaggle-status": "kaggle-crate-intelligence-status",
    "kaggle-submit": "submit-kaggle-crate-intelligence",
  } as const;
  type QueueItem = { id: string; crate_id: string; task: string; state: "pending" | "done" };
  let stagedQueue = $state<QueueItem[]>([]);

  const scenarioLibrary = [
    {
      id: "tv-morning-loop",
      title: "TV morning loop",
      scope: "Mock Roku device · local only",
      description: "Check power state, wake the virtual TV, press Home, and launch the sample app.",
      toml: `[scenario]\nname = "tv-morning-loop"\ntarget = "mock"\non_fail = "abort"\nvars = { vendor = "roku", ip = "192.168.1.40" }\n\n[[steps]]\nrun = "tv.power.state"\nargs = { ip = "$vars.ip", vendor = "$vars.vendor" }\nexpect = "ok"\n\n[[steps]]\nrun = "tv.power.wake"\nargs = { mac = "AA:BB:CC:DD:EE:FF" }\nexpect = "ok"\nwithin_ms = 3000\n\n[[steps]]\nrun = "tv.power.state"\nargs = { ip = "$vars.ip", vendor = "$vars.vendor" }\nexpect = { state = { key = "power", equals = "On" } }\npoll = { attempts = 5, interval_ms = 100 }\n\n[[steps]]\nrun = "tv.key.press"\nargs = { ip = "$vars.ip", vendor = "$vars.vendor", key = "home" }\nexpect = "ok"\n\n[[steps]]\nrun = "tv.launch.app"\nargs = { ip = "$vars.ip", app_id = "netflix" }\nexpect = "ok"`,
    },
    {
      id: "cctv-zone-watch",
      title: "CCTV zone watch",
      scope: "Mock camera forge · local only",
      description: "Exercise discovery, snapshot, zone detection, PTZ, and capability refusal paths.",
      toml: `[scenario]\nname = "cctv-zone-watch"\ntarget = "mock"\non_fail = "continue"\nvars = { cam = "192.168.2.10" }\n\n[[steps]]\nrun = "cctv.discovery.sweep"\nargs = { timeout_ms = 1500 }\nexpect = "ok"\n\n[[steps]]\nrun = "cctv.stream.snapshot"\nargs = { ip = "$vars.cam" }\nexpect = "ok"\n\n[[steps]]\nrun = "cctv.detect.zones"\nargs = { ip = "$vars.cam" }\nexpect = "ok"\n\n[[steps]]\nrun = "cctv.ptz.move"\nargs = { ip = "$vars.cam", pan = 0.5, tilt = 0.0, zoom = 1.0 }\nexpect = "ok"\n\n[[steps]]\nrun = "cctv.ptz.stop"\nargs = { ip = "$vars.cam" }\nexpect = "ok"\n\n[[steps]]\nrun = "cctv.ptz.goto_preset"\ndevice = "uvc-02"\nargs = { ip = "$vars.cam", preset = 1 }\nexpect = { error_kind = "capability" }`,
    },
  ];
  const internalCrates = ["lab-conformance", "lab-coverage", "lab-engine", "lab-errors", "lab-generate", "lab-manifest", "lab-mocks", "lab-scenarios"];

  onMount(() => {
    try { stagedQueue = JSON.parse(localStorage.getItem("cratelab-forge-queue") ?? "[]"); } catch { stagedQueue = []; }
    void refresh();
  });

  function stageBatch(task: "research-summary" | "rustdoc-audit") {
    if (!status) return;
    const alreadyStaged = new Set(stagedQueue.map((item) => `${item.crate_id}:${item.task}`));
    const start = Math.max(0, status.pilot.crates.findIndex((crate) => crate.id === selectedCrate));
    const ordered = [...status.pilot.crates.slice(start), ...status.pilot.crates.slice(0, start)];
    const next = ordered.filter((crate) => !alreadyStaged.has(`${crate.id}:${task}`)).slice(0, status.pilot.batch_size);
    if (!next.length) { result = `No new ${task} items to stage in the next batch.`; return; }
    const additions = next.map((crate) => ({ id: `${task}:${crate.id}`, crate_id: crate.id, task, state: "pending" as const }));
    stagedQueue = [...stagedQueue, ...additions];
    localStorage.setItem("cratelab-forge-queue", JSON.stringify(stagedQueue));
    result = `Staged ${additions.length} ${task} items locally: ${additions.map((item) => item.crate_id).join(", ")}.`;
  }

  async function refresh() {
    loading = true;
    error = "";
    try {
      const [nextStatus, nextSettings] = await Promise.all([api.forgeStatus(), api.forgeGetSettings()]);
      status = nextStatus;
      settings = nextSettings;
      if (!selectedCrate && nextStatus.pilot.crates.length) selectedCrate = nextStatus.pilot.crates[0].id;
    } catch (e) { error = String(e); }
    finally { loading = false; }
  }

  async function runScenario(toml: string) {
    busy = true; error = ""; result = ""; scenario = null;
    try { scenario = await api.runScenario(toml, false); }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function runLocalAudit(task: "rustdoc-audit" | "research-summary") {
    if (!selectedCrate) return;
    busy = true; error = ""; result = "";
    try {
      const run = await api.forgeDryRun(task, selectedCrate);
      result = `${task} · ${run.target}\n${run.source_files} source files · ${run.doc_candidates} documentation candidates\nReport: ${run.report}`;
      status = await api.forgeStatus();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function localKnowledgeAction(action: "build-signage-index" | "validate-signage-index" | "build-crate-intelligence-index" | "validate-crate-intelligence-index") {
    busy = true; error = ""; result = "";
    try {
      const response = await api.forgeRunTool(action);
      if (!response.ok) throw new Error(response.error || `${action} failed`);
      result = response.output;
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  function setting(key: string) { return settings?.fields.find((field) => field.key === key); }

  async function buildPromptPack() {
    if (!internalCrate) return;
    busy = true; error = ""; crateSearch = null;
    try { promptPack = await api.forgeCrateResearchPrompt(internalCrate, "research-summary"); }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function runConfirmedKnowledgeAction() {
    const action = confirmKnowledgeAction;
    confirmKnowledgeAction = null;
    if (!action) return;
    if (action === "search") {
      if (!internalCrate || !crateQuery.trim()) return;
      busy = true; error = "";
      try { crateSearch = await api.forgeSearchCrateKnowledge(internalCrate, crateQuery); }
      catch (e) { error = String(e); }
      finally { busy = false; }
      return;
    }
    const tool = knowledgeToolActions[action as keyof typeof knowledgeToolActions];
    busy = true; error = ""; result = "";
    try {
      const response = await api.forgeRunTool(tool);
      if (!response.ok) throw new Error(response.error || `${tool} failed`);
      result = response.output;
      settings = await api.forgeGetSettings();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function probeIntegration() {
    if (!pendingProbe) return;
    busy = true; error = ""; result = "";
    try {
      const response = await api.forgeProbeProvider(pendingProbe.id);
      result = `${response.provider}: ${response.summary} (HTTP ${response.status})`;
      pendingProbe = null;
      status = await api.forgeStatus();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function loadCircleCi() {
    busy = true; error = "";
    try { circleCi = await api.forgeCircleciDashboard(); circleCiReview = false; }
    catch (e) { error = String(e); }
    finally { busy = false; }
  }

  let configured = $derived(status?.providers.filter((p) => p.credential_ready).length ?? 0);
  let integrationProviders = $derived((status?.providers ?? []).filter((p) => ["circleci", "github-read", "huggingface", "modal", "kaggle", "qdrant", "gemini", "groq"].includes(p.id)));
  let probed = $derived(status?.providers.filter((p) => p.probe_ok === true).length ?? 0);

  // "Today" reads the engine's ledger. Unknown caps render "—", never a guess.
  function todayUsage(id: string): string {
    const used = status?.usage?.requests?.[id];
    const cap = status?.daily_caps?.[id];
    if (used == null && cap == null) return "—";
    return cap ? `${used ?? 0} / ${cap}` : `${used ?? 0}`;
  }
</script>

<section class="mx-auto max-w-[1500px] pb-7">
  <header class="mb-5 flex flex-col items-start justify-between gap-5 min-[761px]:flex-row min-[761px]:items-end">
    <div><div class="mb-[7px] flex items-center gap-[7px] text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground"><span class="size-[7px] rounded-full bg-success ring-[3px] ring-success/15"></span> CRATELAB OPERATIONS</div><h1 class="flex items-center gap-3 text-[25px] tracking-[-0.4px]">Scenarios & integrations <Badge variant="outline" class="rounded-full border-success/50 bg-success/10 px-[9px] py-1 text-[9px] font-extrabold tracking-[0.8px] text-success">REVIEWABLE ACTIONS</Badge></h1><p class="mt-[7px] text-[13px] text-muted-foreground">Run mock lab scenarios, prepare crate research for the selected repository, and check connected services from one operations board.</p></div>
    <Button variant="outline" onclick={refresh} disabled={loading}>{loading ? "Refreshing…" : "Refresh readiness"}</Button>
  </header>

  {#if error}<div class="mb-3.5 flex items-start justify-between gap-2.5 rounded-[9px] border border-destructive/40 bg-destructive/10 px-[13px] py-2.5 text-[12px] text-destructive" role="alert"><span>{error}</span><button class="cursor-pointer border-0 bg-transparent p-[2px_6px] text-base leading-none" onclick={() => (error = "")} aria-label="Dismiss">×</button></div>{/if}

  <div class="mb-3.5 grid grid-cols-2 gap-[11px] min-[1061px]:grid-cols-4">
    <article class="flex min-h-[104px] flex-col items-start rounded-[10px] border border-primary bg-linear-to-br from-brand to-primary px-[15px] py-[13px] text-primary-foreground"><span class="text-[10.5px] font-[650] text-primary-foreground/80">Target workspace</span><strong class="mt-1 text-[20px] tracking-[-0.35px]">tv-rs</strong><small class="mt-px text-[10.5px] text-primary-foreground/80">{status?.pilot.crates.length ?? "—"} crates · batches of {status?.pilot.batch_size ?? 3}</small></article>
    <article class="flex min-h-[104px] flex-col items-start rounded-[10px] border border-border bg-card px-[15px] py-[13px]"><span class="text-[10.5px] font-[650] text-muted-foreground">Configured integrations</span><strong class="mt-1 text-[20px] tracking-[-0.35px]">{loading ? "—" : `${configured} / ${status?.providers.length ?? 0}`}</strong><small class="mt-px text-[10.5px] text-muted-foreground">Credential presence · values stay masked</small></article>
      <article class="flex min-h-[104px] flex-col items-start rounded-[10px] border border-border bg-card px-[15px] py-[13px]"><span class="text-[10.5px] font-[650] text-muted-foreground">Local task queue</span><strong class="mt-1 text-[20px] tracking-[-0.35px]">{stagedQueue.filter((item) => item.state === "pending").length} pending</strong><small class="mt-px text-[10.5px] text-muted-foreground">Stored locally · no provider request</small></article>
      <article class="flex min-h-[104px] flex-col items-start rounded-[10px] border border-border bg-card px-[15px] py-[13px]"><span class="text-[10.5px] font-[650] text-muted-foreground">Knowledge compute</span><strong class="mt-1 text-[20px] tracking-[-0.35px]">HF · Modal · Kaggle</strong><small class="mt-px text-[10.5px] text-muted-foreground">Private index · authenticated search · CPU-first eval</small></article>
  </div>

  <section class="mb-3.5 rounded-[11px] border border-l-[3px] border-border border-l-primary bg-card p-[15px_17px]">
    <div class="mb-3.5 flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">PINNED SCENARIO · PRODUCT CONTEXT</span><h2 class="mt-0.5 text-[15px] text-foreground">SignageMaster desktop + mobile companion</h2><p class="mt-1 text-[12px] text-muted-foreground">The product context behind retrieval: Simple-Signage is the desktop player, companion-app is the mobile companion. Both stay local; upload and deployment are separate confirmed actions.</p></div><Badge variant="outline" class="shrink-0 rounded-full border-success/50 bg-success/10 px-[9px] py-1 text-[9px] font-extrabold tracking-[0.8px] text-success">LOCAL FIRST</Badge></div>
    <div class="grid grid-cols-1 gap-3 min-[701px]:grid-cols-2 min-[1051px]:grid-cols-3">
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">01 · DESKTOP</span><h3 class="mt-3 mb-[5px] text-[14px]">Simple-Signage docs</h3><p class="min-h-[38px] text-[12px] text-muted-foreground">Player, layout and scheduling documentation. Indexed into the local BM25 corpus.</p><div class="mt-2.5 flex flex-wrap items-center gap-[9px]"><Button variant="outline" size="sm" onclick={() => localKnowledgeAction("build-signage-index")} disabled={busy}>Build desktop index</Button><Button variant="outline" size="sm" onclick={() => localKnowledgeAction("validate-signage-index")} disabled={busy}>Validate</Button></div><small class="mt-2.5 block text-[10.5px] text-muted-foreground">Root: {setting("SIMPLE_SIGNAGE_ROOT")?.configured ? "configured in Providers" : "set SIMPLE_SIGNAGE_ROOT in Providers"}.</small></Card.Root>
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">02 · MOBILE COMPANION</span><h3 class="mt-3 mb-[5px] text-[14px]">companion-app docs</h3><p class="min-h-[38px] text-[12px] text-muted-foreground">Mobile pairing and sync documentation. Same corpus, its own provenance.</p><div class="mt-2.5 flex flex-wrap items-center gap-[9px]"><Button variant="outline" size="sm" onclick={() => localKnowledgeAction("validate-signage-index")} disabled={busy}>Validate provenance</Button></div><small class="mt-2.5 block text-[10.5px] text-muted-foreground">Root: {setting("COMPANION_APP_ROOT")?.configured ? "configured in Providers" : "set COMPANION_APP_ROOT in Providers"}.</small></Card.Root>
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">03 · RETRIEVAL</span><h3 class="mt-3 mb-[5px] text-[14px]">Where the corpus is served</h3><p class="min-h-[38px] text-[12px] text-muted-foreground">{status?.retrieval.enabled ? status.retrieval.destination : "Local BM25 only — no Modal endpoint configured."}</p><small class="mt-2.5 block text-[10.5px] text-muted-foreground">Read-only search · the private dataset stays the source of truth.</small></Card.Root>
    </div>
    {#if result}<pre class="mt-3.5 max-h-[240px] overflow-auto whitespace-pre-wrap rounded-lg bg-foreground p-3 font-mono text-[11px] text-background" role="status">{result}</pre>{/if}
  </section>

  <section class="mb-3.5 rounded-[11px] border border-border bg-card p-[15px_17px]">
    <div class="mb-3.5 flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">RUNGS · ONE ROW EACH</span><h2 class="mt-0.5 text-[15px] text-foreground">Providers &amp; infrastructure</h2><p class="mt-1 text-[12px] text-muted-foreground">Probe is a cheap GET to the service identity or health endpoint — never a spend endpoint. Configured and Probed are different claims.</p></div><Badge variant="secondary" class="inline-grid h-[25px] min-w-[25px] shrink-0 place-items-center rounded-lg px-1 text-[11px] font-[750] text-muted-foreground">{probed}/{status?.providers.length ?? 0} probed</Badge></div>
    <Table.Root class="min-w-[720px] text-[11px]">
      <Table.Header><Table.Row><Table.Head>Provider</Table.Head><Table.Head>State</Table.Head><Table.Head>Today</Table.Head><Table.Head>Role</Table.Head><Table.Head>Action</Table.Head></Table.Row></Table.Header>
      <Table.Body>
        {#each status?.providers ?? [] as p (p.id)}
          <Table.Row>
            <Table.Cell><div class="flex items-center gap-2"><span class={cn("inline-grid size-[25px] place-items-center rounded-[7px] text-[8px] font-[850]", p.credential_ready ? "bg-success/15 text-success" : "bg-muted text-muted-foreground")}>{p.name.slice(0, 2).toUpperCase()}</span><div><strong class="block text-[11.5px]">{p.name}</strong><small class="block text-[9.5px] text-muted-foreground">{p.note}</small></div></div></Table.Cell>
            <Table.Cell><span class={cn("flex items-center gap-1 text-[8px] whitespace-nowrap", p.probe_ok === true ? "text-success" : p.credential_ready && p.probe_ok == null ? "text-warning" : p.probe_ok === false ? "text-destructive" : "text-muted-foreground")}><i class={cn("size-1.5 rounded-full", p.probe_ok === true ? "bg-success" : p.credential_ready && p.probe_ok == null ? "bg-warning" : p.probe_ok === false ? "bg-destructive" : "bg-border")}></i>{p.state}</span></Table.Cell>
            <Table.Cell class="font-mono tabular-nums whitespace-nowrap">{todayUsage(p.id)}</Table.Cell>
            <Table.Cell class="text-[10px] text-muted-foreground">{p.role}</Table.Cell>
            <Table.Cell>
              {#if p.probe_supported && p.credential_ready}
                <Button variant="outline" size="xs" onclick={() => (pendingProbe = p)} disabled={busy}>{p.probe_ok === true ? "Recheck" : "Probe"}</Button>
              {:else if !p.credential_ready && p.setup_url}
                <Button href={p.setup_url} target="_blank" rel="noreferrer" variant="outline" size="xs">How to get ↗</Button>
              {:else}<span class="inline-block min-w-[47px]"></span>{/if}
            </Table.Cell>
          </Table.Row>
        {:else}
          <Table.Row><Table.Cell colspan={5} class="text-[11px] text-muted-foreground">No provider state loaded yet.</Table.Cell></Table.Row>
        {/each}
      </Table.Body>
    </Table.Root>
    <div class="mt-3 rounded-[7px] bg-muted px-[11px] py-[9px] text-[9.5px] text-muted-foreground"><strong class="font-semibold text-foreground">Configured ≠ probed.</strong> Green means a cheap GET passed; amber means the key exists but was never verified. Values are never rendered here.</div>
  </section>

  <section class="mb-3.5 rounded-[11px] border border-border bg-card p-[15px_17px]">
    <div class="mb-3.5 flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">SCENARIO LIBRARY</span><h2 class="mt-0.5 text-[15px] text-foreground">Run a lab scenario</h2><p class="mt-1 text-[12px] text-muted-foreground">These scenarios exercise the Rust lab engine against its in-memory mock forge. They do not control real TVs or cameras.</p></div><Badge variant="outline" class="shrink-0 rounded-full border-success/50 bg-success/10 px-[9px] py-1 text-[9px] font-extrabold tracking-[0.8px] text-success">LOCAL MOCK</Badge></div>
    <div class="grid grid-cols-1 gap-3 min-[701px]:grid-cols-2">
      {#each scenarioLibrary as item (item.id)}
        <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><div class="flex items-center justify-between gap-[9px]"><span class="inline-grid h-[30px] min-w-[34px] place-items-center rounded-lg bg-secondary text-[10px] font-extrabold text-primary">{item.id.startsWith("tv") ? "TV" : "CCTV"}</span><Badge variant="outline" class="shrink-0 rounded-full border-success/50 bg-success/10 px-[9px] py-1 text-[9px] font-extrabold tracking-[0.8px] text-success">{item.scope}</Badge></div><h3 class="mt-3 mb-[5px] text-[14px]">{item.title}</h3><p class="min-h-[38px] text-[12px] text-muted-foreground">{item.description}</p><Button variant="outline" onclick={() => runScenario(item.toml)} disabled={busy}>{busy ? "Running…" : "Run scenario"}</Button></Card.Root>
      {/each}
    </div>
    {#if scenario}
      <div class="mt-3.5 rounded-[9px] border border-success/30 bg-success/10 p-3" role="status"><div class="flex justify-between gap-3"><strong>{scenario.ok ? "Scenario passed" : "Scenario completed with failures"}</strong><span class="text-[11px] text-muted-foreground">{scenario.name} · {scenario.steps.length} steps · {scenario.duration_ms} ms</span></div><div class="mt-2.5 grid grid-cols-1 gap-1.5 min-[701px]:grid-cols-2">{#each scenario.steps as step, i (`${scenario.name}-${i}`)}<article class="flex items-baseline gap-2 rounded-md bg-card px-2 py-1.5 text-[11px]"><b class={step.status === "fail" ? "text-destructive" : "text-success"}>{step.status === "pass" ? "✓" : step.status === "fail" ? "!" : "·"}</b><span>{step.name}</span><small class="ml-auto text-muted-foreground">{step.outcome || step.detail || step.status}</small></article>{/each}</div></div>
    {/if}
  </section>

  <section class="mb-3.5 rounded-[11px] border border-border bg-card p-[15px_17px]">
    <div class="mb-3.5 flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">CRATE EVOLUTION</span><h2 class="mt-0.5 text-[15px] text-foreground">Research and docs actions</h2><p class="mt-1 text-[12px] text-muted-foreground">Start with local reports and a docs-only knowledge index; external provider calls remain separately confirmed.</p></div><Badge variant="outline" class="shrink-0 rounded-full border-success/50 bg-success/10 px-[9px] py-1 text-[9px] font-extrabold tracking-[0.8px] text-success">LOCAL FIRST</Badge></div>
    <div class="grid grid-cols-1 gap-3 min-[701px]:grid-cols-2 min-[1051px]:grid-cols-3">
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">01 · TV-RS</span><h3 class="mt-3 mb-[5px] text-[14px]">Prepare a research or rustdoc audit</h3><Label for="ops-crate" class="mt-2.5 mb-1 block text-[11px] font-normal text-muted-foreground">Crate</Label><Select.Root type="single" value={selectedCrate} onValueChange={(v: string) => (selectedCrate = v)} disabled={busy || loading}><Select.Trigger id="ops-crate" class="w-full"><Select.Value /></Select.Trigger><Select.Content>{#each status?.pilot.crates ?? [] as crate (crate.id)}<Select.Item value={crate.id} label={crate.package}>{crate.package}</Select.Item>{/each}</Select.Content></Select.Root><div class="mt-2.5 flex flex-wrap items-center gap-[9px]"><Button variant="outline" size="sm" onclick={() => runLocalAudit("research-summary")} disabled={busy || !selectedCrate}>Run research inventory</Button><Button variant="outline" size="sm" onclick={() => runLocalAudit("rustdoc-audit")} disabled={busy || !selectedCrate}>Run rustdoc audit</Button></div><div class="mt-2.5 flex flex-wrap items-center gap-[9px]"><Button variant="outline" size="sm" onclick={() => stageBatch("research-summary")} disabled={!status?.pilot.crates.length}>Stage research batch</Button><Button variant="outline" size="sm" onclick={() => stageBatch("rustdoc-audit")} disabled={!status?.pilot.crates.length}>Stage rustdoc batch</Button></div><small class="mt-2.5 block text-[10.5px] text-muted-foreground">Reports read the selected crate and stay local. Staging adds up to {status?.pilot.batch_size ?? 3} items to Forge’s local queue.</small></Card.Root>
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">02 · KNOWLEDGE</span><h3 class="mt-3 mb-[5px] text-[14px]">Signage docs corpus</h3><p class="min-h-[38px] text-[12px] text-muted-foreground">Build or validate the docs-only local index from Simple-Signage and companion-app.</p><div class="mt-2.5 flex flex-wrap items-center gap-[9px]"><Button variant="outline" size="sm" onclick={() => localKnowledgeAction("build-signage-index")} disabled={busy}>Build local index</Button><Button variant="outline" size="sm" onclick={() => localKnowledgeAction("validate-signage-index")} disabled={busy}>Validate index</Button></div><small class="mt-2.5 block text-[10.5px] text-muted-foreground">These actions stay local; upload and deployment are in Forge Settings.</small></Card.Root>
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">03 · CIRCLECI</span><h3 class="mt-3 mb-[5px] text-[14px]">Review research/docs gates</h3><p class="min-h-[38px] text-[12px] text-muted-foreground">{circleCi ? `${circleCi.connected_count} of ${circleCi.configured_count} configured projects are visible.` : "Read account, project, recent pipeline, and workflow status."}</p><Button variant="outline" onclick={() => (circleCiReview = true)} disabled={busy || !integrationProviders.find((p) => p.id === "circleci")?.credential_ready}>{circleCi ? "Refresh CI status" : "Review status request"}</Button><small class="mt-2.5 block text-[10.5px] text-muted-foreground">GET requests only; Forge will not trigger, cancel, or retry pipelines.</small></Card.Root>
    </div>
    {#if result}<pre class="mt-3.5 max-h-[240px] overflow-auto whitespace-pre-wrap rounded-lg bg-foreground p-3 font-mono text-[11px] text-background" role="status">{result}</pre>{/if}
  </section>

  <section class="mb-3.5 rounded-[11px] border border-border bg-card p-[15px_17px]">
    <div class="mb-3.5 flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">INTERNAL CRATE INTELLIGENCE</span><h2 class="mt-0.5 text-[15px] text-foreground">Build evidence, prompts, and retrieval</h2><p class="mt-1 text-[12px] text-muted-foreground">Separate private knowledge path over CrateLab’s own <code>crates/</code> implementation. It is distinct from tv-rs target work and signage documentation.</p></div><Badge variant="outline" class="shrink-0 rounded-full border-success/50 bg-success/10 px-[9px] py-1 text-[9px] font-extrabold tracking-[0.8px] text-success">SOURCE SCOPED</Badge></div>
    <div class="grid grid-cols-1 gap-3 min-[701px]:grid-cols-2">
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">01 · LOCAL SOURCE</span><h3 class="mt-3 mb-[5px] text-[14px]">Crate index and context pack</h3><Label for="internal-crate" class="mt-2.5 mb-1 block text-[11px] font-normal text-muted-foreground">Internal CrateLab crate</Label><Select.Root type="single" value={internalCrate} onValueChange={(v: string) => (internalCrate = v)} disabled={busy}><Select.Trigger id="internal-crate" class="w-full"><Select.Value /></Select.Trigger><Select.Content>{#each internalCrates as name (name)}<Select.Item value={name} label={name}>{name}</Select.Item>{/each}</Select.Content></Select.Root><div class="mt-2.5 flex flex-wrap items-center gap-[9px]"><Button variant="outline" size="sm" onclick={() => localKnowledgeAction("build-crate-intelligence-index")} disabled={busy}>Build index</Button><Button variant="outline" size="sm" onclick={() => localKnowledgeAction("validate-crate-intelligence-index")} disabled={busy}>Validate</Button></div><Button variant="outline" onclick={buildPromptPack} disabled={busy}>Build research prompt</Button><small class="mt-2.5 block text-[10.5px] text-muted-foreground">Reads only <code>crates/*/src/**/*.rs</code> plus package name, description, and internal dependencies. Secret-like source files are skipped.</small></Card.Root>
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">02 · HUGGING FACE</span><h3 class="mt-3 mb-[5px] text-[14px]">Private crate knowledge dataset</h3><p class="min-h-[38px] text-[12px] text-muted-foreground">Target: <code>{setting("HF_CRATES_DATASET_REPO")?.value || "configure in Forge Settings"}</code>. Current local index: {settings?.crate_index_present ? `${Math.round(settings.crate_index_bytes / 1024)} KB` : "not built"}.</p><Button variant="outline" onclick={() => (confirmKnowledgeAction = "upload")} disabled={busy || !settings?.crate_index_present || !setting("HF_CRATES_DATASET_REPO")?.value || !(setting("HF_TOKEN_WRITE")?.configured || setting("HF_TOKEN")?.configured) || !settings?.hf_cli_installed}>Review private HF upload</Button><small class="mt-2.5 block text-[10.5px] text-muted-foreground">Creates/updates a private dataset and pins its revision after confirmation.</small></Card.Root>
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">03 · MODAL SEARCH</span><h3 class="mt-3 mb-[5px] text-[14px]">Authenticated crate retrieval</h3><p class="min-h-[38px] text-[12px] text-muted-foreground">{setting("CRATE_INTEL_MODAL_SEARCH_URL")?.configured ? setting("CRATE_INTEL_MODAL_SEARCH_URL")?.value : "Search app not deployed"} · pinned HF revision: {setting("HF_CRATES_DATASET_REVISION")?.configured ? "configured" : "missing"}.</p><div class="mt-2.5 flex flex-wrap items-center gap-[9px]"><Button variant="outline" size="sm" onclick={() => (confirmKnowledgeAction = "deploy")} disabled={busy || !setting("HF_CRATES_DATASET_REVISION")?.configured || !(setting("HF_READ_TOKEN")?.configured || setting("HF_TOKEN")?.configured) || !settings?.modal_cli_installed || !settings?.modal_profile_configured}>Review deployment</Button><Button variant="outline" size="sm" onclick={() => (confirmKnowledgeAction = "health")} disabled={busy || !setting("CRATE_INTEL_MODAL_SEARCH_URL")?.configured || !setting("CRATE_INTEL_MODAL_SEARCH_TOKEN")?.configured}>Review health check</Button></div><small class="mt-2.5 block text-[10.5px] text-muted-foreground">Search requests are crate-filtered and provenance-checked. Health checks and queries send the saved bearer token.</small></Card.Root>
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">03 · MODAL SEARCH</span><h3 class="mt-3 mb-[5px] text-[14px]">Authenticated crate retrieval</h3><p class="min-h-[38px] text-[12px] text-muted-foreground">{setting("CRATE_INTEL_MODAL_SEARCH_URL")?.configured ? setting("CRATE_INTEL_MODAL_SEARCH_URL")?.value : "Search app not deployed"} · pinned HF revision: {setting("HF_CRATES_DATASET_REVISION")?.configured ? "configured" : "missing"}.</p><Label for="crate-intel-query" class="mt-2.5 mb-1 block text-[11px] font-normal text-muted-foreground">Search {internalCrate} source evidence</Label><textarea id="crate-intel-query" class="w-full resize-y rounded-md border border-input bg-background px-2.5 py-2 font-mono text-[11px] leading-[1.5] text-foreground" bind:value={crateQuery} rows="3" disabled={busy}></textarea><Button variant="outline" onclick={() => (confirmKnowledgeAction = "search")} disabled={busy || !crateQuery.trim() || !setting("CRATE_INTEL_MODAL_SEARCH_URL")?.configured || !setting("CRATE_INTEL_MODAL_SEARCH_TOKEN")?.configured || !setting("HF_CRATES_DATASET_REVISION")?.configured}>Review search request</Button><div class="mt-2.5 flex flex-wrap items-center gap-[9px]"><Button variant="outline" size="sm" onclick={() => (confirmKnowledgeAction = "deploy")} disabled={busy || !setting("HF_CRATES_DATASET_REVISION")?.configured || !(setting("HF_READ_TOKEN")?.configured || setting("HF_TOKEN")?.configured) || !settings?.modal_cli_installed || !settings?.modal_profile_configured}>Review deployment</Button><Button variant="outline" size="sm" onclick={() => (confirmKnowledgeAction = "health")} disabled={busy || !setting("CRATE_INTEL_MODAL_SEARCH_URL")?.configured || !setting("CRATE_INTEL_MODAL_SEARCH_TOKEN")?.configured}>Review health check</Button></div><small class="mt-2.5 block text-[10.5px] text-muted-foreground">Search requests are crate-filtered and provenance-checked. Health checks and queries send the saved bearer token.</small></Card.Root>
      <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><span class="block text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">04 · KAGGLE JOB</span><h3 class="mt-3 mb-[5px] text-[14px]">Dense retrieval evaluation</h3><p class="min-h-[38px] text-[12px] text-muted-foreground">{settings?.kaggle_cli_installed ? "CLI installed · isolated CrateLab environment" : "CLI not detected"} · {setting("KAGGLE_KERNEL_ID")?.value || "kernel id not configured"}.</p><div class="mt-2.5 flex flex-wrap items-center gap-[9px]"><Button variant="outline" size="sm" onclick={() => (confirmKnowledgeAction = "kaggle-status")} disabled={busy || !settings?.kaggle_cli_installed || !setting("KAGGLE_API_KEY")?.configured || !setting("KAGGLE_KERNEL_ID")?.value}>Review job status</Button><Button variant="outline" size="sm" onclick={() => (confirmKnowledgeAction = "kaggle-submit")} disabled={busy || !settings?.kaggle_cli_installed || !setting("KAGGLE_API_KEY")?.configured || !setting("KAGGLE_KERNEL_ID")?.value || !setting("HF_CRATES_DATASET_REVISION")?.configured || !setting("HF_READ_TOKEN")?.configured}>Review CPU job</Button></div><small class="mt-2.5 block text-[10.5px] text-muted-foreground">Compares BM25 to dense retrieval on private benchmark queries. Kaggle Secrets must contain the HF read token, dataset id, and pinned revision.</small></Card.Root>
    </div>
    {#if promptPack}<details class="mt-3.5 rounded-[9px] border border-border bg-card p-[13px]" open><summary class="cursor-pointer text-[12px] font-[750] text-foreground">Context-engineered prompt · {promptPack.crate} · {promptPack.evidence_items} API pointers</summary><textarea class="mt-2.5 block w-full rounded-md border border-input bg-foreground px-2.5 py-2.5 font-mono text-[10px] leading-[1.5] text-background" readonly rows="14" aria-label="Generated crate research prompt">{promptPack.prompt}</textarea><small class="mt-[7px] block text-[10px] text-muted-foreground">Prompt source: {promptPack.prompt_template}. The prompt packet labels provenance, separates fact/inference/open questions, and scopes retrieval to this crate.</small></details>{/if}
    {#if crateSearch}<div class="mt-3.5 rounded-[9px] border border-border bg-card p-[13px]"><div class="flex flex-col gap-1 text-[11px] text-foreground min-[701px]:flex-row min-[701px]:justify-between min-[701px]:gap-3"><strong>{crateSearch.count} source chunks retrieved for {crateSearch.crate}</strong><span class="text-muted-foreground">HF revision {crateSearch.revision}</span></div>{#each crateSearch.hits as hit (hit.source)}<article class="mt-2.5 border-t border-border pt-2.5"><div class="flex flex-col gap-1 text-[11px] text-foreground min-[701px]:flex-row min-[701px]:justify-between min-[701px]:gap-3"><code class="[overflow-wrap:anywhere]">{hit.source}</code><span class="text-muted-foreground">{hit.kind} · score {hit.score.toFixed(3)}</span></div><pre class="mt-[7px] max-h-[260px] overflow-auto whitespace-pre-wrap rounded-md bg-muted p-2.5 font-mono text-[10px] leading-[1.5] text-foreground">{hit.text}</pre></article>{:else}<p class="text-[11px] text-muted-foreground">No matching source chunks. Refine the query or rebuild the index.</p>{/each}</div>{/if}
    {#if result}<pre class="mt-3.5 max-h-[240px] overflow-auto whitespace-pre-wrap rounded-lg bg-foreground p-3 font-mono text-[11px] text-background" role="status">{result}</pre>{/if}
  </section>

  <section class="mb-3.5 rounded-[11px] border border-border bg-card p-[15px_17px]">
    <div class="mb-3.5 flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">COLLABORATION & SERVICE CONNECTIONS</span><h2 class="mt-0.5 text-[15px] text-foreground">Integrations</h2><p class="mt-1 text-[12px] text-muted-foreground">Probe a configured service after reviewing what credential is sent. Missing credentials link to setup.</p></div><Button href="https://app.circleci.com/projects" target="_blank" rel="noreferrer" variant="outline" class="no-underline">CircleCI projects ↗</Button></div>
    <div class="grid grid-cols-1 gap-3 min-[701px]:grid-cols-2 min-[1051px]:grid-cols-3">
      {#each integrationProviders as p (p.id)}
        <Card.Root class="min-w-0 gap-0 rounded-[10px] bg-card px-3.5 py-3.5 ring-border"><div class="flex items-start gap-[9px]"><span class={cn("inline-grid size-[25px] place-items-center rounded-[7px] text-[8px] font-[850]", p.credential_ready ? "bg-success/15 text-success" : "bg-muted text-muted-foreground")}>{p.name.slice(0, 2).toUpperCase()}</span><div class="grid min-w-0 gap-0.5"><strong class="text-[12px]">{p.name}</strong><small class="text-[10px] text-muted-foreground">{p.role}</small></div><span class={cn("ml-auto flex items-center gap-1 whitespace-nowrap text-[8px]", p.probe_ok === true ? "text-success" : p.credential_ready && p.probe_ok == null ? "text-warning" : p.probe_ok === false ? "text-destructive" : "text-muted-foreground")}><i class={cn("size-1.5 rounded-full", p.probe_ok === true ? "bg-success" : p.credential_ready && p.probe_ok == null ? "bg-warning" : p.probe_ok === false ? "bg-destructive" : "bg-border")}></i>{p.state}</span></div><p class="my-2.5 min-h-[44px] text-[12px] text-muted-foreground">{p.note}</p><div class="flex min-h-[28px] flex-wrap items-center justify-between gap-[9px]">{#if p.probe_supported && p.credential_ready}<Button variant="outline" size="xs" onclick={() => (pendingProbe = p)} disabled={busy}>{p.probe_ok === true ? "Review recheck" : "Review connection check"}</Button>{:else if !p.credential_ready && p.setup_url}<Button href={p.setup_url} target="_blank" rel="noreferrer" variant="outline" size="xs">Setup guide ↗</Button>{:else}<span class="inline-block min-w-[47px]"></span>{/if}{#if p.id === "circleci"}<span class="text-[10px] text-muted-foreground">CircleCI organizations appear after status review</span>{/if}</div></Card.Root>
      {/each}
    </div>
  </section>

  {#if circleCi}
    <section class="mb-3.5 rounded-[11px] border border-border bg-card p-[15px_17px]"><div class="mb-3.5 flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">CIRCLECI COLLABORATIONS</span><h2 class="mt-0.5 text-[15px] text-foreground">Visible organizations and projects</h2></div><Badge variant="secondary" class="inline-grid h-[25px] min-w-[25px] shrink-0 place-items-center rounded-lg px-1 text-[11px] font-[750] text-muted-foreground">{circleCi.organizations.length} orgs</Badge></div><div class="grid grid-cols-1 gap-2 min-[701px]:grid-cols-3">{#each circleCi.organizations as org, i (`${org.slug}-${i}`)}<article class="grid gap-1 rounded-lg border border-border p-2.5"><strong>{org.name || org.slug}</strong><code class="text-[10px] text-muted-foreground [overflow-wrap:anywhere]">{org.slug}</code><span class="text-[10px] text-muted-foreground [overflow-wrap:anywhere]">{org.vcs_type || "VCS"}</span></article>{:else}<p class="text-[12px] text-muted-foreground">No collaborations were returned for this account.</p>{/each}</div>
      {#if circleCi.projects.length}<h3 class="mt-4 mb-2 text-[12px] text-muted-foreground">Configured projects</h3><div class="grid grid-cols-1 gap-2 min-[701px]:grid-cols-3">{#each circleCi.projects as project (project.slug)}<article class="grid gap-1 rounded-lg border border-border p-2.5"><strong>{project.name} · {project.connected ? project.state : "not connected / not visible"}</strong><code class="text-[10px] text-muted-foreground [overflow-wrap:anywhere]">{project.slug}</code><span class="text-[10px] text-muted-foreground [overflow-wrap:anywhere]">{project.pipelines[0] ? `Latest pipeline #${project.pipelines[0].number ?? "—"} · ${project.pipelines[0].state ?? "unknown"}` : project.error || "No pipeline history"}</span></article>{/each}</div>{/if}
    </section>
  {/if}

  {#if pendingProbe}
    <div class="fixed inset-0 z-20 grid place-items-center bg-black/50 p-5 backdrop-blur-[2px]" role="presentation"><dialog class="w-[min(520px,100%)] rounded-[13px] border border-border bg-card p-[22px] shadow-2xl" open aria-labelledby="integration-review-title"><span class="text-[9px] font-[850] tracking-[1.2px] text-muted-foreground">READ-ONLY INTEGRATION CHECK</span><h2 id="integration-review-title" class="mt-1.5 mb-[7px] text-[18px] text-foreground">Check {pendingProbe.name} connection?</h2><p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">Forge sends the saved credential to the service’s identity or health endpoint. It does not send crate source or change service settings.</p><code class="block [overflow-wrap:anywhere] rounded-md bg-muted px-2.5 py-[9px] text-[9.5px] text-foreground">{pendingProbe.probe_destination}</code><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Credential</strong><span>{pendingProbe.id === "circleci" ? "CIRCLECI_TOKEN" : pendingProbe.id === "github-read" ? "GH_TOKEN" : pendingProbe.id === "huggingface" ? "HF_READ_TOKEN or HF_TOKEN" : pendingProbe.id.toUpperCase() + " configured credential"}</span></div><div class="mt-[18px] flex justify-end gap-2"><Button variant="outline" size="sm" onclick={() => (pendingProbe = null)} disabled={busy}>Cancel</Button><Button size="sm" onclick={probeIntegration} disabled={busy}>{busy ? "Checking…" : "Send credential and check"}</Button></div></dialog></div>
  {/if}
  {#if circleCiReview}
    <div class="fixed inset-0 z-20 grid place-items-center bg-black/50 p-5 backdrop-blur-[2px]" role="presentation"><dialog class="w-[min(520px,100%)] rounded-[13px] border border-border bg-card p-[22px] shadow-2xl" open aria-labelledby="ops-circleci-title"><span class="text-[9px] font-[850] tracking-[1.2px] text-muted-foreground">READ-ONLY CIRCLECI STATUS</span><h2 id="ops-circleci-title" class="mt-1.5 mb-[7px] text-[18px] text-foreground">Read CircleCI collaborations and gate status?</h2><p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">Forge sends <strong>CIRCLECI_TOKEN</strong> to CircleCI API v2 and reads account, visible organizations, project visibility, recent pipelines, and latest pipeline workflows for the configured slugs.</p><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">No CI control actions</strong><span>No pipeline triggers, cancellations, retries, configuration changes, or source uploads.</span></div><div class="mt-[18px] flex justify-end gap-2"><Button variant="outline" size="sm" onclick={() => (circleCiReview = false)} disabled={busy}>Cancel</Button><Button size="sm" onclick={loadCircleCi} disabled={busy}>{busy ? "Reading…" : "Send token and read status"}</Button></div></dialog></div>
  {/if}
  {#if confirmKnowledgeAction}
    <div class="fixed inset-0 z-20 grid place-items-center bg-black/50 p-5 backdrop-blur-[2px]" role="presentation"><dialog class="w-[min(520px,100%)] rounded-[13px] border border-border bg-card p-[22px] shadow-2xl" open aria-labelledby="crate-knowledge-confirm-title">
      <span class="text-[9px] font-[850] tracking-[1.2px] text-muted-foreground">CRATE KNOWLEDGE OPERATION</span>
      {#if confirmKnowledgeAction === "upload"}
        <h2 id="crate-knowledge-confirm-title" class="mt-1.5 mb-[7px] text-[18px] text-foreground">Upload internal crate index to private HF?</h2><p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">Forge sends the full validated Rust source-chunk index for all CrateLab crates to <strong>{setting("HF_CRATES_DATASET_REPO")?.value}</strong> and creates a new pinned revision.</p><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Data</strong><span>All indexed <code>crates/*/src/**/*.rs</code> chunks plus package metadata, provenance, and revision. Secret-like files were skipped; review remains recommended.</span></div><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Privacy and token</strong><span>Rust creates/validates a private dataset before upload. HF receives the configured HF_TOKEN_WRITE or HF_TOKEN. No tv-rs target files or signage docs are included.</span></div>
      {:else if confirmKnowledgeAction === "deploy"}
        <h2 id="crate-knowledge-confirm-title" class="mt-1.5 mb-[7px] text-[18px] text-foreground">Deploy private crate search to Modal?</h2><p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">Creates/replaces Modal secrets and deploys the crate-filtered search app for <strong>{setting("HF_CRATES_DATASET_REPO")?.value}</strong> at <strong>{setting("HF_CRATES_DATASET_REVISION")?.value}</strong>.</p><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Credentials sent to Modal</strong><span>The HF read token (dedicated HF_READ_TOKEN preferred), dataset identity, revision, and generated search bearer token. If HF_READ_TOKEN is missing, the configured HF_TOKEN will be used.</span></div><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Service and cost</strong><span>Publishes an HTTPS endpoint protected by the bearer token. Modal compute may use included quota or incur charges according to your account and usage.</span></div>
      {:else if confirmKnowledgeAction === "health"}
        <h2 id="crate-knowledge-confirm-title" class="mt-1.5 mb-[7px] text-[18px] text-foreground">Check crate Modal search health?</h2><p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">Sends the saved <strong>CRATE_INTEL_MODAL_SEARCH_TOKEN</strong> to the configured HTTPS endpoint.</p><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Read-only response</strong><span>Returns index chunk count, crate count, and pinned dataset revision. No search query or crate source is sent.</span></div>
      {:else if confirmKnowledgeAction === "search"}
        <h2 id="crate-knowledge-confirm-title" class="mt-1.5 mb-[7px] text-[18px] text-foreground">Search Modal for {internalCrate} evidence?</h2><p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">Sends this query and crate filter to the configured Modal endpoint:</p><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Query</strong><span>{crateQuery}</span></div><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Returned data</strong><span>Up to six source chunks with exact source path, line, crate, and revision are returned to CrateLab. They are not sent to an inference provider by this action.</span></div>
      {:else if confirmKnowledgeAction === "kaggle-status"}
        <h2 id="crate-knowledge-confirm-title" class="mt-1.5 mb-[7px] text-[18px] text-foreground">Read Kaggle kernel status?</h2><p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">Sends the configured Kaggle API token to Kaggle to read the latest status for <strong>{setting("KAGGLE_KERNEL_ID")?.value}</strong>.</p><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Effect</strong><span>Read-only request. No notebook upload, compute start, cancellation, or dataset change.</span></div>
      {:else}
        <h2 id="crate-knowledge-confirm-title" class="mt-1.5 mb-[7px] text-[18px] text-foreground">Submit and run the private Kaggle evaluation?</h2><p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">Uploads the local job source and metadata for <strong>{setting("KAGGLE_KERNEL_ID")?.value}</strong>, then starts a Kaggle kernel run.</p><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Compute and data</strong><span>CPU-only by default, private kernel, internet enabled to download the pinned private HF index and sentence-transformer model. Kaggle receives the job code and source-derived retrieval artifacts; compute uses the quota/billing terms of your Kaggle account.</span></div><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Secrets required in Kaggle</strong><span>Set HF_READ_TOKEN, HF_CRATES_DATASET_REPO, and HF_CRATES_DATASET_REVISION in Kaggle Secrets and grant access to this private kernel. The token is never written into the notebook or sent with the job source.</span></div><div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Outputs</strong><span>Retrieval evaluation JSON, chunk metadata, and compressed embeddings remain Kaggle job outputs. Forge does not download or publish them back to HF.</span></div>
      {/if}
      <div class="mt-[18px] flex justify-end gap-2"><Button variant="outline" size="sm" onclick={() => (confirmKnowledgeAction = null)} disabled={busy}>Cancel</Button><Button size="sm" onclick={runConfirmedKnowledgeAction} disabled={busy}>{busy ? "Working…" : confirmKnowledgeAction === "upload" ? "Upload private index" : confirmKnowledgeAction === "deploy" ? "Create secrets and deploy" : confirmKnowledgeAction === "health" ? "Send token and check health" : confirmKnowledgeAction === "search" ? "Send query and retrieve" : confirmKnowledgeAction === "kaggle-status" ? "Send token and read status" : "Upload job and start CPU run"}</Button></div>
    </dialog></div>
  {/if}
</section>
