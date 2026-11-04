<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { CircleCiDashboard, CrateKnowledgeSearch, CrateResearchPrompt, FleetProvider, FleetSettingsSnapshot, FleetStatus, ScenarioReport } from "../types";

  let status = $state<FleetStatus | null>(null);
  let settings = $state<FleetSettingsSnapshot | null>(null);
  let circleCi = $state<CircleCiDashboard | null>(null);
  let busy = $state(false);
  let loading = $state(true);
  let error = $state("");
  let result = $state("");
  let scenario = $state<ScenarioReport | null>(null);
  let pendingProbe = $state<FleetProvider | null>(null);
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
      scope: "Mock camera fleet · local only",
      description: "Exercise discovery, snapshot, zone detection, PTZ, and capability refusal paths.",
      toml: `[scenario]\nname = "cctv-zone-watch"\ntarget = "mock"\non_fail = "continue"\nvars = { cam = "192.168.2.10" }\n\n[[steps]]\nrun = "cctv.discovery.sweep"\nargs = { timeout_ms = 1500 }\nexpect = "ok"\n\n[[steps]]\nrun = "cctv.stream.snapshot"\nargs = { ip = "$vars.cam" }\nexpect = "ok"\n\n[[steps]]\nrun = "cctv.detect.zones"\nargs = { ip = "$vars.cam" }\nexpect = "ok"\n\n[[steps]]\nrun = "cctv.ptz.move"\nargs = { ip = "$vars.cam", pan = 0.5, tilt = 0.0, zoom = 1.0 }\nexpect = "ok"\n\n[[steps]]\nrun = "cctv.ptz.stop"\nargs = { ip = "$vars.cam" }\nexpect = "ok"\n\n[[steps]]\nrun = "cctv.ptz.goto_preset"\ndevice = "uvc-02"\nargs = { ip = "$vars.cam", preset = 1 }\nexpect = { error_kind = "capability" }`,
    },
  ];
  const internalCrates = ["lab-conformance", "lab-coverage", "lab-engine", "lab-errors", "lab-generate", "lab-manifest", "lab-mocks", "lab-scenarios"];

  onMount(() => {
    try { stagedQueue = JSON.parse(localStorage.getItem("cratelab-fleet-queue") ?? "[]"); } catch { stagedQueue = []; }
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
    localStorage.setItem("cratelab-fleet-queue", JSON.stringify(stagedQueue));
    result = `Staged ${additions.length} ${task} items locally: ${additions.map((item) => item.crate_id).join(", ")}.`;
  }

  async function refresh() {
    loading = true;
    error = "";
    try {
      const [nextStatus, nextSettings] = await Promise.all([api.fleetStatus(), api.fleetGetSettings()]);
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
      const run = await api.fleetDryRun(task, selectedCrate);
      result = `${task} · ${run.target}\n${run.source_files} source files · ${run.doc_candidates} documentation candidates\nReport: ${run.report}`;
      status = await api.fleetStatus();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function localKnowledgeAction(action: "build-signage-index" | "validate-signage-index" | "build-crate-intelligence-index" | "validate-crate-intelligence-index") {
    busy = true; error = ""; result = "";
    try {
      const response = await api.fleetRunTool(action);
      if (!response.ok) throw new Error(response.error || `${action} failed`);
      result = response.output;
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  function setting(key: string) { return settings?.fields.find((field) => field.key === key); }

  async function buildPromptPack() {
    if (!internalCrate) return;
    busy = true; error = ""; crateSearch = null;
    try { promptPack = await api.fleetCrateResearchPrompt(internalCrate, "research-summary"); }
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
      try { crateSearch = await api.fleetSearchCrateKnowledge(internalCrate, crateQuery); }
      catch (e) { error = String(e); }
      finally { busy = false; }
      return;
    }
    const tool = knowledgeToolActions[action as keyof typeof knowledgeToolActions];
    busy = true; error = ""; result = "";
    try {
      const response = await api.fleetRunTool(tool);
      if (!response.ok) throw new Error(response.error || `${tool} failed`);
      result = response.output;
      settings = await api.fleetGetSettings();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function probeIntegration() {
    if (!pendingProbe) return;
    busy = true; error = ""; result = "";
    try {
      const response = await api.fleetProbeProvider(pendingProbe.id);
      result = `${response.provider}: ${response.summary} (HTTP ${response.status})`;
      pendingProbe = null;
      status = await api.fleetStatus();
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function loadCircleCi() {
    busy = true; error = "";
    try { circleCi = await api.fleetCircleciDashboard(); circleCiReview = false; }
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

<section class="ops-dashboard">
  <header class="fleet-header">
    <div><div class="eyebrow"><span class="eyebrow-dot"></span> CRATELAB OPERATIONS</div><h1>Scenarios & integrations <span class="safe-pill">REVIEWABLE ACTIONS</span></h1><p class="fleet-lede">Run mock lab scenarios, prepare crate research for the selected repository, and check connected services from one operations board.</p></div>
    <button class="ghost" onclick={refresh} disabled={loading}>{loading ? "Refreshing…" : "Refresh readiness"}</button>
  </header>

  {#if error}<div class="fleet-alert" role="alert"><span>{error}</span><button class="alert-close" onclick={() => (error = "")} aria-label="Dismiss">×</button></div>{/if}

  <div class="fleet-metrics ops-metrics">
    <article class="metric-card metric-primary"><span class="metric-label">Target workspace</span><strong>tv-rs</strong><small>{status?.pilot.crates.length ?? "—"} crates · batches of {status?.pilot.batch_size ?? 3}</small></article>
    <article class="metric-card"><span class="metric-label">Configured integrations</span><strong>{loading ? "—" : `${configured} / ${status?.providers.length ?? 0}`}</strong><small>Credential presence · values stay masked</small></article>
      <article class="metric-card"><span class="metric-label">Local task queue</span><strong>{stagedQueue.filter((item) => item.state === "pending").length} pending</strong><small>Stored locally · no provider request</small></article>
      <article class="metric-card"><span class="metric-label">Knowledge compute</span><strong>HF · Modal · Kaggle</strong><small>Private index · authenticated search · CPU-first eval</small></article>
  </div>

  <section class="ops-section panel pinned-scenario">
    <div class="panel-heading"><div><span class="section-kicker">PINNED SCENARIO · PRODUCT CONTEXT</span><h2>SignageMaster desktop + mobile companion</h2><p>The product context behind retrieval: Simple-Signage is the desktop player, companion-app is the mobile companion. Both stay local; upload and deployment are separate confirmed actions.</p></div><span class="local-only">LOCAL FIRST</span></div>
    <div class="ops-actions-grid">
      <article class="ops-action-card"><span class="section-kicker">01 · DESKTOP</span><h3>Simple-Signage docs</h3><p>Player, layout and scheduling documentation. Indexed into the local BM25 corpus.</p><div class="ops-button-row"><button class="ghost" onclick={() => localKnowledgeAction("build-signage-index")} disabled={busy}>Build desktop index</button><button class="ghost" onclick={() => localKnowledgeAction("validate-signage-index")} disabled={busy}>Validate</button></div><small>Root: {setting("SIMPLE_SIGNAGE_ROOT")?.configured ? "configured in Providers" : "set SIMPLE_SIGNAGE_ROOT in Providers"}.</small></article>
      <article class="ops-action-card"><span class="section-kicker">02 · MOBILE COMPANION</span><h3>companion-app docs</h3><p>Mobile pairing and sync documentation. Same corpus, its own provenance.</p><div class="ops-button-row"><button class="ghost" onclick={() => localKnowledgeAction("validate-signage-index")} disabled={busy}>Validate provenance</button></div><small>Root: {setting("COMPANION_APP_ROOT")?.configured ? "configured in Providers" : "set COMPANION_APP_ROOT in Providers"}.</small></article>
      <article class="ops-action-card"><span class="section-kicker">03 · RETRIEVAL</span><h3>Where the corpus is served</h3><p>{status?.retrieval.enabled ? status.retrieval.destination : "Local BM25 only — no Modal endpoint configured."}</p><small>Read-only search · the private dataset stays the source of truth.</small></article>
    </div>
    {#if result}<pre class="ops-output" role="status">{result}</pre>{/if}
  </section>

  <section class="ops-section panel">
    <div class="panel-heading"><div><span class="section-kicker">RUNGS · ONE ROW EACH</span><h2>Providers &amp; infrastructure</h2><p>Probe is a cheap GET to the service identity or health endpoint — never a spend endpoint. Configured and Probed are different claims.</p></div><span class="count-chip">{probed}/{status?.providers.length ?? 0} probed</span></div>
    <div class="prov-table-wrap">
      <table class="prov-table">
        <thead><tr><th>Provider</th><th>State</th><th>Today</th><th>Role</th><th>Action</th></tr></thead>
        <tbody>
          {#each status?.providers ?? [] as p (p.id)}
            <tr>
              <td><div class="prov-cell"><span class="provider-mark" class:mark-ready={p.credential_ready}>{p.name.slice(0, 2).toUpperCase()}</span><div><strong>{p.name}</strong><small>{p.note}</small></div></div></td>
              <td><span class="provider-state" class:state-ready={p.probe_ok === true} class:state-pending={p.credential_ready && p.probe_ok == null} class:state-failed={p.probe_ok === false}><i></i>{p.state}</span></td>
              <td class="prov-today">{todayUsage(p.id)}</td>
              <td class="prov-role">{p.role}</td>
              <td>
                {#if p.probe_supported && p.credential_ready}
                  <button class="probe-button" onclick={() => (pendingProbe = p)} disabled={busy}>{p.probe_ok === true ? "Recheck" : "Probe"}</button>
                {:else if !p.credential_ready && p.setup_url}
                  <a class="probe-button setup-provider-link" href={p.setup_url} target="_blank" rel="noreferrer">How to get ↗</a>
                {:else}<span class="probe-spacer"></span>{/if}
              </td>
            </tr>
          {:else}
            <tr><td colspan="5" class="prov-empty">No provider state loaded yet.</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    <div class="provider-footnote"><strong>Configured ≠ probed.</strong> Green means a cheap GET passed; amber means the key exists but was never verified. Values are never rendered here.</div>
  </section>

  <section class="ops-section panel">
    <div class="panel-heading"><div><span class="section-kicker">SCENARIO LIBRARY</span><h2>Run a lab scenario</h2><p>These scenarios exercise the Rust lab engine against its in-memory mock fleet. They do not control real TVs or cameras.</p></div><span class="local-only">LOCAL MOCK</span></div>
    <div class="ops-scenario-grid">
      {#each scenarioLibrary as item (item.id)}
        <article class="ops-scenario-card"><div class="ops-card-top"><span class="ops-icon">{item.id.startsWith("tv") ? "TV" : "CCTV"}</span><span class="local-only">{item.scope}</span></div><h3>{item.title}</h3><p>{item.description}</p><button class="ghost" onclick={() => runScenario(item.toml)} disabled={busy}>{busy ? "Running…" : "Run scenario"}</button></article>
      {/each}
    </div>
    {#if scenario}
      <div class="ops-result" role="status"><div><strong>{scenario.ok ? "Scenario passed" : "Scenario completed with failures"}</strong><span>{scenario.name} · {scenario.steps.length} steps · {scenario.duration_ms} ms</span></div><div class="ops-step-list">{#each scenario.steps as step, i (`${scenario.name}-${i}`)}<article class:step-failed={step.status === "fail"}><b>{step.status === "pass" ? "✓" : step.status === "fail" ? "!" : "·"}</b><span>{step.name}</span><small>{step.outcome || step.detail || step.status}</small></article>{/each}</div></div>
    {/if}
  </section>

  <section class="ops-section panel">
    <div class="panel-heading"><div><span class="section-kicker">CRATE EVOLUTION</span><h2>Research and docs actions</h2><p>Start with local reports and a docs-only knowledge index; external provider calls remain separately confirmed.</p></div><span class="local-only">LOCAL FIRST</span></div>
    <div class="ops-actions-grid">
      <article class="ops-action-card"><span class="section-kicker">01 · TV-RS</span><h3>Prepare a research or rustdoc audit</h3><label for="ops-crate">Crate</label><select id="ops-crate" bind:value={selectedCrate} disabled={busy || loading}>{#each status?.pilot.crates ?? [] as crate (crate.id)}<option value={crate.id}>{crate.package}</option>{/each}</select><div class="ops-button-row"><button class="ghost" onclick={() => runLocalAudit("research-summary")} disabled={busy || !selectedCrate}>Run research inventory</button><button class="ghost" onclick={() => runLocalAudit("rustdoc-audit")} disabled={busy || !selectedCrate}>Run rustdoc audit</button></div><div class="ops-button-row"><button class="ghost" onclick={() => stageBatch("research-summary")} disabled={!status?.pilot.crates.length}>Stage research batch</button><button class="ghost" onclick={() => stageBatch("rustdoc-audit")} disabled={!status?.pilot.crates.length}>Stage rustdoc batch</button></div><small>Reports read the selected crate and stay local. Staging adds up to {status?.pilot.batch_size ?? 3} items to Fleet’s local queue.</small></article>
      <article class="ops-action-card"><span class="section-kicker">02 · KNOWLEDGE</span><h3>Signage docs corpus</h3><p>Build or validate the docs-only local index from Simple-Signage and companion-app.</p><div class="ops-button-row"><button class="ghost" onclick={() => localKnowledgeAction("build-signage-index")} disabled={busy}>Build local index</button><button class="ghost" onclick={() => localKnowledgeAction("validate-signage-index")} disabled={busy}>Validate index</button></div><small>These actions stay local; upload and deployment are in Fleet Settings.</small></article>
      <article class="ops-action-card"><span class="section-kicker">03 · CIRCLECI</span><h3>Review research/docs gates</h3><p>{circleCi ? `${circleCi.connected_count} of ${circleCi.configured_count} configured projects are visible.` : "Read account, project, recent pipeline, and workflow status."}</p><button class="ghost" onclick={() => (circleCiReview = true)} disabled={busy || !integrationProviders.find((p) => p.id === "circleci")?.credential_ready}>{circleCi ? "Refresh CI status" : "Review status request"}</button><small>GET requests only; Fleet will not trigger, cancel, or retry pipelines.</small></article>
    </div>
    {#if result}<pre class="ops-output" role="status">{result}</pre>{/if}
  </section>

  <section class="ops-section panel crate-intel-section">
    <div class="panel-heading"><div><span class="section-kicker">INTERNAL CRATE INTELLIGENCE</span><h2>Build evidence, prompts, and retrieval</h2><p>Separate private knowledge path over CrateLab’s own <code>crates/</code> implementation. It is distinct from tv-rs target work and signage documentation.</p></div><span class="local-only">SOURCE SCOPED</span></div>
    <div class="ops-actions-grid">
      <article class="ops-action-card"><span class="section-kicker">01 · LOCAL SOURCE</span><h3>Crate index and context pack</h3><label for="internal-crate">Internal CrateLab crate</label><select id="internal-crate" bind:value={internalCrate} disabled={busy}>{#each internalCrates as name (name)}<option value={name}>{name}</option>{/each}</select><div class="ops-button-row"><button class="ghost" onclick={() => localKnowledgeAction("build-crate-intelligence-index")} disabled={busy}>Build index</button><button class="ghost" onclick={() => localKnowledgeAction("validate-crate-intelligence-index")} disabled={busy}>Validate</button></div><button class="ghost" onclick={buildPromptPack} disabled={busy}>Build research prompt</button><small>Reads only <code>crates/*/src/**/*.rs</code> plus package name, description, and internal dependencies. Secret-like source files are skipped.</small></article>
      <article class="ops-action-card"><span class="section-kicker">02 · HUGGING FACE</span><h3>Private crate knowledge dataset</h3><p>Target: <code>{setting("HF_CRATES_DATASET_REPO")?.value || "configure in Fleet Settings"}</code>. Current local index: {settings?.crate_index_present ? `${Math.round(settings.crate_index_bytes / 1024)} KB` : "not built"}.</p><button class="ghost" onclick={() => (confirmKnowledgeAction = "upload")} disabled={busy || !settings?.crate_index_present || !setting("HF_CRATES_DATASET_REPO")?.value || !(setting("HF_TOKEN_WRITE")?.configured || setting("HF_TOKEN")?.configured) || !settings?.hf_cli_installed}>Review private HF upload</button><small>Creates/updates a private dataset and pins its revision after confirmation.</small></article>
      <article class="ops-action-card"><span class="section-kicker">03 · MODAL SEARCH</span><h3>Authenticated crate retrieval</h3><p>{setting("CRATE_INTEL_MODAL_SEARCH_URL")?.configured ? setting("CRATE_INTEL_MODAL_SEARCH_URL")?.value : "Search app not deployed"} · pinned HF revision: {setting("HF_CRATES_DATASET_REVISION")?.configured ? "configured" : "missing"}.</p><div class="ops-button-row"><button class="ghost" onclick={() => (confirmKnowledgeAction = "deploy")} disabled={busy || !setting("HF_CRATES_DATASET_REVISION")?.configured || !(setting("HF_READ_TOKEN")?.configured || setting("HF_TOKEN")?.configured) || !settings?.modal_cli_installed || !settings?.modal_profile_configured}>Review deployment</button><button class="ghost" onclick={() => (confirmKnowledgeAction = "health")} disabled={busy || !setting("CRATE_INTEL_MODAL_SEARCH_URL")?.configured || !setting("CRATE_INTEL_MODAL_SEARCH_TOKEN")?.configured}>Review health check</button></div><small>Search requests are crate-filtered and provenance-checked. Health checks and queries send the saved bearer token.</small></article>
      <article class="ops-action-card"><span class="section-kicker">03 · MODAL SEARCH</span><h3>Authenticated crate retrieval</h3><p>{setting("CRATE_INTEL_MODAL_SEARCH_URL")?.configured ? setting("CRATE_INTEL_MODAL_SEARCH_URL")?.value : "Search app not deployed"} · pinned HF revision: {setting("HF_CRATES_DATASET_REVISION")?.configured ? "configured" : "missing"}.</p><label for="crate-intel-query">Search {internalCrate} source evidence</label><textarea id="crate-intel-query" class="crate-query" bind:value={crateQuery} rows="3" disabled={busy}></textarea><button class="ghost" onclick={() => (confirmKnowledgeAction = "search")} disabled={busy || !crateQuery.trim() || !setting("CRATE_INTEL_MODAL_SEARCH_URL")?.configured || !setting("CRATE_INTEL_MODAL_SEARCH_TOKEN")?.configured || !setting("HF_CRATES_DATASET_REVISION")?.configured}>Review search request</button><div class="ops-button-row"><button class="ghost" onclick={() => (confirmKnowledgeAction = "deploy")} disabled={busy || !setting("HF_CRATES_DATASET_REVISION")?.configured || !(setting("HF_READ_TOKEN")?.configured || setting("HF_TOKEN")?.configured) || !settings?.modal_cli_installed || !settings?.modal_profile_configured}>Review deployment</button><button class="ghost" onclick={() => (confirmKnowledgeAction = "health")} disabled={busy || !setting("CRATE_INTEL_MODAL_SEARCH_URL")?.configured || !setting("CRATE_INTEL_MODAL_SEARCH_TOKEN")?.configured}>Review health check</button></div><small>Search requests are crate-filtered and provenance-checked. Health checks and queries send the saved bearer token.</small></article>
      <article class="ops-action-card"><span class="section-kicker">04 · KAGGLE JOB</span><h3>Dense retrieval evaluation</h3><p>{settings?.kaggle_cli_installed ? "CLI installed · isolated CrateLab environment" : "CLI not detected"} · {setting("KAGGLE_KERNEL_ID")?.value || "kernel id not configured"}.</p><div class="ops-button-row"><button class="ghost" onclick={() => (confirmKnowledgeAction = "kaggle-status")} disabled={busy || !settings?.kaggle_cli_installed || !setting("KAGGLE_API_KEY")?.configured || !setting("KAGGLE_KERNEL_ID")?.value}>Review job status</button><button class="ghost" onclick={() => (confirmKnowledgeAction = "kaggle-submit")} disabled={busy || !settings?.kaggle_cli_installed || !setting("KAGGLE_API_KEY")?.configured || !setting("KAGGLE_KERNEL_ID")?.value || !setting("HF_CRATES_DATASET_REVISION")?.configured || !setting("HF_READ_TOKEN")?.configured}>Review CPU job</button></div><small>Compares BM25 to dense retrieval on private benchmark queries. Kaggle Secrets must contain the HF read token, dataset id, and pinned revision.</small></article>
    </div>
    {#if promptPack}<details class="crate-prompt-preview" open><summary>Context-engineered prompt · {promptPack.crate} · {promptPack.evidence_items} API pointers</summary><textarea readonly rows="14" aria-label="Generated crate research prompt">{promptPack.prompt}</textarea><small>Prompt source: {promptPack.prompt_template}. The prompt packet labels provenance, separates fact/inference/open questions, and scopes retrieval to this crate.</small></details>{/if}
    {#if crateSearch}<div class="crate-retrieval-results"><div><strong>{crateSearch.count} source chunks retrieved for {crateSearch.crate}</strong><span>HF revision {crateSearch.revision}</span></div>{#each crateSearch.hits as hit (hit.source)}<article><div><code>{hit.source}</code><span>{hit.kind} · score {hit.score.toFixed(3)}</span></div><pre>{hit.text}</pre></article>{:else}<p>No matching source chunks. Refine the query or rebuild the index.</p>{/each}</div>{/if}
    {#if result}<pre class="ops-output" role="status">{result}</pre>{/if}
  </section>

  <section class="ops-section panel">
    <div class="panel-heading"><div><span class="section-kicker">COLLABORATION & SERVICE CONNECTIONS</span><h2>Integrations</h2><p>Probe a configured service after reviewing what credential is sent. Missing credentials link to setup.</p></div><a class="ghost settings-link" href="https://app.circleci.com/projects" target="_blank" rel="noreferrer">CircleCI projects ↗</a></div>
    <div class="ops-integrations">
      {#each integrationProviders as p (p.id)}
        <article class="ops-integration-card"><div class="ops-integration-head"><span class="provider-mark" class:mark-ready={p.credential_ready}>{p.name.slice(0, 2).toUpperCase()}</span><div><strong>{p.name}</strong><small>{p.role}</small></div><span class="provider-state" class:state-ready={p.probe_ok === true} class:state-pending={p.credential_ready && p.probe_ok == null} class:state-failed={p.probe_ok === false}><i></i>{p.state}</span></div><p>{p.note}</p><div class="ops-integration-actions">{#if p.probe_supported && p.credential_ready}<button class="probe-button" onclick={() => (pendingProbe = p)} disabled={busy}>{p.probe_ok === true ? "Review recheck" : "Review connection check"}</button>{:else if !p.credential_ready && p.setup_url}<a class="probe-button setup-provider-link" href={p.setup_url} target="_blank" rel="noreferrer">Setup guide ↗</a>{:else}<span class="probe-spacer"></span>{/if}{#if p.id === "circleci"}<span class="ops-meta">CircleCI organizations appear after status review</span>{/if}</div></article>
      {/each}
    </div>
  </section>

  {#if circleCi}
    <section class="ops-section panel"><div class="panel-heading"><div><span class="section-kicker">CIRCLECI COLLABORATIONS</span><h2>Visible organizations and projects</h2></div><span class="count-chip">{circleCi.organizations.length} orgs</span></div><div class="ops-org-list">{#each circleCi.organizations as org, i (`${org.slug}-${i}`)}<article><strong>{org.name || org.slug}</strong><code>{org.slug}</code><span>{org.vcs_type || "VCS"}</span></article>{:else}<p class="ops-empty">No collaborations were returned for this account.</p>{/each}</div>
      {#if circleCi.projects.length}<h3 class="ops-subhead">Configured projects</h3><div class="ops-org-list">{#each circleCi.projects as project (project.slug)}<article><strong>{project.name} · {project.connected ? project.state : "not connected / not visible"}</strong><code>{project.slug}</code><span>{project.pipelines[0] ? `Latest pipeline #${project.pipelines[0].number ?? "—"} · ${project.pipelines[0].state ?? "unknown"}` : project.error || "No pipeline history"}</span></article>{/each}</div>{/if}
    </section>
  {/if}

  {#if pendingProbe}
    <div class="modal-backdrop" role="presentation"><dialog class="confirm-modal" open aria-labelledby="integration-review-title"><span class="modal-kicker">READ-ONLY INTEGRATION CHECK</span><h2 id="integration-review-title">Check {pendingProbe.name} connection?</h2><p>Fleet sends the saved credential to the service’s identity or health endpoint. It does not send crate source or change service settings.</p><code class="modal-destination">{pendingProbe.probe_destination}</code><div class="modal-detail"><strong>Credential</strong><span>{pendingProbe.id === "circleci" ? "CIRCLECI_TOKEN" : pendingProbe.id === "github-read" ? "GH_TOKEN" : pendingProbe.id === "huggingface" ? "HF_READ_TOKEN or HF_TOKEN" : pendingProbe.id.toUpperCase() + " configured credential"}</span></div><div class="modal-actions"><button class="ghost" onclick={() => (pendingProbe = null)} disabled={busy}>Cancel</button><button class="primary" onclick={probeIntegration} disabled={busy}>{busy ? "Checking…" : "Send credential and check"}</button></div></dialog></div>
  {/if}
  {#if circleCiReview}
    <div class="modal-backdrop" role="presentation"><dialog class="confirm-modal" open aria-labelledby="ops-circleci-title"><span class="modal-kicker">READ-ONLY CIRCLECI STATUS</span><h2 id="ops-circleci-title">Read CircleCI collaborations and gate status?</h2><p>Fleet sends <strong>CIRCLECI_TOKEN</strong> to CircleCI API v2 and reads account, visible organizations, project visibility, recent pipelines, and latest pipeline workflows for the configured slugs.</p><div class="modal-detail"><strong>No CI control actions</strong><span>No pipeline triggers, cancellations, retries, configuration changes, or source uploads.</span></div><div class="modal-actions"><button class="ghost" onclick={() => (circleCiReview = false)} disabled={busy}>Cancel</button><button class="primary" onclick={loadCircleCi} disabled={busy}>{busy ? "Reading…" : "Send token and read status"}</button></div></dialog></div>
  {/if}
  {#if confirmKnowledgeAction}
    <div class="modal-backdrop" role="presentation"><dialog class="confirm-modal" open aria-labelledby="crate-knowledge-confirm-title">
      <span class="modal-kicker">CRATE KNOWLEDGE OPERATION</span>
      {#if confirmKnowledgeAction === "upload"}
        <h2 id="crate-knowledge-confirm-title">Upload internal crate index to private HF?</h2><p>Fleet sends the full validated Rust source-chunk index for all CrateLab crates to <strong>{setting("HF_CRATES_DATASET_REPO")?.value}</strong> and creates a new pinned revision.</p><div class="modal-detail"><strong>Data</strong><span>All indexed <code>crates/*/src/**/*.rs</code> chunks plus package metadata, provenance, and revision. Secret-like files were skipped; review remains recommended.</span></div><div class="modal-detail"><strong>Privacy and token</strong><span>Rust creates/validates a private dataset before upload. HF receives the configured HF_TOKEN_WRITE or HF_TOKEN. No tv-rs target files or signage docs are included.</span></div>
      {:else if confirmKnowledgeAction === "deploy"}
        <h2 id="crate-knowledge-confirm-title">Deploy private crate search to Modal?</h2><p>Creates/replaces Modal secrets and deploys the crate-filtered search app for <strong>{setting("HF_CRATES_DATASET_REPO")?.value}</strong> at <strong>{setting("HF_CRATES_DATASET_REVISION")?.value}</strong>.</p><div class="modal-detail"><strong>Credentials sent to Modal</strong><span>The HF read token (dedicated HF_READ_TOKEN preferred), dataset identity, revision, and generated search bearer token. If HF_READ_TOKEN is missing, the configured HF_TOKEN will be used.</span></div><div class="modal-detail"><strong>Service and cost</strong><span>Publishes an HTTPS endpoint protected by the bearer token. Modal compute may use included quota or incur charges according to your account and usage.</span></div>
      {:else if confirmKnowledgeAction === "health"}
        <h2 id="crate-knowledge-confirm-title">Check crate Modal search health?</h2><p>Sends the saved <strong>CRATE_INTEL_MODAL_SEARCH_TOKEN</strong> to the configured HTTPS endpoint.</p><div class="modal-detail"><strong>Read-only response</strong><span>Returns index chunk count, crate count, and pinned dataset revision. No search query or crate source is sent.</span></div>
      {:else if confirmKnowledgeAction === "search"}
        <h2 id="crate-knowledge-confirm-title">Search Modal for {internalCrate} evidence?</h2><p>Sends this query and crate filter to the configured Modal endpoint:</p><div class="modal-detail"><strong>Query</strong><span>{crateQuery}</span></div><div class="modal-detail"><strong>Returned data</strong><span>Up to six source chunks with exact source path, line, crate, and revision are returned to CrateLab. They are not sent to an inference provider by this action.</span></div>
      {:else if confirmKnowledgeAction === "kaggle-status"}
        <h2 id="crate-knowledge-confirm-title">Read Kaggle kernel status?</h2><p>Sends the configured Kaggle API token to Kaggle to read the latest status for <strong>{setting("KAGGLE_KERNEL_ID")?.value}</strong>.</p><div class="modal-detail"><strong>Effect</strong><span>Read-only request. No notebook upload, compute start, cancellation, or dataset change.</span></div>
      {:else}
        <h2 id="crate-knowledge-confirm-title">Submit and run the private Kaggle evaluation?</h2><p>Uploads the local job source and metadata for <strong>{setting("KAGGLE_KERNEL_ID")?.value}</strong>, then starts a Kaggle kernel run.</p><div class="modal-detail"><strong>Compute and data</strong><span>CPU-only by default, private kernel, internet enabled to download the pinned private HF index and sentence-transformer model. Kaggle receives the job code and source-derived retrieval artifacts; compute uses the quota/billing terms of your Kaggle account.</span></div><div class="modal-detail"><strong>Secrets required in Kaggle</strong><span>Set HF_READ_TOKEN, HF_CRATES_DATASET_REPO, and HF_CRATES_DATASET_REVISION in Kaggle Secrets and grant access to this private kernel. The token is never written into the notebook or sent with the job source.</span></div><div class="modal-detail"><strong>Outputs</strong><span>Retrieval evaluation JSON, chunk metadata, and compressed embeddings remain Kaggle job outputs. Fleet does not download or publish them back to HF.</span></div>
      {/if}
      <div class="modal-actions"><button class="ghost" onclick={() => (confirmKnowledgeAction = null)} disabled={busy}>Cancel</button><button class="primary" onclick={runConfirmedKnowledgeAction} disabled={busy}>{busy ? "Working…" : confirmKnowledgeAction === "upload" ? "Upload private index" : confirmKnowledgeAction === "deploy" ? "Create secrets and deploy" : confirmKnowledgeAction === "health" ? "Send token and check health" : confirmKnowledgeAction === "search" ? "Send query and retrieve" : confirmKnowledgeAction === "kaggle-status" ? "Send token and read status" : "Upload job and start CPU run"}</button></div>
    </dialog></div>
  {/if}
</section>
