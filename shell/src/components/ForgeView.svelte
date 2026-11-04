<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import { forgeWorkflows } from "../forge-workflows";
  import type { CircleCiDashboard, ForgeLivePreview, ForgeProvider, ForgeRun, ForgeStatus, LearningNote } from "../types";

  let status = $state<ForgeStatus | null>(null);
  let busy = $state(false);
  let loading = $state(true);
  let error = $state("");
  let lastRun = $state<ForgeRun | null>(null);
  let task = $state<string>("rustdoc-audit");
  let caseContext = $state("");
  let learningNotes = $state("");
  let crateId = $state("discovery");
  const workflow = $derived(forgeWorkflows.find((item) => item.id === task));
  let ragProjectId = $state("simple-signage");
  type QueueItem = { id: string; repository?: string; crate_id: string; task: string; state: "pending" | "done" };
  let queue = $state<QueueItem[]>([]);
  let probeTarget = $state<ForgeProvider | null>(null);
  let probeBusy = $state(false);
  let probeMessage = $state("");
  let livePreview = $state<ForgeLivePreview | null>(null);
  let liveBusy = $state(false);
  let liveAnswer = $state("");
  let liveError = $state("");
  let circleCi = $state<CircleCiDashboard | null>(null);
  let circleCiReview = $state(false);
  let circleCiBusy = $state(false);
  let circleCiError = $state("");

  const groups = [
    { label: "Inference", ids: ["gemini", "groq", "allam", "cloudflare", "nvidia", "openrouter", "zhipu"] },
    { label: "Model hosts", ids: ["colab-bonsai", "kaggle-bonsai"] },
    { label: "Knowledge & compute", ids: ["huggingface", "kaggle", "modal", "qdrant"] },
    { label: "Delivery & source", ids: ["circleci", "github-read"] },
  ];

  onMount(() => {
    try { queue = JSON.parse(localStorage.getItem("cratelab-forge-queue") ?? "[]"); } catch { queue = []; }
    void refresh();
  });

  function saveQueue(next: QueueItem[]) {
    queue = next;
    localStorage.setItem("cratelab-forge-queue", JSON.stringify(next));
  }

  function stageBatch() {
    if (!status) return;
    const repository = status.target;
    const alreadyStaged = new Set(queue.map((item) => `${item.repository ?? "tv-rs"}:${item.crate_id}:${item.task}`));
    const start = Math.max(0, status.pilot.crates.findIndex((crate) => crate.id === crateId));
    const ordered = [...status.pilot.crates.slice(start), ...status.pilot.crates.slice(0, start)];
    const next = ordered.filter((crate) => !alreadyStaged.has(`${repository}:${crate.id}:${task}`)).slice(0, status.pilot.batch_size);
    const additions = next.map((crate) => ({ id: `${repository}:${task}:${crate.id}`, repository, crate_id: crate.id, task, state: "pending" as const }));
    saveQueue([...queue, ...additions]);
  }

  function completeQueueItem(repository: string, crate: string, taskName: string) {
    const updated = queue.map((item) => (item.repository ?? "tv-rs") === repository && item.crate_id === crate && item.task === taskName && item.state === "pending" ? { ...item, state: "done" as const } : item);
    saveQueue(updated);
  }

  function selectQueueItem(item: QueueItem) {
    if (item.state !== "pending") return;
    if (item.repository && item.repository !== status?.target) return;
    crateId = item.crate_id;
    task = item.task;
  }

  async function refresh() {
    loading = true;
    error = "";
    try {
      status = await api.forgeStatus();
      if (!status.pilot.crates.some((crate) => crate.id === crateId)) crateId = status.pilot.crates[0]?.id ?? "";
      learningNotes = currentLearningNotes(status.target, task);
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
      status = await api.forgeSelectProvider(id);
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
      lastRun = await api.forgeDryRun(task, crateId, caseContext);
      completeQueueItem(status?.target ?? "tv-rs", crateId, task);
      status = await api.forgeStatus();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function openProbe(p: ForgeProvider) {
    if (!p.credential_ready || !p.probe_supported) return;
    probeMessage = "";
    probeTarget = p;
  }

  async function confirmProbe() {
    if (!probeTarget) return;
    probeBusy = true;
    probeMessage = "";
    try {
      const result = await api.forgeProbeProvider(probeTarget.id);
      probeMessage = result.summary;
      status = await api.forgeStatus();
      if (result.ok) probeTarget = null;
    } catch (e) {
      probeMessage = String(e);
      status = await api.forgeStatus().catch(() => status);
    } finally {
      probeBusy = false;
    }
  }

  async function selectModel(model: string) {
    const selected = status?.providers.find((p) => p.selected && ["gemini", "groq", "allam", "cloudflare", "nvidia", "openrouter", "zhipu", "modelscope", "colab-bonsai", "kaggle-bonsai"].includes(p.id));
    if (!selected || !model) return;
    busy = true;
    try {
      status = await api.forgeSelectModel(selected.id, model);
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
      learningNotes = currentLearningNotes(status?.target ?? "tv-rs", task);
      livePreview = await api.forgePreviewLive(selected.id, selected.selected_model, crateId, task, ragProjectId, caseContext, learningNotes);
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
      const result = await api.forgeGenerateAudit(livePreview.provider, livePreview.model, crateId, task, livePreview.retrieval_project, caseContext, learningNotes);
      liveAnswer = result.answer;
      lastRun = result.run;
      completeQueueItem(status?.target ?? "tv-rs", crateId, task);
      status = await api.forgeStatus();
      livePreview = null;
    } catch (e) {
      liveError = String(e);
    } finally {
      liveBusy = false;
    }
  }

  function currentLearningNotes(repository: string, workflow: string): string {
    try {
      const notes = JSON.parse(localStorage.getItem("cratelab-learning-notes") ?? "[]") as LearningNote[];
      return notes.filter((item) => item.repository === repository && item.task === workflow).slice(-5)
        .map((item) => `- ${item.rating}: ${item.note}`).join("\n");
    } catch { return ""; }
  }

  // Capture a reviewed outcome note. Local-only, repository/task scoped;
  // the next case for the same repo + workflow carries it into its prompt.
  let noteRating = $state<"useful" | "partial" | "wrong">("useful");
  let noteText = $state("");
  let noteSaved = $state(false);
  function saveLearningNote() {
    const note = noteText.trim();
    if (!note || !status) return;
    let notes: LearningNote[] = [];
    try { notes = JSON.parse(localStorage.getItem("cratelab-learning-notes") ?? "[]") as LearningNote[]; } catch { notes = []; }
    notes.push({ repository: status.target, task, crate_id: crateId, rating: noteRating, note, created_at: Math.floor(Date.now() / 1000) });
    localStorage.setItem("cratelab-learning-notes", JSON.stringify(notes.slice(-200)));
    noteText = "";
    noteRating = "useful";
    noteSaved = true;
    learningNotes = currentLearningNotes(status.target, task);
    setTimeout(() => (noteSaved = false), 2500);
  }

  function selectedInference(): ForgeProvider | undefined {
    return status?.providers.find((p) => p.selected && ["gemini", "groq", "allam", "cloudflare", "nvidia", "openrouter", "zhipu", "modelscope", "colab-bonsai", "kaggle-bonsai"].includes(p.id));
  }

  function provider(id: string): ForgeProvider | undefined {
    return status?.providers.find((p) => p.id === id);
  }

  function time(timestamp: number): string {
    return new Date(timestamp * 1000).toLocaleString();
  }

  // The rail proves the ordering rule: local evidence (first steps) is
  // collected before any provider call, and steps can fail independently.
  function stepState(index: number): "done" | "active" | "todo" {
    const steps = workflow?.steps ?? [];
    const evidenceCount = Math.min(2, Math.max(1, steps.length - 1));
    if (index < evidenceCount) return lastRun ? "done" : "todo";
    if (index === evidenceCount) return liveAnswer ? "done" : lastRun ? "active" : "todo";
    return liveAnswer ? "done" : "todo";
  }

  function ciTime(value: string | null): string {
    return value ? new Date(value).toLocaleString() : "—";
  }

  async function loadCircleCi() {
    circleCiBusy = true;
    circleCiError = "";
    try { circleCi = await api.forgeCircleciDashboard(); circleCiReview = false; }
    catch (e) { circleCiError = String(e); }
    finally { circleCiBusy = false; }
  }

  let configuredCount = $derived(status?.providers.filter((p) => p.credential_ready).length ?? 0);
  let providerCount = $derived(status?.providers.length ?? 0);
</script>

<section class="forge-page" aria-labelledby="forge-title">
  <header class="forge-header">
    <div>
      <div class="eyebrow"><span class="eyebrow-dot"></span> LOCAL CONTROL PLANE</div>
      <h1 id="forge-title">Work orchestration <span class="safe-pill">REVIEW BEFORE DISPATCH</span></h1>
      <p class="forge-lede">Prepare a case, choose local evidence or provider analysis, and review the result for {status?.target ?? "the selected repository"}.</p>
    </div>
    <button class="ghost refresh-button" onclick={refresh} disabled={loading} aria-label="Refresh local provider readiness">
      <span class:spin={loading}>↻</span> Refresh status
    </button>
  </header>

  {#if error}<div class="forge-alert" role="alert"><span>{error}</span><button class="alert-close" onclick={() => (error = "")} aria-label="Dismiss">×</button></div>{/if}
  {#if loading}<p class="forge-lede">Loading forge state…</p>{/if}

  <div class="forge-metrics">
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
      <span class="metric-label">Selected repository</span><strong>{status?.target ?? "—"}</strong><small>{status?.pilot.crates.length ?? "—"} crates · batches of {status?.pilot.batch_size ?? 3}</small>
    </article>
  </div>

  <section class="architecture-card" aria-label="Forge architecture">
    <div class="section-heading">
      <div><span class="section-kicker">HOW A RUN FLOWS</span><h2>One control plane, staged execution</h2></div>
      <span class="flow-caption">Secrets stay local · source context goes only to the chosen/eligible provider</span>
    </div>
    <div class="flow-track">
      <div class="flow-node"><span class="flow-index">01</span><strong>cratelab</strong><small>queue · policy · ledger</small></div>
      <span class="flow-arrow">→</span>
      <div class="flow-node"><span class="flow-index">02</span><strong>Workflow</strong><small>case · scope · evidence</small></div>
      <span class="flow-arrow">→</span>
      <div class="flow-node flow-node-muted"><span class="flow-index">03</span><strong>Provider ladder</strong><small>confirmed · capped · logged</small></div>
      <span class="flow-arrow">→</span>
      <div class="flow-node flow-node-muted"><span class="flow-index">04</span><strong>Knowledge context</strong><small>HF · Modal · optional docs</small></div>
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

  <div class="forge-columns">
    <section class="panel targets-panel">
      <div class="panel-heading"><div><span class="section-kicker">REPOSITORY</span><h2>{status?.target ?? "Selected source"}</h2></div><span class="count-chip">{status?.pilot.crates.length ?? 0} crates</span></div>
      <article class="target-card control-target">
        <div class="target-top"><span class="target-icon">CL</span><span class="target-tag control-tag">CONTROL CENTER</span></div>
        <h3>cratelab</h3><p>Owns the router, local settings, run history, safety rules, and this UI.</p>
        <div class="target-foot"><span>Self-modification</span><b>Off for this test</b></div>
      </article>
      <article class="target-card">
        <div class="target-top"><span class="target-icon target-icon-green">RS</span><span class="target-tag">RUST LIBRARY</span></div>
        <h3>{status?.target ?? "Rust workspace"}</h3><p>{status?.target_path}. Repository selection is shared with the Ops Dashboard; source scans are read-only.</p>
        <div class="target-foot"><span>Workspace source</span><b class:status-ok={status?.target_available} class:status-bad={status && !status.target_available}>{status?.target_available ? `${status?.pilot.crates.length} crates available` : loading ? "Checking…" : "Path unavailable"}</b></div>
      </article>
      <div class="scope-note"><strong>Read-only target</strong><span>Only the selected crate source enters a case. Local preparation writes reports under Forge; it never edits this repository.</span></div>
    </section>

    <section class="panel run-panel">
      <div class="panel-heading"><div><span class="section-kicker">CASE WORKBENCH</span><h2>Build a case packet</h2></div><span class="local-only">DRY RUN DEFAULT</span></div>

      <div class="case-selectors">
        <div class="case-field">
          <span class="field-label">Repository</span>
          <b class="case-repo">{status?.target ?? "—"}</b>
          <small>switch scope in the top header</small>
        </div>
        <label class="case-field" for="forge-crate">
          <span class="field-label">Crate</span>
          <select id="forge-crate" bind:value={crateId} disabled={busy || liveBusy || loading}>
            {#each status?.pilot.crates ?? [] as crate (crate.id)}<option value={crate.id}>{crate.package}</option>{/each}
          </select>
        </label>
        <label class="case-field" for="forge-task">
          <span class="field-label">Task</span>
          <select id="forge-task" bind:value={task}>
            {#each forgeWorkflows as workflow (workflow.id)}
              <option value={workflow.id}>{workflow.group} · {workflow.label}</option>
            {/each}
          </select>
        </label>
        <label class="case-field" for="forge-provider">
          <span class="field-label">Provider (rung)</span>
          <select id="forge-provider" value={status?.selected_provider ?? ""} onchange={(event) => event.currentTarget.value && selectProvider(event.currentTarget.value)} disabled={busy || loading}>
            <option value="">No provider selected</option>
            {#each (status?.providers ?? []).filter((p) => ["gemini", "groq", "cloudflare", "nvidia", "openrouter", "zhipu", "colab-bonsai", "kaggle-bonsai"].includes(p.id)) as p (p.id)}
              <option value={p.id}>{p.name}{p.credential_ready ? " · credential present" : " · not configured"}</option>
            {/each}
          </select>
          <small>always explicit · no silent fallback</small>
        </label>
        {#if selectedInference()?.available_models.length}
          <label class="case-field" for="forge-model">
            <span class="field-label">Model</span>
            <select id="forge-model" value={selectedInference()?.selected_model ?? ""} onchange={(event) => selectModel(event.currentTarget.value)} disabled={busy || loading}>
              <option value="">Select a discovered model</option>
              {#each selectedInference()?.available_models ?? [] as model (model)}<option value={model}>{model}</option>{/each}
            </select>
            <small>served by the rung · from its last probe</small>
          </label>
        {/if}
      </div>

      <div class="step-rail" aria-label="Case steps">
        {#each workflow?.steps ?? [] as step, i (i)}
          <div class="step-node" class:step-done={stepState(i) === "done"} class:step-active={stepState(i) === "active"}>
            <span class="step-index">{i + 1}</span><span class="step-label">{step}</span>
          </div>
        {/each}
      </div>
      <p class="button-caption">Local evidence is collected before any provider call. A failed context step degrades to local-evidence-only and says so.</p>
      <div class="workflow-recipe"><strong>{workflow?.local}</strong><ol>{#each workflow?.steps ?? [] as step, i (`${workflow?.id}-${i}`)}<li>{step}</li>{/each}</ol><small>Provider step: {workflow?.live}</small></div>
      <label class="field-label" for="case-context">Case details / issue text (optional)</label>
      <textarea id="case-context" class="case-context-input" bind:value={caseContext} rows="4" maxlength="12000" placeholder="Describe the symptom, feature question, or issue text. Issue links are not fetched automatically."></textarea>
      {#if learningNotes}<div class="learning-context"><strong>Learning notes carried into this workflow</strong><pre>{learningNotes}</pre></div>{/if}
      <div class="learning-capture">
        <label class="field-label" for="learning-note">Capture learning note (local, {status?.target ?? "repo"} / {task})</label>
        <div class="learning-capture-row">
          <select id="learning-rating" bind:value={noteRating}>
            <option value="useful">useful</option>
            <option value="partial">partial</option>
            <option value="wrong">wrong</option>
          </select>
          <input id="learning-note" bind:value={noteText} placeholder="What did this workflow get right or wrong? Carried into future cases for this repo + workflow." maxlength="2000" />
          <button class="ghost" onclick={saveLearningNote} disabled={!noteText.trim()}>Save note</button>
        </div>
        {#if noteSaved}<small class="note-saved">✓ saved — included in the next case for this repository + workflow</small>{/if}
      </div>
      <label class="field-label" for="forge-rag-project">Signage docs context (optional)</label>
      <select id="forge-rag-project" bind:value={ragProjectId} disabled={liveBusy}>
        <option value="">No signage docs</option>
        <option value="simple-signage">Simple-Signage docs</option>
        <option value="companion-app">Companion-app docs</option>
      </select>
      <div class="dispatch-hint"><span class="hint-icon">i</span><span>Selected docs are private project material. After you confirm, up to six matching snippets may be sent to Modal and then the inference provider.</span></div>
      <div class="dispatch-hint"><span class="hint-icon">i</span><span>Selection is local. Use the rung row’s Probe button to verify credentials; that sends the key only, not crate files.</span></div>
      <button class="ghost provider-run-button" onclick={stageBatch} disabled={loading || !status?.pilot.crates.length}>Stage next batch of {status?.pilot.batch_size ?? 3}</button>
      {#if queue.some((item) => (item.repository ?? "tv-rs") === status?.target)}
        <div class="queue-list" aria-label="Local crate task queue">
          <strong>{status?.target} cases · {queue.filter((item) => (item.repository ?? "tv-rs") === status?.target && item.state === "pending").length} pending</strong>
          {#each queue.filter((item) => (item.repository ?? "tv-rs") === status?.target).slice(-12) as item (item.id)}
            <button class="queue-item" class:queue-done={item.state === "done"} onclick={() => selectQueueItem(item)} disabled={item.state === "done"}>
              <span>{status?.pilot.crates.find((crate) => crate.id === item.crate_id)?.package ?? item.crate_id}</span>
              <small>{item.task} · {item.state}</small>
            </button>
          {/each}
        </div>
      {/if}
      <div class="live-gates">
        <span class:gate-ok={selectedInference()?.probe_ok === true}>rung probed ✓</span>
        <span class:gate-ok={!!selectedInference()?.selected_model}>model explicit</span>
        <span class:gate-ok={!!lastRun}>local evidence prepared</span>
      </div>
      <div class="run-actions">
        <button class="primary run-button" onclick={runDryRun} disabled={busy || loading || !status?.target_available}>
          {#if busy}<span class="button-spinner"></span> Preparing…{:else}<span>▶</span> Dry run{/if}
        </button>
        <button class="ghost provider-run-button live-run" onclick={previewLiveRun} disabled={liveBusy || !selectedInference()?.credential_ready || selectedInference()?.probe_ok !== true || !selectedInference()?.selected_model}>
          {liveBusy ? "Preparing…" : "Run live (review first)"}
        </button>
      </div>
      <p class="button-caption">Dry run is local and free — it prepares evidence only. Run live opens the outbound request for review first and stays on free-tier rungs.</p>
      {#if lastRun || liveAnswer}
        <div class="evidence-split">
          <div class="evidence-pane">
            <span class="section-kicker">EVIDENCE · LOCAL, BEFORE ANY RESULT</span>
            {#if lastRun}
              <div class="run-result" role="status">
                <div class="result-top"><span class="result-check">✓</span><strong>Case packet prepared · {lastRun.repository ?? status?.target}</strong><span class="result-time">{time(lastRun.completed_at)}</span></div>
                <div class="result-stats"><span><b>{lastRun.source_files}</b> Rust files</span><span><b>{lastRun.doc_candidates}</b> doc candidates</span></div>
                <code class="report-path">{lastRun.report}</code>
              </div>
            {:else}
              <p class="forge-lede">No local evidence yet — run a dry run first.</p>
            {/if}
          </div>
          <div class="evidence-pane">
            <span class="section-kicker">RESULT · PROVIDER, REVIEWED</span>
            {#if liveAnswer}
              <div class="provider-answer"><div class="answer-heading"><strong>Analysis result · {task}</strong><span>Saved locally · review before any follow-up</span></div><pre>{liveAnswer}</pre></div>
            {:else}
              <p class="forge-lede">No provider result yet. Dry run first, then Run live with a probed rung.</p>
            {/if}
          </div>
        </div>
      {/if}
      {#if liveError}<div class="forge-alert" role="alert"><span>{liveError}</span><button class="alert-close" onclick={() => (liveError = "")} aria-label="Dismiss">×</button></div>{/if}
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
            {#each group.ids as providerId (providerId)}
              {@const p = provider(providerId)}
              {#if p}
                <div class="provider-row" class:provider-selected={p.selected}>
                  <span class="provider-mark" class:mark-ready={p.credential_ready}>{p.name.slice(0, 2).toUpperCase()}</span>
                  <div class="provider-copy"><strong>{p.name}</strong><small>{p.probe_ok === true && p.available_models.length ? `${p.available_models.length} model IDs discovered` : p.note}</small></div>
                  <span class="provider-state" class:state-ready={p.probe_ok === true} class:state-pending={p.credential_ready && p.probe_ok == null} class:state-failed={p.probe_ok === false}><i></i>{p.state}</span>
                  {#if p.probe_supported && p.credential_ready}
                    <button class="probe-button" onclick={() => openProbe(p)} disabled={probeBusy}>{p.probe_ok === true ? "Recheck" : "Probe"}</button>
                  {:else if !p.credential_ready && p.setup_url}
                    <a class="probe-button setup-provider-link" href={p.setup_url} target="_blank" rel="noreferrer">Get access ↗</a>
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
    <div class="panel-heading"><div><span class="section-kicker">KNOWLEDGE PIPELINE</span><h2>Retrieval-augmented context</h2><p>Docs-only corpus from Simple-Signage and companion-app; private HF sync and Modal search are opt-in.</p></div><span class="local-only">{status?.retrieval.enabled ? "CONNECTED" : "LOCAL CORPUS"}</span></div>
    <div class="rag-track">
      <div class="rag-step"><span>01 · SOURCE</span><strong>Signage project docs</strong><small>Simple-Signage + companion-app · corpus stays local</small></div><span class="rag-arrow">→</span>
      <div class="rag-step rag-pending"><span>02 · INDEX</span><strong>Local BM25</strong><small>Chunked docs with project:// provenance</small></div><span class="rag-arrow">→</span>
      <div class="rag-step rag-pending"><span>03 · STORE</span><strong>Hugging Face</strong><small>Private dataset is source of truth</small></div><span class="rag-arrow">→</span>
      <div class="rag-step" class:rag-pending={!status?.retrieval.enabled}><span>04 · SEARCH</span><strong>Modal /search</strong><small>{status?.retrieval.enabled ? status.retrieval.destination : "Configure endpoint and token to enable"}</small></div>
    </div>
    <div class="rag-foot"><span>{status?.retrieval.enabled ? "Retrieval runs only after provider-run confirmation" : "Current mode: direct selected-crate scan"}</span><span>Dataset upload and deployment remain manual</span></div>
  </section>

  <section class="panel circleci-panel">
    <div class="panel-heading providers-heading">
      <div><span class="section-kicker">RESEARCH & DOCS GATES</span><h2>CircleCI operations</h2><p>Read-only account, project, pipeline, and workflow status. Forge does not trigger pipelines or change CircleCI/GitHub settings.</p></div>
      <button class="ghost" onclick={() => (circleCiReview = true)} disabled={circleCiBusy || !provider("circleci")?.credential_ready}>{circleCiBusy ? "Loading…" : circleCi ? "Refresh CI status" : "Review status request"}</button>
    </div>
    {#if circleCi}
      <div class="ci-metrics">
        <article><span>Account</span><strong>{circleCi.account.name || circleCi.account.login || "Authenticated"}</strong></article>
        <article><span>Projects connected</span><strong>{circleCi.connected_count} / {circleCi.configured_count}</strong></article>
        <article><span>Organizations visible</span><strong>{circleCi.organizations.length}</strong></article>
        <article><span>Last checked</span><strong>{time(circleCi.checked_at)}</strong></article>
      </div>
      {#if !circleCi.setup.project_slugs_configured}
        <div class="ci-setup-note"><strong>Project list is empty.</strong><span>In Forge Settings, set <code>CIRCLECI_PROJECT_SLUGS</code> to a comma-separated list such as <code>gh/Zakarialabib/tv-rs</code>.</span><a href="https://app.circleci.com/projects" target="_blank" rel="noreferrer">Connect a project in CircleCI ↗</a></div>
      {:else}
        <div class="ci-project-list">
          {#each circleCi.projects as project (project.slug)}
            <article class="ci-project-card">
              <div class="ci-project-heading"><div><strong>{project.name}</strong><code>{project.slug}</code></div><span class="ci-state" class:ci-connected={project.connected}>{project.connected ? project.state : "Not connected / not visible"}</span></div>
              {#if project.error}<p class="ci-project-error">{project.error}</p>{/if}
              {#if project.connected}
                {#if project.pipelines.length}
                  <div class="ci-pipeline-line"><strong>Latest pipeline #{project.pipelines[0].number ?? "—"}</strong><span>{project.pipelines[0].state || "unknown"}</span><span>{project.pipelines[0].branch || "branch unavailable"}</span><time>{ciTime(project.pipelines[0].created_at)}</time></div>
                  {#if project.latest_workflows.length}<div class="ci-workflow-list"><span class="section-kicker">LATEST PIPELINE WORKFLOWS</span>{#each project.latest_workflows as workflow (workflow.id)}<div><strong>{workflow.name}</strong><span class="ci-workflow-status">{workflow.status}</span><time>{ciTime(workflow.stopped_at || workflow.created_at)}</time></div>{/each}</div>{/if}
                {:else}<p class="ci-project-error">No pipelines have been returned for this project.</p>{/if}
              {/if}
            </article>
          {/each}
        </div>
      {/if}
      <div class="ci-template-status"><span class="legend-dot" class:ready={circleCi.setup.template_present}></span><span>Forge gate template {circleCi.setup.template_present ? "ready" : "missing"}: <code>{circleCi.setup.template_path}</code></span><span class="legend-dot" class:ready={circleCi.setup.target_config_present}></span><span>Target config {circleCi.setup.target_config_present ? "present" : "not installed"}: <code>{circleCi.setup.target_config_path}</code></span></div>
      {#if !circleCi.setup.target_config_present}<p class="ci-setup-note"><strong>Setup next:</strong> connect the selected repository’s project in CircleCI, review the Forge Rust gate template, then add it to that target through your normal Git review.</p>{/if}
    {:else}
      <div class="empty-history"><span class="empty-icon">CI</span><div><strong>CI status has not been loaded</strong><p>Configure a CircleCI token and project slug in Settings, then review the read-only request.</p></div></div>
    {/if}
      {#if circleCiError}<div class="forge-alert" role="alert"><span>{circleCiError}</span><button class="alert-close" onclick={() => (circleCiError = "")} aria-label="Dismiss">×</button></div>{/if}
    <div class="provider-footnote"><strong>External action boundary.</strong> Loading sends the CircleCI token to CircleCI API v2 and reads up to five recent pipeline rows per configured project. It does not start, cancel, or retry jobs.</div>
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

  <footer class="forge-footer"><span>cratelab · forge module</span><span>Inference requires per-run confirmation. RAG uploads and CI/GitHub writes remain off.</span></footer>

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
        <h2 id="live-title">Send {livePreview.repository} case to {livePreview.provider}?</h2>
        <p>This sends public README and Rust source files for {livePreview.target} to the selected provider. Cargo manifests and lockfiles are excluded. Its data handling follows that provider’s terms.</p>
        <code class="modal-destination">{livePreview.destination}</code>
        <div class="modal-detail"><strong>Model</strong><span>{livePreview.model}</span></div>
        <div class="modal-detail"><strong>Workflow and case</strong><span>{task} · {livePreview.case_context || "No extra case details supplied"}</span></div>
        {#if livePreview.learning_notes}<div class="modal-detail"><strong>Reviewed learning notes sent</strong><span>{livePreview.learning_notes}</span></div>{/if}
        <div class="modal-detail"><strong>Files and size</strong><span>{livePreview.files.length} files · {Math.ceil(livePreview.source_bytes / 1024)} KB source context · max 1,200 output tokens</span></div>
        <div class="modal-detail"><strong>Eligible route</strong><span>{livePreview.route.map((item) => `${item.provider} (${item.model}, local cap remaining ${item.requests_left_today})`).join(" → ") || "No fallback is currently eligible"}</span></div>
        {#if livePreview.retrieval_enabled && livePreview.retrieval_project}<div class="modal-detail"><strong>Private docs retrieval</strong><span>After confirmation, query {livePreview.retrieval_destination} for {livePreview.retrieval_project}; up to 6 matching snippets will be attached to this prompt and sent to the selected provider.</span></div>{/if}
        <div class="modal-file-list">{#each livePreview.files as file (file)}<span>{file}</span>{/each}</div>
        <div class="modal-detail"><strong>Routing and cost</strong><span>After confirmation, configured and successfully probed fallback providers may also receive the same source context, within local request caps. Calls may consume quota or incur charges depending on account billing; verify every eligible model is free before continuing.</span></div>
        {#if liveError}<div class="modal-message" role="alert">{liveError}</div>{/if}
        <div class="modal-actions"><button class="ghost" onclick={() => (livePreview = null)} disabled={liveBusy}>Cancel</button><button class="primary" onclick={confirmLiveRun} disabled={liveBusy}>{liveBusy ? "Sending…" : `Send source to ${livePreview.provider}`}</button></div>
      </dialog>
    </div>
  {/if}

  {#if circleCiReview}
    <div class="modal-backdrop" role="presentation">
      <dialog class="confirm-modal" open aria-labelledby="circleci-review-title">
        <span class="modal-kicker">READ-ONLY CIRCLECI REQUEST</span>
        <h2 id="circleci-review-title">Load CircleCI operations status?</h2>
        <p>Forge will send the saved <strong>CIRCLECI_TOKEN</strong> to CircleCI API v2 and request your account, visible organizations, and recent pipeline/workflow status for the configured project slugs.</p>
        <div class="modal-detail"><strong>Project slugs</strong><span>{circleCi?.projects.map((project) => project.slug).join(", ") || "Read from Forge Settings when you continue"}</span></div>
        <div class="modal-detail"><strong>Scope</strong><span>GET requests only · at most 12 projects · latest 5 pipeline summaries per project · no source upload, job trigger, cancellation, or settings changes.</span></div>
        <div class="modal-actions"><button class="ghost" onclick={() => (circleCiReview = false)} disabled={circleCiBusy}>Cancel</button><button class="primary" onclick={loadCircleCi} disabled={circleCiBusy}>{circleCiBusy ? "Reading CircleCI…" : "Send token and load status"}</button></div>
      </dialog>
    </div>
  {/if}
</section>
