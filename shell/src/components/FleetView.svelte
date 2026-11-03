<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { FleetLivePreview, FleetProvider, FleetRun, FleetStatus } from "../types";

  let status = $state<FleetStatus | null>(null);
  let busy = $state(false);
  let loading = $state(true);
  let error = $state("");
  let lastRun = $state<FleetRun | null>(null);
  let task = $state("rustdoc-audit");
  let crateId = $state("discovery");
  type QueueItem = { id: string; crate_id: string; task: string; state: "pending" | "done" };
  let queue = $state<QueueItem[]>([]);
  let probeTarget = $state<FleetProvider | null>(null);
  let probeBusy = $state(false);
  let probeMessage = $state("");
  let livePreview = $state<FleetLivePreview | null>(null);
  let liveBusy = $state(false);
  let liveAnswer = $state("");
  let liveError = $state("");

  const groups = [
    { label: "Inference", ids: ["gemini", "groq", "allam", "cloudflare", "nvidia", "openrouter", "zhipu"] },
    { label: "Model hosts", ids: ["colab-bonsai", "kaggle-bonsai"] },
    { label: "Knowledge & compute", ids: ["huggingface", "kaggle", "modal", "qdrant"] },
    { label: "Delivery & source", ids: ["circleci", "github-read"] },
  ];

  onMount(() => {
    try { queue = JSON.parse(localStorage.getItem("cratelab-fleet-queue") ?? "[]"); } catch { queue = []; }
    void refresh();
  });

  function saveQueue(next: QueueItem[]) {
    queue = next;
    localStorage.setItem("cratelab-fleet-queue", JSON.stringify(next));
  }

  function stageBatch() {
    if (!status) return;
    const alreadyStaged = new Set(queue.map((item) => `${item.crate_id}:${item.task}`));
    const start = Math.max(0, status.pilot.crates.findIndex((crate) => crate.id === crateId));
    const ordered = [...status.pilot.crates.slice(start), ...status.pilot.crates.slice(0, start)];
    const next = ordered.filter((crate) => !alreadyStaged.has(`${crate.id}:${task}`)).slice(0, status.pilot.batch_size);
    const additions = next.map((crate) => ({ id: `${task}:${crate.id}`, crate_id: crate.id, task, state: "pending" as const }));
    saveQueue([...queue, ...additions]);
  }

  function completeQueueItem(crate: string, taskName: string) {
    const updated = queue.map((item) => item.crate_id === crate && item.task === taskName && item.state === "pending" ? { ...item, state: "done" as const } : item);
    saveQueue(updated);
  }

  function selectQueueItem(item: QueueItem) {
    if (item.state !== "pending") return;
    crateId = item.crate_id;
    task = item.task;
  }

  async function refresh() {
    loading = true;
    error = "";
    try {
      status = await api.fleetStatus();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function selectProvider(id: string) {
    busy = true;
    error = "";
    try {
      status = await api.fleetSelectProvider(id);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function runDryRun() {
    busy = true;
    error = "";
    lastRun = null;
    try {
      lastRun = await api.fleetDryRun(task, crateId);
      completeQueueItem(crateId, task);
      status = await api.fleetStatus();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function openProbe(p: FleetProvider) {
    if (!p.credential_ready || !p.probe_supported) return;
    probeMessage = "";
    probeTarget = p;
  }

  async function confirmProbe() {
    if (!probeTarget) return;
    probeBusy = true;
    probeMessage = "";
    try {
      const result = await api.fleetProbeProvider(probeTarget.id);
      probeMessage = result.summary;
      status = await api.fleetStatus();
      if (result.ok) probeTarget = null;
    } catch (e) {
      probeMessage = String(e);
      status = await api.fleetStatus().catch(() => status);
    } finally {
      probeBusy = false;
    }
  }

  async function selectModel(model: string) {
    const selected = status?.providers.find((p) => p.selected && ["gemini", "groq", "allam", "cloudflare", "nvidia", "openrouter", "zhipu", "modelscope", "colab-bonsai", "kaggle-bonsai"].includes(p.id));
    if (!selected || !model) return;
    busy = true;
    try {
      status = await api.fleetSelectModel(selected.id, model);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function previewLiveRun() {
    const selected = status?.providers.find((p) => p.selected && ["gemini", "groq", "allam", "cloudflare", "nvidia", "openrouter", "zhipu", "modelscope", "colab-bonsai", "kaggle-bonsai"].includes(p.id));
    if (!selected?.selected_model) return;
    liveBusy = true;
    liveError = "";
    liveAnswer = "";
    try {
      livePreview = await api.fleetPreviewLive(selected.id, selected.selected_model, crateId, task);
    } catch (e) {
      liveError = String(e);
    } finally {
      liveBusy = false;
    }
  }

  async function confirmLiveRun() {
    if (!livePreview) return;
    liveBusy = true;
    liveError = "";
    try {
      const result = await api.fleetGenerateAudit(livePreview.provider, livePreview.model, crateId, task);
      liveAnswer = result.answer;
      lastRun = result.run;
      completeQueueItem(crateId, task);
      status = await api.fleetStatus();
      livePreview = null;
    } catch (e) {
      liveError = String(e);
    } finally {
      liveBusy = false;
    }
  }

  function selectedInference(): FleetProvider | undefined {
    return status?.providers.find((p) => p.selected && ["gemini", "groq", "allam", "cloudflare", "nvidia", "openrouter", "zhipu", "modelscope", "colab-bonsai", "kaggle-bonsai"].includes(p.id));
  }

  function provider(id: string): FleetProvider | undefined {
    return status?.providers.find((p) => p.id === id);
  }

  function time(timestamp: number): string {
    return new Date(timestamp * 1000).toLocaleString();
  }

  let configuredCount = $derived(status?.providers.filter((p) => p.credential_ready).length ?? 0);
  let providerCount = $derived(status?.providers.length ?? 0);
</script>

<section class="fleet-page" aria-labelledby="fleet-title">
  <header class="fleet-header">
    <div>
      <div class="eyebrow"><span class="eyebrow-dot"></span> LOCAL CONTROL PLANE</div>
      <h1 id="fleet-title">Inference fleet <span class="safe-pill">SAFE MODE</span></h1>
      <p class="fleet-lede">Route crate work across free-tier providers, inspect readiness and usage, and keep every action reviewable from cratelab.</p>
    </div>
    <button class="ghost refresh-button" onclick={refresh} disabled={loading} aria-label="Refresh local provider readiness">
      <span class:spin={loading}>↻</span> Refresh status
    </button>
  </header>

  {#if error}
    <div class="fleet-alert" role="alert"><strong>Could not load fleet state</strong><span>{error}</span></div>
  {/if}

  <div class="fleet-metrics">
    <article class="metric-card metric-primary">
      <span class="metric-label">Control center</span><strong>cratelab</strong><small>Local orchestration and run history</small>
    </article>
    <article class="metric-card">
      <span class="metric-label">Credentialed services</span><strong>{loading ? "—" : `${configuredCount} / ${providerCount}`}</strong><small>Presence only · values stay hidden</small>
    </article>
    <article class="metric-card">
      <span class="metric-label">Provider actions</span><strong class="metric-off">Confirm first</strong><small>Confirmed runs use bounded fallback routing</small>
    </article>
    <article class="metric-card">
      <span class="metric-label">Target scope</span><strong>{status?.pilot.crates.length ?? "—"} crates</strong><small>tv-rs workspace · batches of {status?.pilot.batch_size ?? 3}</small>
    </article>
  </div>

  <section class="architecture-card" aria-label="Fleet architecture">
    <div class="section-heading">
      <div><span class="section-kicker">HOW A RUN FLOWS</span><h2>One control plane, staged execution</h2></div>
      <span class="flow-caption">Secrets stay local · source context goes only to the chosen/eligible provider</span>
    </div>
    <div class="flow-track">
      <div class="flow-node"><span class="flow-index">01</span><strong>cratelab</strong><small>queue · policy · ledger</small></div>
      <span class="flow-arrow">→</span>
      <div class="flow-node"><span class="flow-index">02</span><strong>Task gate</strong><small>scope · class · limits</small></div>
      <span class="flow-arrow">→</span>
      <div class="flow-node flow-node-muted"><span class="flow-index">03</span><strong>Provider ladder</strong><small>confirmed · capped · logged</small></div>
      <span class="flow-arrow">→</span>
      <div class="flow-node flow-node-muted"><span class="flow-index">04</span><strong>Knowledge layer</strong><small>HF · embeddings · search</small></div>
      <span class="flow-arrow">→</span>
      <div class="flow-node"><span class="flow-index">05</span><strong>Human review</strong><small>local report · you decide</small></div>
    </div>
    <div class="guardrail-strip"><span>✓</span> Read-only target scan <span>✓</span> Confirmation before dispatch <span>✓</span> Daily local caps <span>✓</span> No target or GitHub writes</div>
  </section>

  <section class="policy-panel panel">
    <div class="panel-heading"><div><span class="section-kicker">DISPATCH POLICY</span><h2>Task classes decide the route</h2></div><span class="flow-caption">Class B audit is enabled; fallback uses configured, probed providers</span></div>
    <div class="class-grid">
      <article class="class-card class-active"><span class="class-letter">B</span><div><strong>Documentation</strong><small>Docs-only patch proposal · current test</small></div><span class="class-route">Gemini / Bonsai → Groq</span></article>
      <article class="class-card"><span class="class-letter">A</span><div><strong>Cleanup</strong><small>Formatting and lint-only changes</small></div><span class="class-route">Bonsai → Gemini → Groq</span></article>
      <article class="class-card"><span class="class-letter">C</span><div><strong>Research</strong><small>Analysis and ecosystem comparisons</small></div><span class="class-route">Gemini → NIM → Compound</span></article>
      <article class="class-card"><span class="class-letter">D</span><div><strong>Knowledge</strong><small>Digests and low-risk summaries</small></div><span class="class-route">Flash Lite → Groq → Zhipu</span></article>
      <article class="class-card"><span class="class-letter">E</span><div><strong>Release prep</strong><small>Changelog and compatibility review</small></div><span class="class-route">Bonsai → Groq → Gemini</span></article>
    </div>
    <div class="policy-footnote">Routes are architecture defaults from the plan, not verified live availability. Provider selection and route execution are separate controls.</div>
  </section>

  <div class="fleet-columns">
    <section class="panel targets-panel">
      <div class="panel-heading"><div><span class="section-kicker">WORKSPACES</span><h2>Active scope</h2></div><span class="count-chip">{(status?.pilot.crates.length ?? 0) + 1}</span></div>
      <article class="target-card control-target">
        <div class="target-top"><span class="target-icon">CL</span><span class="target-tag control-tag">CONTROL CENTER</span></div>
        <h3>cratelab</h3><p>Owns the router, local settings, run history, safety rules, and this UI.</p>
        <div class="target-foot"><span>Self-modification</span><b>Off for this test</b></div>
      </article>
      <article class="target-card">
        <div class="target-top"><span class="target-icon target-icon-green">RS</span><span class="target-tag">RUST LIBRARY</span></div>
        <h3>tv-rs workspace</h3><p>All catalogued crates are eligible for read-only research and documentation tasks. Work is triaged in small batches.</p>
        <div class="target-foot"><span>Workspace source</span><b class:status-ok={status?.target_available} class:status-bad={status && !status.target_available}>{status?.target_available ? `${status?.pilot.crates.length} crates available` : loading ? "Checking…" : "Not found"}</b></div>
      </article>
      <div class="scope-note"><strong>Scope lock</strong><span>Other crates and private repositories are excluded. The target workspace is never modified by a dry run.</span></div>
    </section>

    <section class="panel run-panel">
      <div class="panel-heading"><div><span class="section-kicker">TASK DISPATCH</span><h2>Prepare a run</h2></div><span class="local-only">LOCAL ONLY</span></div>
      <label class="field-label" for="fleet-crate">Crate</label>
      <select id="fleet-crate" bind:value={crateId} disabled={busy || liveBusy || loading}>
        {#each status?.pilot.crates ?? [] as crate (crate.id)}<option value={crate.id}>{crate.package}</option>{/each}
      </select>
      <label class="field-label" for="fleet-task">Task</label>
      <select id="fleet-task" bind:value={task}>
        {#each status?.pilot.tasks ?? ["rustdoc-audit"] as taskOption (taskOption)}
          <option value={taskOption}>{taskOption === "rustdoc-audit" ? "Rustdoc audit · documentation only" : "Research summary · evidence and open questions"}</option>
        {/each}
      </select>
      <label class="field-label" for="fleet-provider">Preferred provider (saved locally)</label>
      <select id="fleet-provider" value={status?.selected_provider ?? ""} onchange={(event) => event.currentTarget.value && selectProvider(event.currentTarget.value)} disabled={busy || loading}>
        <option value="">No provider selected</option>
        {#each (status?.providers ?? []).filter((p) => ["gemini", "groq", "cloudflare", "nvidia", "openrouter", "zhipu", "colab-bonsai", "kaggle-bonsai"].includes(p.id)) as p (p.id)}
          <option value={p.id}>{p.name}{p.credential_ready ? " · credential present" : " · not configured"}</option>
        {/each}
      </select>
      {#if selectedInference()?.available_models.length}
        <label class="field-label" for="fleet-model">Model from latest provider probe</label>
        <select id="fleet-model" value={selectedInference()?.selected_model ?? ""} onchange={(event) => selectModel(event.currentTarget.value)} disabled={busy || loading}>
          <option value="">Select a discovered model</option>
          {#each selectedInference()?.available_models ?? [] as model (model)}<option value={model}>{model}</option>{/each}
        </select>
      {/if}
      <div class="dispatch-hint"><span class="hint-icon">i</span><span>Selection is local. Use the provider row’s Probe button to verify credentials; that sends the key only, not crate files.</span></div>
      <button class="ghost provider-run-button" onclick={stageBatch} disabled={loading || !status?.pilot.crates.length}>Stage next batch of {status?.pilot.batch_size ?? 3}</button>
      {#if queue.length}
        <div class="queue-list" aria-label="Local crate task queue">
          <strong>Local queue · {queue.filter((item) => item.state === "pending").length} pending</strong>
          {#each queue.slice(-12) as item (item.id)}
            <button class="queue-item" class:queue-done={item.state === "done"} onclick={() => selectQueueItem(item)} disabled={item.state === "done"}>
              <span>{status?.pilot.crates.find((crate) => crate.id === item.crate_id)?.package ?? item.crate_id}</span>
              <small>{item.task} · {item.state}</small>
            </button>
          {/each}
        </div>
      {/if}
      <button class="ghost provider-run-button" onclick={previewLiveRun} disabled={liveBusy || !selectedInference()?.credential_ready || selectedInference()?.probe_ok !== true || !selectedInference()?.selected_model}>
        {liveBusy ? "Preparing…" : "Preview outbound model run"}
      </button>
      <button class="primary run-button" onclick={runDryRun} disabled={busy || loading || !status?.target_available}>
        {#if busy}<span class="button-spinner"></span> Working…{:else}<span>▶</span> Run local dry audit{/if}
      </button>
      <p class="button-caption">Scans the selected crate and writes a local report. No edits, issues, or PRs.</p>
      {#if lastRun}
        <div class="run-result" role="status">
          <div class="result-top"><span class="result-check">✓</span><strong>Dry run complete</strong><span class="result-time">{time(lastRun.completed_at)}</span></div>
          <div class="result-stats"><span><b>{lastRun.source_files}</b> Rust files</span><span><b>{lastRun.doc_candidates}</b> doc candidates</span></div>
          <code class="report-path">{lastRun.report}</code>
        </div>
      {/if}
      {#if liveError}<div class="fleet-alert" role="alert">{liveError}</div>{/if}
      {#if liveAnswer}<div class="provider-answer"><div class="answer-heading"><strong>Provider audit result</strong><span>Saved locally · review before any edits</span></div><pre>{liveAnswer}</pre></div>{/if}
    </section>
  </div>

  <section class="panel providers-panel">
    <div class="panel-heading providers-heading">
      <div><span class="section-kicker">SERVICE MAP</span><h2>Providers & infrastructure</h2><p>Credential readiness is local metadata only. Green does not mean the service has been health-checked.</p></div>
      <div class="legend"><span class="legend-dot ready"></span>Credential present <span class="legend-dot missing"></span>Not configured</div>
    </div>
    <div class="provider-groups">
      {#each groups as group (group.label)}
        <div class="provider-group">
          <h3>{group.label}</h3>
          <div class="provider-list">
            {#each group.ids.map(provider) as p (p?.id)}
              {#if p}
                <div class="provider-row" class:provider-selected={p.selected}>
                  <span class="provider-mark" class:mark-ready={p.credential_ready}>{p.name.slice(0, 2).toUpperCase()}</span>
                  <div class="provider-copy"><strong>{p.name}</strong><small>{p.probe_ok === true && p.available_models.length ? `${p.available_models.length} model IDs discovered` : p.note}</small></div>
                  <span class="provider-state" class:state-ready={p.probe_ok === true} class:state-pending={p.credential_ready && p.probe_ok == null} class:state-failed={p.probe_ok === false}><i></i>{p.state}</span>
                  {#if p.probe_supported && p.credential_ready}
                    <button class="probe-button" onclick={() => openProbe(p)} disabled={probeBusy}>{p.probe_ok === true ? "Recheck" : "Probe"}</button>
                  {:else}<span class="probe-spacer"></span>{/if}
                  <span class="provider-role">{p.role}</span>
                </div>
              {/if}
            {/each}
          </div>
        </div>
      {/each}
    </div>
    <div class="provider-footnote"><strong>Secrets are not shown here.</strong> Add credentials only to the ignored local .env file. The UI checks whether a value exists; it never displays or logs the value.</div>
  </section>

  <section class="rag-panel panel">
    <div class="panel-heading"><div><span class="section-kicker">KNOWLEDGE PIPELINE</span><h2>Retrieval-augmented context</h2><p>Build only when direct crate context stops being enough.</p></div><span class="local-only">NOT DEPLOYED</span></div>
    <div class="rag-track">
      <div class="rag-step"><span>01 · SOURCE</span><strong>tv-rs workspace</strong><small>All {status?.pilot.crates.length ?? 0} crates catalogued</small></div><span class="rag-arrow">→</span>
      <div class="rag-step rag-pending"><span>02 · EMBED</span><strong>Kaggle + BGE-M3</strong><small>Batch index with crate:// headers</small></div><span class="rag-arrow">→</span>
      <div class="rag-step rag-pending"><span>03 · STORE</span><strong>Hugging Face</strong><small>Private dataset is source of truth</small></div><span class="rag-arrow">→</span>
      <div class="rag-step rag-pending"><span>04 · SEARCH</span><strong>Modal /search</strong><small>Optional Qdrant serving layer</small></div>
    </div>
    <div class="rag-foot"><span>Current mode: direct local source scan</span><span>No dataset upload, embedding job, or hosted endpoint has run</span></div>
  </section>

  <section class="panel history-panel">
    <div class="panel-heading"><div><span class="section-kicker">LOCAL LEDGER</span><h2>Recent runs</h2></div><span class="count-chip">{status?.runs.length ?? 0}</span></div>
    {#if status?.runs.length}
      <div class="run-table-wrap"><table class="run-table"><thead><tr><th>Task</th><th>Target</th><th>Mode</th><th>Source files</th><th>Doc candidates</th><th>Time</th></tr></thead><tbody>
        {#each status.runs as run, i (`${run.completed_at}-${i}`)}<tr><td>{run.task}</td><td>{run.target}</td><td><span class="mode-chip">{run.mode}</span></td><td>{run.source_files}</td><td>{run.doc_candidates}</td><td>{time(run.completed_at)}</td></tr>{/each}
      </tbody></table></div>
    {:else}
      <div class="empty-history"><span class="empty-icon">◷</span><div><strong>No runs recorded yet</strong><p>Run the local dry audit to create the first entry in this machine's ledger.</p></div></div>
    {/if}
  </section>

  <footer class="fleet-footer"><span>cratelab · fleet module</span><span>Inference requires per-run confirmation. RAG uploads and CI/GitHub writes remain off.</span></footer>

  {#if probeTarget}
    <div class="modal-backdrop" role="presentation" onclick={(event) => event.target === event.currentTarget && !probeBusy && (probeTarget = null)}>
      <dialog class="confirm-modal" open aria-labelledby="probe-title">
        <span class="modal-kicker">READ-ONLY CONNECTION CHECK</span>
        <h2 id="probe-title">Probe {probeTarget.name}?</h2>
        <p>The configured credential will be sent in an authenticated request to this provider endpoint. No crate source or prompt content is sent.</p>
        <code class="modal-destination">{probeTarget.probe_destination}</code>
        <div class="modal-detail"><strong>What is sent</strong><span>{probeTarget.id === "huggingface" ? "HF_TOKEN" : probeTarget.id === "circleci" ? "CIRCLECI_TOKEN" : probeTarget.id === "kaggle" ? "KAGGLE_API_KEY" : probeTarget.id === "cloudflare" ? "CF_API_TOKEN" : probeTarget.id === "qdrant" ? "QDRANT_API_KEY" : probeTarget.id === "modelscope" ? "MODELSCOPE_TOKEN" : probeTarget.id === "openrouter" ? "OPENROUTER_API_KEY" : probeTarget.id === "gemini" ? "GEMINI_API_KEY" : probeTarget.id === "groq" || probeTarget.id === "allam" ? "GROQ_API_KEY" : probeTarget.id === "nvidia" ? "NVIDIA_API_KEY" : probeTarget.id === "zhipu" ? "ZHIPU_API_KEY" : "Configured endpoint credential"} only</span></div>
        <div class="modal-detail"><strong>Stored locally</strong><span>Success/failure status, timestamp, and model IDs only. Response body is discarded.</span></div>
        {#if probeMessage}<div class="modal-message" role="status">{probeMessage}</div>{/if}
        <div class="modal-actions"><button class="ghost" onclick={() => (probeTarget = null)} disabled={probeBusy}>Cancel</button><button class="primary" onclick={confirmProbe} disabled={probeBusy}>{probeBusy ? "Probing…" : "Send credential and probe"}</button></div>
      </dialog>
    </div>
  {/if}

  {#if livePreview}
    <div class="modal-backdrop" role="presentation">
      <dialog class="confirm-modal live-modal" open aria-labelledby="live-title">
        <span class="modal-kicker">OUTBOUND MODEL REQUEST</span>
        <h2 id="live-title">Send crate context to {livePreview.provider}?</h2>
        <p>This sends public README and Rust source files for {livePreview.target} to the selected provider. Cargo manifests and lockfiles are excluded. Its data handling follows that provider’s terms.</p>
        <code class="modal-destination">{livePreview.destination}</code>
        <div class="modal-detail"><strong>Model</strong><span>{livePreview.model}</span></div>
        <div class="modal-detail"><strong>Files and size</strong><span>{livePreview.files.length} files · {Math.ceil(livePreview.source_bytes / 1024)} KB source context · max 1,200 output tokens</span></div>
        <div class="modal-detail"><strong>Eligible route</strong><span>{livePreview.route.map((item) => `${item.provider} (${item.model}, local cap remaining ${item.requests_left_today})`).join(" → ") || "No fallback is currently eligible"}</span></div>
        <div class="modal-file-list">{#each livePreview.files as file (file)}<span>{file}</span>{/each}</div>
        <div class="modal-detail"><strong>Routing and cost</strong><span>After confirmation, configured and successfully probed fallback providers may also receive the same source context, within local request caps. Calls may consume quota or incur charges depending on account billing; verify every eligible model is free before continuing.</span></div>
        {#if liveError}<div class="modal-message" role="alert">{liveError}</div>{/if}
        <div class="modal-actions"><button class="ghost" onclick={() => (livePreview = null)} disabled={liveBusy}>Cancel</button><button class="primary" onclick={confirmLiveRun} disabled={liveBusy}>{liveBusy ? "Sending…" : `Send source to ${livePreview.provider}`}</button></div>
      </dialog>
    </div>
  {/if}
</section>
