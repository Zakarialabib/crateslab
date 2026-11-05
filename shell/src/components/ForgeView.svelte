<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { CircleCiDashboard, ForgeCatalog, ForgeLivePreview, ForgeProvider, ForgeRun, ForgeStatus, LearningNote } from "../types";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import * as Card from "$lib/components/ui/card";
  import * as Table from "$lib/components/ui/table";
  import * as Select from "$lib/components/ui/select";
  import * as Dialog from "$lib/components/ui/dialog";
  import { cn } from "$lib/utils";

  let status = $state<ForgeStatus | null>(null);
  let busy = $state(false);
  let loading = $state(true);
  let error = $state("");
  let lastRun = $state<ForgeRun | null>(null);
  let task = $state<string>("rustdoc-audit");
  let caseContext = $state("");
  let learningNotes = $state("");
  let crateId = $state("discovery");
  // The engine's authored registries drive this screen: the task catalog
  // (`forge.tasks.toml`) and the seven-stage workflow spine
  // (`forge.workflows.toml`). Nothing here is a hand-kept frontend table.
  let catalog = $state<ForgeCatalog | null>(null);
  let workflowId = $state("");
  const forgeTasks = $derived((catalog?.tasks ?? []).filter((item) => item.scopes.includes("pilot")));
  const workflow = $derived(forgeTasks.find((item) => item.id === task));
  const engineWorkflows = $derived(catalog?.workflows ?? []);
  const activeWorkflow = $derived(engineWorkflows.find((item) => item.id === workflowId) ?? engineWorkflows[0]);
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
      const current = await api.forgeStatus();
      status = current;
      catalog = await api.forgeCatalog();
      if (!current.pilot.crates.some((crate) => crate.id === crateId)) crateId = current.pilot.crates[0]?.id ?? "";
      if (!forgeTasks.some((item) => item.id === task)) task = forgeTasks[0]?.id ?? task;
      if (!engineWorkflows.some((item) => item.id === workflowId)) workflowId = engineWorkflows.find((item) => item.target === current.target)?.id ?? engineWorkflows[0]?.id ?? "";
      learningNotes = currentLearningNotes(current.target, task);
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

<section class="mx-auto max-w-[1500px] pb-7" aria-labelledby="forge-title">
  <header class="mb-5 flex items-end justify-between gap-5 max-md:flex-col max-md:items-start">
    <div>
      <div class="mb-[7px] flex items-center gap-[7px] text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground"><span class="h-[7px] w-[7px] rounded-full bg-success shadow-[0_0_0_3px_#34a87822]"></span> LOCAL CONTROL PLANE</div>
      <h1 id="forge-title" class="flex items-center gap-3 text-2xl font-semibold tracking-[-0.4px]">Work orchestration <Badge variant="secondary" class="border-success/40 bg-success/10 text-[9px] font-extrabold tracking-[0.8px] text-success">REVIEW BEFORE DISPATCH</Badge></h1>
      <p class="mt-[7px] text-[13px] text-muted-foreground">Prepare a case, choose local evidence or provider analysis, and review the result for {status?.target ?? "the selected repository"}.</p>
    </div>
    <Button variant="outline" size="sm" class="min-w-[142px]" onclick={refresh} disabled={loading} aria-label="Refresh local provider readiness">
      <span class={cn("mr-[5px] inline-block", loading && "animate-spin")}>↻</span> Refresh status
    </Button>
  </header>

  {#if error}<div class="mb-3.5 flex items-start justify-between gap-2.5 rounded-[9px] border border-destructive/40 bg-destructive/10 px-[13px] py-2.5 text-xs text-destructive" role="alert"><span>{error}</span><button class="cursor-pointer border-0 bg-transparent p-[2px_6px] text-base leading-none" onclick={() => (error = "")} aria-label="Dismiss">×</button></div>{/if}
  {#if loading}<p class="mt-[7px] text-[13px] text-muted-foreground">Loading forge state…</p>{/if}

  <div class="mb-3.5 grid grid-cols-4 gap-[11px] max-[1060px]:grid-cols-2">
    <article class="flex min-h-[104px] flex-col items-start rounded-[10px] border border-primary bg-gradient-to-br from-brand to-primary px-[15px] py-[13px] text-primary-foreground">
      <span class="text-[10.5px] font-semibold text-[#c5d9e9]">Control center</span><strong class="mt-1 block text-xl tracking-[-0.35px]">cratelab</strong><small class="mt-[1px] text-[10.5px] text-[#c5d9e9]">Local orchestration and run history</small>
    </article>
    <article class="flex min-h-[104px] flex-col items-start rounded-[10px] border border-border bg-card px-[15px] py-[13px]">
      <span class="text-[10.5px] font-semibold text-muted-foreground">Credentialed services</span><strong class="mt-1 block text-xl tracking-[-0.35px]">{loading ? "—" : `${configuredCount} / ${providerCount}`}</strong><small class="mt-[1px] text-[10.5px] text-muted-foreground">Presence only · values stay hidden</small>
    </article>
    <article class="flex min-h-[104px] flex-col items-start rounded-[10px] border border-border bg-card px-[15px] py-[13px]">
      <span class="text-[10.5px] font-semibold text-muted-foreground">Provider actions</span><strong class="mt-1 block text-[17px] font-bold text-destructive">Confirm first</strong><small class="mt-[1px] text-[10.5px] text-muted-foreground">Confirmed runs use bounded fallback routing</small>
    </article>
    <article class="flex min-h-[104px] flex-col items-start rounded-[10px] border border-border bg-card px-[15px] py-[13px]">
      <span class="text-[10.5px] font-semibold text-muted-foreground">Selected repository</span><strong class="mt-1 block text-xl tracking-[-0.35px]">{status?.target ?? "—"}</strong><small class="mt-[1px] text-[10.5px] text-muted-foreground">{status?.pilot.crates.length ?? "—"} crates · batches of {status?.pilot.batch_size ?? 3}</small>
    </article>
  </div>

  <Card.Root class="mb-3.5 gap-0 rounded-[11px] px-[18px] py-4" aria-label="Forge architecture">
    <div class="flex items-center justify-between gap-3">
      <div><span class="text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">HOW A RUN FLOWS</span><h2 class="mt-0.5 text-[15px] text-foreground">One control plane, staged execution</h2></div>
      <span class="text-[10.5px] text-muted-foreground">Secrets stay local · source context goes only to the chosen/eligible provider</span>
    </div>
    <div class="mt-3.5 grid grid-cols-[1fr_24px_1fr_24px_1fr_24px_1fr_24px_1fr] items-center gap-[5px] max-[760px]:grid-cols-1">
      <div class="relative min-h-[69px] rounded-lg border border-border bg-secondary px-2.5 pb-2 pt-3"><span class="absolute right-2 top-1.5 text-[8px] font-extrabold text-muted-foreground">01</span><strong class="block text-[11.5px]">cratelab</strong><small class="mt-[3px] block text-[9.5px] text-muted-foreground">queue · policy · ledger</small></div>
      <span class="text-center text-base text-muted-foreground max-[760px]:rotate-90">→</span>
      <div class="relative min-h-[69px] rounded-lg border border-border bg-secondary px-2.5 pb-2 pt-3"><span class="absolute right-2 top-1.5 text-[8px] font-extrabold text-muted-foreground">02</span><strong class="block text-[11.5px]">Workflow</strong><small class="mt-[3px] block text-[9.5px] text-muted-foreground">case · scope · evidence</small></div>
      <span class="text-center text-base text-muted-foreground max-[760px]:rotate-90">→</span>
      <div class="relative min-h-[69px] rounded-lg border border-dashed border-border bg-muted px-2.5 pb-2 pt-3 text-muted-foreground"><span class="absolute right-2 top-1.5 text-[8px] font-extrabold">03</span><strong class="block text-[11.5px]">Provider ladder</strong><small class="mt-[3px] block text-[9.5px]">confirmed · capped · logged</small></div>
      <span class="text-center text-base text-muted-foreground max-[760px]:rotate-90">→</span>
      <div class="relative min-h-[69px] rounded-lg border border-dashed border-border bg-muted px-2.5 pb-2 pt-3 text-muted-foreground"><span class="absolute right-2 top-1.5 text-[8px] font-extrabold">04</span><strong class="block text-[11.5px]">Knowledge context</strong><small class="mt-[3px] block text-[9.5px]">HF · Modal · optional docs</small></div>
      <span class="text-center text-base text-muted-foreground max-[760px]:rotate-90">→</span>
      <div class="relative min-h-[69px] rounded-lg border border-border bg-secondary px-2.5 pb-2 pt-3"><span class="absolute right-2 top-1.5 text-[8px] font-extrabold text-muted-foreground">05</span><strong class="block text-[11.5px]">Human review</strong><small class="mt-[3px] block text-[9.5px] text-muted-foreground">local report · you decide</small></div>
    </div>
    <div class="mt-3 flex flex-wrap gap-x-3.5 gap-y-[7px] border-t border-border pt-2.5 text-[10px] text-muted-foreground"><span class="-mr-2.5 font-extrabold text-success">✓</span> Read-only target scan <span class="-mr-2.5 font-extrabold text-success">✓</span> Confirmation before dispatch <span class="-mr-2.5 font-extrabold text-success">✓</span> Daily local caps <span class="-mr-2.5 font-extrabold text-success">✓</span> No target or GitHub writes</div>
  </Card.Root>

  <Card.Root class="mb-3.5 gap-0 rounded-[11px] px-[17px] py-[15px]">
    <div class="mb-[13px] flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">DISPATCH POLICY</span><h2 class="mt-0.5 text-[15px] text-foreground">Task classes decide the route</h2></div><span class="text-[10.5px] text-muted-foreground">Class B audit is enabled; fallback uses configured, probed providers</span></div>
    <div class="grid grid-cols-[repeat(5,minmax(130px,1fr))] gap-2 max-[760px]:grid-cols-[repeat(2,minmax(0,1fr))]">
      <article class="grid min-h-[65px] grid-cols-[27px_minmax(0,1fr)] items-center gap-[7px] rounded-lg border border-primary/40 bg-accent p-[9px] shadow-[inset_0_0_0_1px_#d4e8f5]"><span class="inline-grid h-[25px] w-[25px] place-items-center rounded-md bg-primary/15 text-[11px] font-extrabold text-primary">B</span><div><strong class="block truncate text-[9.5px] text-foreground">Documentation</strong><small class="mt-0.5 block text-[8px] leading-[1.3] text-muted-foreground">Docs-only patch proposal · current test</small></div><span class="col-span-2 truncate text-[8px] text-muted-foreground">Gemini / Bonsai → Groq</span></article>
      <article class="grid min-h-[65px] grid-cols-[27px_minmax(0,1fr)] items-center gap-[7px] rounded-lg border border-border bg-card p-[9px]"><span class="inline-grid h-[25px] w-[25px] place-items-center rounded-md bg-secondary text-[11px] font-extrabold text-muted-foreground">A</span><div><strong class="block truncate text-[9.5px] text-foreground">Cleanup</strong><small class="mt-0.5 block text-[8px] leading-[1.3] text-muted-foreground">Formatting and lint-only changes</small></div><span class="col-span-2 truncate text-[8px] text-muted-foreground">Bonsai → Gemini → Groq</span></article>
      <article class="grid min-h-[65px] grid-cols-[27px_minmax(0,1fr)] items-center gap-[7px] rounded-lg border border-border bg-card p-[9px]"><span class="inline-grid h-[25px] w-[25px] place-items-center rounded-md bg-secondary text-[11px] font-extrabold text-muted-foreground">C</span><div><strong class="block truncate text-[9.5px] text-foreground">Research</strong><small class="mt-0.5 block text-[8px] leading-[1.3] text-muted-foreground">Analysis and ecosystem comparisons</small></div><span class="col-span-2 truncate text-[8px] text-muted-foreground">Gemini → NIM → Compound</span></article>
      <article class="grid min-h-[65px] grid-cols-[27px_minmax(0,1fr)] items-center gap-[7px] rounded-lg border border-border bg-card p-[9px]"><span class="inline-grid h-[25px] w-[25px] place-items-center rounded-md bg-secondary text-[11px] font-extrabold text-muted-foreground">D</span><div><strong class="block truncate text-[9.5px] text-foreground">Knowledge</strong><small class="mt-0.5 block text-[8px] leading-[1.3] text-muted-foreground">Digests and low-risk summaries</small></div><span class="col-span-2 truncate text-[8px] text-muted-foreground">Flash Lite → Groq → Zhipu</span></article>
      <article class="grid min-h-[65px] grid-cols-[27px_minmax(0,1fr)] items-center gap-[7px] rounded-lg border border-border bg-card p-[9px]"><span class="inline-grid h-[25px] w-[25px] place-items-center rounded-md bg-secondary text-[11px] font-extrabold text-muted-foreground">E</span><div><strong class="block truncate text-[9.5px] text-foreground">Release prep</strong><small class="mt-0.5 block text-[8px] leading-[1.3] text-muted-foreground">Changelog and compatibility review</small></div><span class="col-span-2 truncate text-[8px] text-muted-foreground">Bonsai → Groq → Gemini</span></article>
    </div>
    <div class="mt-[9px] text-[9px] text-muted-foreground">Routes are architecture defaults from the plan, not verified live availability. Provider selection and route execution are separate controls.</div>
  </Card.Root>

  <div class="mb-3.5 grid grid-cols-[minmax(290px,0.9fr)_minmax(400px,1.25fr)] gap-3.5 max-[1060px]:grid-cols-1">
    <Card.Root class="gap-0 rounded-[11px] px-[17px] py-[15px]">
      <div class="mb-[13px] flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">REPOSITORY</span><h2 class="mt-0.5 text-[15px] text-foreground">{status?.target ?? "Selected source"}</h2></div><Badge variant="secondary" class="min-w-[25px] justify-center text-[11px] font-bold">{status?.pilot.crates.length ?? 0} crates</Badge></div>
      <article class="mt-2.5 rounded-[9px] border border-border bg-secondary px-[13px] py-3">
        <div class="flex items-center justify-between"><span class="inline-grid h-[27px] w-[27px] place-items-center rounded-md bg-primary/10 text-[9px] font-extrabold text-primary">CL</span><span class="text-[8px] font-extrabold tracking-[1px] text-primary">CONTROL CENTER</span></div>
        <h3 class="mb-0.5 mt-2 text-[13px]">cratelab</h3><p class="m-0 min-h-[35px] text-[10.5px] leading-[1.45] text-muted-foreground">Owns the router, local settings, run history, safety rules, and this UI.</p>
        <div class="mt-2.5 flex items-center justify-between border-t border-border pt-2 text-[9.5px] text-muted-foreground"><span>Self-modification</span><b class="text-muted-foreground">Off for this test</b></div>
      </article>
      <article class="mt-2.5 rounded-[9px] border border-border bg-card px-[13px] py-3">
        <div class="flex items-center justify-between"><span class="inline-grid h-[27px] w-[27px] place-items-center rounded-md bg-success/10 text-[9px] font-extrabold text-success">RS</span><span class="text-[8px] font-extrabold tracking-[1px] text-muted-foreground">RUST LIBRARY</span></div>
        <h3 class="mb-0.5 mt-2 text-[13px]">{status?.target ?? "Rust workspace"}</h3><p class="m-0 min-h-[35px] text-[10.5px] leading-[1.45] text-muted-foreground">{status?.target_path}. Repository selection is shared with the Ops Dashboard; source scans are read-only.</p>
        <div class="mt-2.5 flex items-center justify-between border-t border-border pt-2 text-[9.5px] text-muted-foreground"><span>Workspace source</span><b class={cn("font-bold", status?.target_available ? "text-success" : status ? "text-destructive" : "text-muted-foreground")}>{status?.target_available ? `${status?.pilot.crates.length} crates available` : loading ? "Checking…" : "Path unavailable"}</b></div>
      </article>
      <div class="mt-2.5 flex flex-col gap-0.5 border-l-[3px] border-primary/40 bg-secondary px-[11px] py-2.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Read-only target</strong><span>Only the selected crate source enters a case. Local preparation writes reports under Forge; it never edits this repository.</span></div>
    </Card.Root>

    <Card.Root class="gap-0 rounded-[11px] px-[17px] py-[15px]">
      <div class="mb-[13px] flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">CASE WORKBENCH</span><h2 class="mt-0.5 text-[15px] text-foreground">Build a case packet</h2></div><span class="inline-flex items-center rounded-full border border-success/40 bg-success/10 px-2.5 py-1 text-[9px] font-extrabold tracking-[0.8px] text-success">DRY RUN DEFAULT</span></div>

      <div class="grid grid-cols-[repeat(auto-fit,minmax(150px,1fr))] gap-2.5">
        <div class="grid min-w-0 gap-1">
          <span class="text-[10.5px] font-bold text-muted-foreground">Repository</span>
          <b class="font-mono text-xs text-foreground">{status?.target ?? "—"}</b>
          <small class="text-[9px] text-muted-foreground">switch scope in the top header</small>
        </div>
        <div class="grid min-w-0 gap-1">
          <span class="text-[10.5px] font-bold text-muted-foreground">Crate</span>
          <Select.Root type="single" bind:value={crateId} disabled={busy || liveBusy || loading}>
            <Select.Trigger id="forge-crate" size="sm" class="w-full" />
            <Select.Content>
              {#each status?.pilot.crates ?? [] as crate (crate.id)}<Select.Item value={crate.id} label={crate.package}>{crate.package}</Select.Item>{/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="grid min-w-0 gap-1">
          <span class="text-[10.5px] font-bold text-muted-foreground">Task</span>
          <Select.Root type="single" bind:value={task}>
            <Select.Trigger id="forge-task" size="sm" class="w-full" />
            <Select.Content>
              {#each forgeTasks as item (item.id)}
                <Select.Item value={item.id} label={`${item.group} · ${item.label}`}>{item.group} · {item.label}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
        </div>
        <div class="grid min-w-0 gap-1">
          <span class="text-[10.5px] font-bold text-muted-foreground">Workflow (spine)</span>
          <Select.Root type="single" bind:value={workflowId}>
            <Select.Trigger id="forge-workflow" size="sm" class="w-full" />
            <Select.Content>
              {#each engineWorkflows as item (item.id)}
                <Select.Item value={item.id} label={`${item.title} · ${item.domain}`}>{item.title} · {item.domain}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          <small class="text-[9px] text-muted-foreground">authored in the engine registry</small>
        </div>
        <div class="grid min-w-0 gap-1">
          <span class="text-[10.5px] font-bold text-muted-foreground">Provider (rung)</span>
          <Select.Root type="single" value={status?.selected_provider ?? ""} onValueChange={(v) => v && selectProvider(v)} disabled={busy || loading}>
            <Select.Trigger id="forge-provider" size="sm" class="w-full" />
            <Select.Content>
              <Select.Item value="" label="No provider selected">No provider selected</Select.Item>
              {#each (status?.providers ?? []).filter((p) => ["gemini", "groq", "cloudflare", "nvidia", "openrouter", "zhipu", "colab-bonsai", "kaggle-bonsai"].includes(p.id)) as p (p.id)}
                <Select.Item value={p.id} label={`${p.name}${p.credential_ready ? " · credential present" : " · not configured"}`}>{p.name}{p.credential_ready ? " · credential present" : " · not configured"}</Select.Item>
              {/each}
            </Select.Content>
          </Select.Root>
          <small class="text-[9px] text-muted-foreground">always explicit · no silent fallback</small>
        </div>
        {#if selectedInference()?.available_models.length}
          <div class="grid min-w-0 gap-1">
            <span class="text-[10.5px] font-bold text-muted-foreground">Model</span>
            <Select.Root type="single" value={selectedInference()?.selected_model ?? ""} onValueChange={(v) => selectModel(v)} disabled={busy || loading}>
              <Select.Trigger id="forge-model" size="sm" class="w-full" />
              <Select.Content>
                <Select.Item value="" label="Select a discovered model">Select a discovered model</Select.Item>
                {#each selectedInference()?.available_models ?? [] as model (model)}<Select.Item value={model} label={model}>{model}</Select.Item>{/each}
              </Select.Content>
            </Select.Root>
            <small class="text-[9px] text-muted-foreground">served by the rung · from its last probe</small>
          </div>
        {/if}
      </div>

      {#if activeWorkflow}
        <div class="mt-3.5 flex flex-wrap gap-1.5" aria-label="Workflow spine">
          {#each activeWorkflow.stages as stage (stage.id)}
            <span class="inline-flex items-center gap-[5px] rounded-lg border border-border bg-secondary px-[9px] py-1 text-[9.5px] text-muted-foreground">{stage.title}{#if stage.gate}<b class="rounded-full bg-accent px-1.5 py-[1px] text-[8px] font-extrabold uppercase tracking-[0.4px] text-accent-foreground">{stage.gate}</b>{/if}</span>
          {/each}
        </div>
      {/if}

      <div class="mt-3 flex flex-wrap gap-1.5" aria-label="Case steps">
        {#each workflow?.steps ?? [] as step, i (i)}
          <div class={cn("flex items-center gap-1.5 rounded-full border border-border bg-card px-[9px] py-[5px] text-[10px] text-muted-foreground", stepState(i) === "done" && "border-success/40 bg-success/10 text-success", stepState(i) === "active" && "border-primary/40 bg-accent text-primary")}>
            <span class={cn("inline-grid h-4 w-4 place-items-center rounded-full bg-secondary text-[8.5px] font-extrabold", stepState(i) === "done" && "bg-success/20", stepState(i) === "active" && "bg-primary/15")}>{i + 1}</span><span>{step}</span>
          </div>
        {/each}
      </div>
      <p class="mt-1.5 text-center text-[9px] text-muted-foreground">Local evidence is collected before any provider call. A failed context step degrades to local-evidence-only and says so.</p>
      <div class="mt-2 text-[10px] text-muted-foreground"><strong class="text-foreground">{workflow?.local}</strong><ol class="my-1 list-decimal pl-4">{#each workflow?.steps ?? [] as step, i (`${workflow?.id}-${i}`)}<li>{step}</li>{/each}</ol><small>Provider step: {workflow?.live}</small></div>
      <label class="mb-1 mt-2.5 block text-[10.5px] font-bold text-muted-foreground" for="case-context">Case details / issue text (optional)</label>
      <textarea id="case-context" class="w-full resize-y rounded-md border border-input bg-background px-2.5 py-2 font-mono text-[11px] leading-relaxed" bind:value={caseContext} rows="4" maxlength="12000" placeholder="Describe the symptom, feature question, or issue text. Issue links are not fetched automatically."></textarea>
      {#if learningNotes}<div class="mt-2 rounded-md border border-border bg-secondary p-2.5 text-[10px]"><strong>Learning notes carried into this workflow</strong><pre class="mt-1 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-[10px]">{learningNotes}</pre></div>{/if}
      <div class="mt-2.5">
        <label class="mb-1 mt-2.5 block text-[10.5px] font-bold text-muted-foreground" for="learning-note">Capture learning note (local, {status?.target ?? "repo"} / {task})</label>
        <div class="flex items-stretch gap-2">
          <Select.Root type="single" value={noteRating} onValueChange={(v) => (noteRating = v as "useful" | "partial" | "wrong")}>
            <Select.Trigger id="learning-rating" size="sm" class="min-w-[110px]" />
            <Select.Content>
              <Select.Item value="useful" label="useful">useful</Select.Item>
              <Select.Item value="partial" label="partial">partial</Select.Item>
              <Select.Item value="wrong" label="wrong">wrong</Select.Item>
            </Select.Content>
          </Select.Root>
          <input id="learning-note" class="flex-1 rounded-md border border-input bg-background px-2.5 py-2 text-xs" bind:value={noteText} placeholder="What did this workflow get right or wrong? Carried into future cases for this repo + workflow." maxlength="2000" />
          <Button variant="outline" size="sm" onclick={saveLearningNote} disabled={!noteText.trim()}>Save note</Button>
        </div>
        {#if noteSaved}<small class="mt-1 block text-success">✓ saved — included in the next case for this repository + workflow</small>{/if}
      </div>
      <label class="mb-1 mt-2.5 block text-[10.5px] font-bold text-muted-foreground" for="forge-rag-project">Signage docs context (optional)</label>
      <Select.Root type="single" bind:value={ragProjectId} disabled={liveBusy}>
        <Select.Trigger id="forge-rag-project" size="sm" class="w-full" />
        <Select.Content>
          <Select.Item value="" label="No signage docs">No signage docs</Select.Item>
          <Select.Item value="simple-signage" label="Simple-Signage docs">Simple-Signage docs</Select.Item>
          <Select.Item value="companion-app" label="Companion-app docs">Companion-app docs</Select.Item>
        </Select.Content>
      </Select.Root>
      <div class="my-2.5 flex items-start gap-[7px] text-[10px] leading-[1.45] text-muted-foreground"><span class="inline-grid h-[15px] w-[15px] flex-none place-items-center rounded-full bg-secondary text-[9px] font-extrabold text-muted-foreground">i</span><span>Selected docs are private project material. After you confirm, up to six matching snippets may be sent to Modal and then the inference provider.</span></div>
      <div class="my-2.5 flex items-start gap-[7px] text-[10px] leading-[1.45] text-muted-foreground"><span class="inline-grid h-[15px] w-[15px] flex-none place-items-center rounded-full bg-secondary text-[9px] font-extrabold text-muted-foreground">i</span><span>Selection is local. Use the rung row’s Probe button to verify credentials; that sends the key only, not crate files.</span></div>
      <Button variant="outline" class="w-full" onclick={stageBatch} disabled={loading || !status?.pilot.crates.length}>Stage next batch of {status?.pilot.batch_size ?? 3}</Button>
      {#if queue.some((item) => (item.repository ?? "tv-rs") === status?.target)}
        <div class="my-2.5 grid gap-1.5 rounded-lg border border-border bg-secondary p-2.5" aria-label="Local crate task queue">
          <strong class="text-[10px] text-foreground">{status?.target} cases · {queue.filter((item) => (item.repository ?? "tv-rs") === status?.target && item.state === "pending").length} pending</strong>
          {#each queue.filter((item) => (item.repository ?? "tv-rs") === status?.target).slice(-12) as item (item.id)}
            <button class={cn("flex justify-between gap-2.5 rounded-[5px] bg-card p-[7px_8px] text-left text-xs text-foreground", item.state === "done" && "cursor-default opacity-55")} onclick={() => selectQueueItem(item)} disabled={item.state === "done"}>
              <span>{status?.pilot.crates.find((crate) => crate.id === item.crate_id)?.package ?? item.crate_id}</span>
              <small class="text-muted-foreground">{item.task} · {item.state}</small>
            </button>
          {/each}
        </div>
      {/if}
      <div class="my-3 flex flex-wrap gap-1.5">
        <span class={cn("rounded-full border border-border bg-card px-2 py-[3px] text-[9.5px] font-bold text-muted-foreground", selectedInference()?.probe_ok === true && "border-success/40 bg-success/10 text-success")}>rung probed ✓</span>
        <span class={cn("rounded-full border border-border bg-card px-2 py-[3px] text-[9.5px] font-bold text-muted-foreground", !!selectedInference()?.selected_model && "border-success/40 bg-success/10 text-success")}>model explicit</span>
        <span class={cn("rounded-full border border-border bg-card px-2 py-[3px] text-[9.5px] font-bold text-muted-foreground", !!lastRun && "border-success/40 bg-success/10 text-success")}>local evidence prepared</span>
      </div>
      <div class="flex items-stretch gap-2">
        <Button class="flex-[1.4]" onclick={runDryRun} disabled={busy || loading || !status?.target_available}>
          {#if busy}<span class="h-[13px] w-[13px] animate-spin rounded-full border-2 border-white/40 border-t-white"></span> Preparing…{:else}<span>▶</span> Dry run{/if}
        </Button>
        <Button variant="outline" class="flex-1" onclick={previewLiveRun} disabled={liveBusy || !selectedInference()?.credential_ready || selectedInference()?.probe_ok !== true || !selectedInference()?.selected_model}>
          {liveBusy ? "Preparing…" : "Run live (review first)"}
        </Button>
      </div>
      <p class="mt-1.5 text-center text-[9px] text-muted-foreground">Dry run is local and free — it prepares evidence only. Run live opens the outbound request for review first and stays on free-tier rungs.</p>
      {#if lastRun || liveAnswer}
        <div class="mt-3 grid grid-cols-2 gap-3 max-[700px]:grid-cols-1">
          <div class="min-w-0 rounded-lg border border-border bg-card p-[11px]">
            <span class="mb-2 block text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">EVIDENCE · LOCAL, BEFORE ANY RESULT</span>
            {#if lastRun}
              <div class="rounded-lg border border-success/40 bg-success/10 p-2.5" role="status">
                <div class="flex items-center gap-[7px] text-[11px] text-success"><span class="inline-grid h-[17px] w-[17px] place-items-center rounded-full bg-success/20">✓</span><strong>Case packet prepared · {lastRun.repository ?? status?.target}</strong><span class="ml-auto text-[9px]">{time(lastRun.completed_at)}</span></div>
                <div class="my-2 flex gap-[15px] text-[10px] text-muted-foreground"><span><b class="text-[13px] text-success">{lastRun.source_files}</b> Rust files</span><span><b class="text-[13px] text-success">{lastRun.doc_candidates}</b> doc candidates</span></div>
                <code class="block break-all text-[9px] text-muted-foreground">{lastRun.report}</code>
              </div>
            {:else}
              <p class="mt-[7px] text-[13px] text-muted-foreground">No local evidence yet — run a dry run first.</p>
            {/if}
          </div>
          <div class="min-w-0 rounded-lg border border-border bg-card p-[11px]">
            <span class="mb-2 block text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">RESULT · PROVIDER, REVIEWED</span>
            {#if liveAnswer}
              <div class="rounded-lg border border-border bg-card p-2.5"><div class="mb-1.5 flex items-center justify-between gap-2 text-[11px]"><strong>Analysis result · {task}</strong><span>Saved locally · review before any follow-up</span></div><pre class="max-h-80 overflow-auto whitespace-pre-wrap rounded-md bg-secondary p-2.5 font-mono text-[10px]">{liveAnswer}</pre></div>
            {:else}
              <p class="mt-[7px] text-[13px] text-muted-foreground">No provider result yet. Dry run first, then Run live with a probed rung.</p>
            {/if}
          </div>
        </div>
      {/if}
      {#if liveError}<div class="mb-3.5 mt-3.5 flex items-start justify-between gap-2.5 rounded-[9px] border border-destructive/40 bg-destructive/10 px-[13px] py-2.5 text-xs text-destructive" role="alert"><span>{liveError}</span><button class="cursor-pointer border-0 bg-transparent p-[2px_6px] text-base leading-none" onclick={() => (liveError = "")} aria-label="Dismiss">×</button></div>{/if}
    </Card.Root>
  </div>

  <Card.Root class="mb-3.5 gap-0 rounded-[11px] px-[17px] py-[15px]">
    <div class="mb-[13px] flex items-start justify-between gap-3 max-[760px]:flex-col">
      <div><span class="text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">SERVICE MAP</span><h2 class="mt-0.5 text-[15px] text-foreground">Providers & infrastructure</h2><p class="mt-1 text-[10px] text-muted-foreground">Credential readiness is local metadata only. Green does not mean the service has been health-checked.</p></div>
      <div class="flex items-center gap-[5px] whitespace-nowrap text-[9px] text-muted-foreground"><span class="h-[7px] w-[7px] rounded-full bg-success"></span>Credential present <span class="ml-[7px] h-[7px] w-[7px] rounded-full bg-muted-foreground"></span>Not configured</div>
    </div>
    <div class="grid grid-cols-2 gap-x-[22px] gap-y-3.5 max-[760px]:grid-cols-1">
      {#each groups as group (group.label)}
        <div>
          <h3 class="mb-1.5 text-[9px] font-extrabold uppercase tracking-[0.8px] text-muted-foreground">{group.label}</h3>
          <div class="flex flex-col">
            {#each group.ids as providerId (providerId)}
              {@const p = provider(providerId)}
              {#if p}
                <div class={cn("grid min-h-12 grid-cols-[28px_minmax(100px,1fr)_auto_auto] items-center gap-2 border-t border-border", p.selected && "bg-gradient-to-r from-secondary to-transparent")}>
                  <span class={cn("inline-grid h-[25px] w-[25px] place-items-center rounded-md bg-secondary text-[8px] font-extrabold text-muted-foreground", p.credential_ready && "bg-success/10 text-success")}>{p.name.slice(0, 2).toUpperCase()}</span>
                  <div class="flex min-w-0 flex-col"><strong class="truncate text-[10.5px] text-foreground">{p.name}</strong><small class="truncate text-[8.5px] text-muted-foreground">{p.probe_ok === true && p.available_models.length ? `${p.available_models.length} model IDs discovered` : p.note}</small></div>
                  <span class={cn("flex items-center gap-1 whitespace-nowrap text-[8px] text-muted-foreground", p.probe_ok === true && "text-success", p.credential_ready && p.probe_ok == null && "text-warning", p.probe_ok === false && "text-destructive")}><i class={cn("h-1.5 w-1.5 rounded-full bg-muted-foreground", p.probe_ok === true && "bg-success", p.credential_ready && p.probe_ok == null && "bg-warning", p.probe_ok === false && "bg-destructive")}></i>{p.state}</span>
                  {#if p.probe_supported && p.credential_ready}
                    <Button variant="outline" size="xs" class="min-w-[47px]" onclick={() => openProbe(p)} disabled={probeBusy}>{p.probe_ok === true ? "Recheck" : "Probe"}</Button>
                  {:else if !p.credential_ready && p.setup_url}
                    <a class="inline-flex min-w-[47px] items-center justify-center rounded-[min(var(--radius-md),10px)] border border-border bg-transparent px-2 py-1 text-[8.5px] font-normal whitespace-nowrap text-primary hover:bg-secondary" href={p.setup_url} target="_blank" rel="noreferrer">Get access ↗</a>
                  {:else}<span class="min-w-[47px]"></span>{/if}
                  <span class="hidden">{p.role}</span>
                </div>
              {/if}
            {/each}
          </div>
        </div>
      {/each}
    </div>
    <div class="mt-3 rounded-md bg-secondary px-[11px] py-2.5 text-[9.5px] text-muted-foreground"><strong class="text-brand">Secrets are not shown here.</strong> Add credentials only to the ignored local .env file. The UI checks whether a value exists; it never displays or logs the value.</div>
  </Card.Root>

  <Card.Root class="mb-3.5 gap-0 rounded-[11px] px-[17px] py-[15px]">
    <div class="mb-[13px] flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">KNOWLEDGE PIPELINE</span><h2 class="mt-0.5 text-[15px] text-foreground">Retrieval-augmented context</h2><p class="mt-1 text-[10px] text-muted-foreground">Docs-only corpus from Simple-Signage and companion-app; private HF sync and Modal search are opt-in.</p></div><span class="inline-flex items-center rounded-full border border-success/40 bg-success/10 px-2.5 py-1 text-[9px] font-extrabold tracking-[0.8px] text-success">{status?.retrieval.enabled ? "CONNECTED" : "LOCAL CORPUS"}</span></div>
    <div class="grid grid-cols-[1fr_20px_1fr_20px_1fr_20px_1fr] items-center gap-[5px] max-[760px]:grid-cols-1">
      <div class="min-h-[75px] rounded-lg border border-border bg-secondary px-[11px] py-2.5"><span class="block text-[8px] font-extrabold tracking-[0.75px] text-muted-foreground">01 · SOURCE</span><strong class="mt-[5px] block text-[10.5px] text-foreground">Signage project docs</strong><small class="mt-[3px] block text-[8.5px] text-muted-foreground">Simple-Signage + companion-app · corpus stays local</small></div><span class="text-center text-muted-foreground max-[760px]:rotate-90">→</span>
      <div class="min-h-[75px] rounded-lg border border-dashed border-border bg-card px-[11px] py-2.5"><span class="block text-[8px] font-extrabold tracking-[0.75px] text-muted-foreground">02 · INDEX</span><strong class="mt-[5px] block text-[10.5px] text-muted-foreground">Local BM25</strong><small class="mt-[3px] block text-[8.5px] text-muted-foreground">Chunked docs with project:// provenance</small></div><span class="text-center text-muted-foreground max-[760px]:rotate-90">→</span>
      <div class="min-h-[75px] rounded-lg border border-dashed border-border bg-card px-[11px] py-2.5"><span class="block text-[8px] font-extrabold tracking-[0.75px] text-muted-foreground">03 · STORE</span><strong class="mt-[5px] block text-[10.5px] text-muted-foreground">Hugging Face</strong><small class="mt-[3px] block text-[8.5px] text-muted-foreground">Private dataset is source of truth</small></div><span class="text-center text-muted-foreground max-[760px]:rotate-90">→</span>
      <div class={cn("min-h-[75px] rounded-lg border border-border bg-secondary px-[11px] py-2.5", !status?.retrieval.enabled && "border-dashed bg-card")}><span class="block text-[8px] font-extrabold tracking-[0.75px] text-muted-foreground">04 · SEARCH</span><strong class="mt-[5px] block text-[10.5px] text-foreground">Modal /search</strong><small class="mt-[3px] block text-[8.5px] text-muted-foreground">{status?.retrieval.enabled ? status.retrieval.destination : "Configure endpoint and token to enable"}</small></div>
    </div>
    <div class="mt-[9px] flex justify-between gap-2.5 text-[9px] text-muted-foreground max-[760px]:flex-col"><span>{status?.retrieval.enabled ? "Retrieval runs only after provider-run confirmation" : "Current mode: direct selected-crate scan"}</span><span>Dataset upload and deployment remain manual</span></div>
  </Card.Root>

  <Card.Root class="mb-3.5 gap-0 rounded-[11px] px-[17px] py-[15px]">
    <div class="mb-[13px] flex items-start justify-between gap-3 max-[760px]:flex-col">
      <div><span class="text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">RESEARCH & DOCS GATES</span><h2 class="mt-0.5 text-[15px] text-foreground">CircleCI operations</h2><p class="mt-1 text-[10px] text-muted-foreground">Read-only account, project, pipeline, and workflow status. Forge does not trigger pipelines or change CircleCI/GitHub settings.</p></div>
      <Button variant="outline" size="sm" onclick={() => (circleCiReview = true)} disabled={circleCiBusy || !provider("circleci")?.credential_ready}>{circleCiBusy ? "Loading…" : circleCi ? "Refresh CI status" : "Review status request"}</Button>
    </div>
    {#if circleCi}
      <div class="my-4 grid grid-cols-4 gap-2.5 max-[800px]:grid-cols-2">
        <article class="grid min-w-0 gap-1.5 rounded-[10px] border border-border bg-card p-3.5"><span class="text-xs text-muted-foreground">Account</span><strong class="break-words text-sm text-foreground">{circleCi.account.name || circleCi.account.login || "Authenticated"}</strong></article>
        <article class="grid min-w-0 gap-1.5 rounded-[10px] border border-border bg-card p-3.5"><span class="text-xs text-muted-foreground">Projects connected</span><strong class="break-words text-sm text-foreground">{circleCi.connected_count} / {circleCi.configured_count}</strong></article>
        <article class="grid min-w-0 gap-1.5 rounded-[10px] border border-border bg-card p-3.5"><span class="text-xs text-muted-foreground">Organizations visible</span><strong class="break-words text-sm text-foreground">{circleCi.organizations.length}</strong></article>
        <article class="grid min-w-0 gap-1.5 rounded-[10px] border border-border bg-card p-3.5"><span class="text-xs text-muted-foreground">Last checked</span><strong class="break-words text-sm text-foreground">{time(circleCi.checked_at)}</strong></article>
      </div>
      {#if !circleCi.setup.project_slugs_configured}
        <div class="mt-3 grid gap-[7px] rounded-md border-l-[3px] border-warning bg-warning/10 px-[15px] py-[13px] text-[13px] text-foreground"><strong>Project list is empty.</strong><span>In Forge Settings, set <code>CIRCLECI_PROJECT_SLUGS</code> to a comma-separated list such as <code>gh/Zakarialabib/tv-rs</code>.</span><a class="w-fit font-bold text-primary" href="https://app.circleci.com/projects" target="_blank" rel="noreferrer">Connect a project in CircleCI ↗</a></div>
      {:else}
        <div class="grid gap-2.5">
          {#each circleCi.projects as project (project.slug)}
            <article class="min-w-0 rounded-[10px] border border-border bg-card p-3.5">
              <div class="flex items-center justify-between gap-3"><div class="grid gap-1"><strong>{project.name}</strong><code class="break-all text-xs text-muted-foreground">{project.slug}</code></div><span class={cn("flex-none rounded-full bg-warning/15 px-2 py-1 text-[11px] font-bold capitalize text-warning", project.connected && "bg-success/15 text-success")}>{project.connected ? project.state : "Not connected / not visible"}</span></div>
              {#if project.error}<p class="mt-2.5 text-xs text-muted-foreground">{project.error}</p>{/if}
              {#if project.connected}
                {#if project.pipelines.length}
                  <div class="mt-3.5 flex flex-wrap items-center justify-start gap-3 border-t border-border pt-3 text-xs text-muted-foreground"><strong class="text-foreground">Latest pipeline #{project.pipelines[0].number ?? "—"}</strong><span>{project.pipelines[0].state || "unknown"}</span><span>{project.pipelines[0].branch || "branch unavailable"}</span><time class="ml-auto">{ciTime(project.pipelines[0].created_at)}</time></div>
                  {#if project.latest_workflows.length}<div class="mt-3.5 grid gap-[7px]"><span class="text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">LATEST PIPELINE WORKFLOWS</span>{#each project.latest_workflows as workflow (workflow.id)}<div class="flex items-center justify-between gap-3 text-xs text-muted-foreground"><strong class="text-foreground">{workflow.name}</strong><span class="font-bold capitalize">{workflow.status}</span><time class="ml-auto">{ciTime(workflow.stopped_at || workflow.created_at)}</time></div>{/each}</div>{/if}
                {:else}<p class="mt-2.5 text-xs text-muted-foreground">No pipelines have been returned for this project.</p>{/if}
              {/if}
            </article>
          {/each}
        </div>
      {/if}
      <div class="mt-3.5 flex flex-wrap items-center gap-[7px] border-t border-border pt-3 text-xs text-muted-foreground"><span class={cn("h-[7px] w-[7px] rounded-full", circleCi.setup.template_present ? "bg-success" : "bg-muted-foreground")}></span><span>Forge gate template {circleCi.setup.template_present ? "ready" : "missing"}: <code>{circleCi.setup.template_path}</code></span><span class={cn("ml-2 h-[7px] w-[7px] rounded-full", circleCi.setup.target_config_present ? "bg-success" : "bg-muted-foreground")}></span><span>Target config {circleCi.setup.target_config_present ? "present" : "not installed"}: <code>{circleCi.setup.target_config_path}</code></span></div>
      {#if !circleCi.setup.target_config_present}<p class="mt-3 grid gap-[7px] rounded-md border-l-[3px] border-warning bg-warning/10 px-[15px] py-[13px] text-[13px] text-foreground"><strong>Setup next:</strong> connect the selected repository’s project in CircleCI, review the Forge Rust gate template, then add it to that target through your normal Git review.</p>{/if}
    {:else}
      <div class="flex min-h-[65px] items-center gap-[11px] rounded-lg border border-dashed border-border bg-card p-2.5"><span class="text-xl text-muted-foreground">CI</span><div><strong class="text-[11px] text-foreground">CI status has not been loaded</strong><p class="mt-0.5 text-[9.5px] text-muted-foreground">Configure a CircleCI token and project slug in Settings, then review the read-only request.</p></div></div>
    {/if}
      {#if circleCiError}<div class="mb-3.5 mt-3.5 flex items-start justify-between gap-2.5 rounded-[9px] border border-destructive/40 bg-destructive/10 px-[13px] py-2.5 text-xs text-destructive" role="alert"><span>{circleCiError}</span><button class="cursor-pointer border-0 bg-transparent p-[2px_6px] text-base leading-none" onclick={() => (circleCiError = "")} aria-label="Dismiss">×</button></div>{/if}
    <div class="mt-3 rounded-md bg-secondary px-[11px] py-2.5 text-[9.5px] text-muted-foreground"><strong class="text-brand">External action boundary.</strong> Loading sends the CircleCI token to CircleCI API v2 and reads up to five recent pipeline rows per configured project. It does not start, cancel, or retry jobs.</div>
  </Card.Root>

  <Card.Root class="mb-3.5 gap-0 rounded-[11px] px-[17px] py-[15px]">
    <div class="mb-[13px] flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">LOCAL LEDGER</span><h2 class="mt-0.5 text-[15px] text-foreground">Recent runs</h2></div><Badge variant="secondary" class="min-w-[25px] justify-center text-[11px] font-bold">{status?.runs.length ?? 0}</Badge></div>
    {#if status?.runs.length}
      <div class="overflow-x-auto"><Table.Root class="text-[10px]"><Table.Header><Table.Row><Table.Head>Task</Table.Head><Table.Head>Target</Table.Head><Table.Head>Mode</Table.Head><Table.Head>Source files</Table.Head><Table.Head>Doc candidates</Table.Head><Table.Head>Time</Table.Head></Table.Row></Table.Header><Table.Body>
        {#each status.runs as run, i (`${run.completed_at}-${i}`)}<Table.Row><Table.Cell>{run.task}</Table.Cell><Table.Cell>{run.target}</Table.Cell><Table.Cell><Badge variant="secondary">{run.mode}</Badge></Table.Cell><Table.Cell>{run.source_files}</Table.Cell><Table.Cell>{run.doc_candidates}</Table.Cell><Table.Cell>{time(run.completed_at)}</Table.Cell></Table.Row>{/each}
      </Table.Body></Table.Root></div>
    {:else}
      <div class="flex min-h-[65px] items-center gap-[11px] rounded-lg border border-dashed border-border bg-card p-2.5"><span class="text-xl text-muted-foreground">◷</span><div><strong class="text-[11px] text-foreground">No runs recorded yet</strong><p class="mt-0.5 text-[9.5px] text-muted-foreground">Run the local dry audit to create the first entry in this machine's ledger.</p></div></div>
    {/if}
  </Card.Root>

  <footer class="mt-3 flex justify-between gap-3 text-[9px] text-muted-foreground max-[760px]:flex-col"><span>cratelab · forge module</span><span>Inference requires per-run confirmation. RAG uploads and CI/GitHub writes remain off.</span></footer>

  {#if probeTarget}
    <Dialog.Root open={true} onOpenChange={(o) => { if (!o && !probeBusy) probeTarget = null; }}>
      <Dialog.Content showCloseButton={false} aria-labelledby="probe-title" class="max-w-[520px] gap-0 rounded-xl p-[22px]">
        <span class="text-[9px] font-extrabold uppercase tracking-[1.2px] text-muted-foreground">READ-ONLY CONNECTION CHECK</span>
        <h2 id="probe-title" class="mb-[7px] mt-1.5 text-lg text-foreground">Probe {probeTarget.name}?</h2>
        <p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">The configured credential will be sent in an authenticated request to this provider endpoint. No crate source or prompt content is sent.</p>
        <code class="block break-all rounded-md bg-secondary px-2.5 py-2 font-mono text-[9.5px] text-brand">{probeTarget.probe_destination}</code>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">What is sent</strong><span>{probeTarget.id === "huggingface" ? "HF_TOKEN" : probeTarget.id === "circleci" ? "CIRCLECI_TOKEN" : probeTarget.id === "kaggle" ? "KAGGLE_API_KEY" : probeTarget.id === "cloudflare" ? "CF_API_TOKEN" : probeTarget.id === "qdrant" ? "QDRANT_API_KEY" : probeTarget.id === "modelscope" ? "MODELSCOPE_TOKEN" : probeTarget.id === "openrouter" ? "OPENROUTER_API_KEY" : probeTarget.id === "gemini" ? "GEMINI_API_KEY" : probeTarget.id === "groq" || probeTarget.id === "allam" ? "GROQ_API_KEY" : probeTarget.id === "nvidia" ? "NVIDIA_API_KEY" : probeTarget.id === "zhipu" ? "ZHIPU_API_KEY" : "Configured endpoint credential"} only</span></div>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Stored locally</strong><span>Success/failure status, timestamp, and model IDs only. Response body is discarded.</span></div>
        {#if probeMessage}<div class="mt-3 rounded-md bg-secondary p-2.5 text-[10px] text-muted-foreground" role="status">{probeMessage}</div>{/if}
        <div class="mt-[18px] flex justify-end gap-2"><Button variant="ghost" size="sm" onclick={() => (probeTarget = null)} disabled={probeBusy}>Cancel</Button><Button size="sm" onclick={confirmProbe} disabled={probeBusy}>{probeBusy ? "Probing…" : "Send credential and probe"}</Button></div>
      </Dialog.Content>
    </Dialog.Root>
  {/if}

  {#if livePreview}
    <Dialog.Root open={true} onOpenChange={(o) => { if (!o && !liveBusy) livePreview = null; }}>
      <Dialog.Content showCloseButton={false} aria-labelledby="live-title" class="max-w-[520px] gap-0 rounded-xl p-[22px]">
        <span class="text-[9px] font-extrabold uppercase tracking-[1.2px] text-muted-foreground">OUTBOUND MODEL REQUEST</span>
        <h2 id="live-title" class="mb-[7px] mt-1.5 text-lg text-foreground">Send {livePreview.repository} case to {livePreview.provider}?</h2>
        <p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">This sends public README and Rust source files for {livePreview.target} to the selected provider. Cargo manifests and lockfiles are excluded. Its data handling follows that provider’s terms.</p>
        <code class="block break-all rounded-md bg-secondary px-2.5 py-2 font-mono text-[9.5px] text-brand">{livePreview.destination}</code>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Model</strong><span>{livePreview.model}</span></div>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Workflow and case</strong><span>{task} · {livePreview.case_context || "No extra case details supplied"}</span></div>
        {#if livePreview.learning_notes}<div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Reviewed learning notes sent</strong><span>{livePreview.learning_notes}</span></div>{/if}
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Files and size</strong><span>{livePreview.files.length} files · {Math.ceil(livePreview.source_bytes / 1024)} KB source context · max 1,200 output tokens</span></div>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Eligible route</strong><span>{livePreview.route.map((item) => `${item.provider} (${item.model}, local cap remaining ${item.requests_left_today})`).join(" → ") || "No fallback is currently eligible"}</span></div>
        {#if livePreview.retrieval_enabled && livePreview.retrieval_project}<div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Private docs retrieval</strong><span>After confirmation, query {livePreview.retrieval_destination} for {livePreview.retrieval_project}; up to 6 matching snippets will be attached to this prompt and sent to the selected provider.</span></div>{/if}
        <div class="mt-3 flex flex-wrap gap-[5px] font-mono text-[9px] text-muted-foreground">{#each livePreview.files as file (file)}<span class="rounded bg-secondary px-1.5 py-0.5">{file}</span>{/each}</div>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Routing and cost</strong><span>After confirmation, configured and successfully probed fallback providers may also receive the same source context, within local request caps. Calls may consume quota or incur charges depending on account billing; verify every eligible model is free before continuing.</span></div>
        {#if liveError}<div class="mt-3 rounded-md bg-destructive/10 p-2.5 text-[10px] text-destructive" role="alert">{liveError}</div>{/if}
        <div class="mt-[18px] flex justify-end gap-2"><Button variant="ghost" size="sm" onclick={() => (livePreview = null)} disabled={liveBusy}>Cancel</Button><Button size="sm" onclick={confirmLiveRun} disabled={liveBusy}>{liveBusy ? "Sending…" : `Send source to ${livePreview.provider}`}</Button></div>
      </Dialog.Content>
    </Dialog.Root>
  {/if}

  {#if circleCiReview}
    <Dialog.Root open={true} onOpenChange={(o) => { if (!o && !circleCiBusy) circleCiReview = false; }}>
      <Dialog.Content showCloseButton={false} aria-labelledby="circleci-review-title" class="max-w-[520px] gap-0 rounded-xl p-[22px]">
        <span class="text-[9px] font-extrabold uppercase tracking-[1.2px] text-muted-foreground">READ-ONLY CIRCLECI REQUEST</span>
        <h2 id="circleci-review-title" class="mb-[7px] mt-1.5 text-lg text-foreground">Load CircleCI operations status?</h2>
        <p class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">Forge will send the saved <strong>CIRCLECI_TOKEN</strong> to CircleCI API v2 and request your account, visible organizations, and recent pipeline/workflow status for the configured project slugs.</p>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Project slugs</strong><span>{circleCi?.projects.map((project) => project.slug).join(", ") || "Read from Forge Settings when you continue"}</span></div>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-brand">Scope</strong><span>GET requests only · at most 12 projects · latest 5 pipeline summaries per project · no source upload, job trigger, cancellation, or settings changes.</span></div>
        <div class="mt-[18px] flex justify-end gap-2"><Button variant="ghost" size="sm" onclick={() => (circleCiReview = false)} disabled={circleCiBusy}>Cancel</Button><Button size="sm" onclick={loadCircleCi} disabled={circleCiBusy}>{circleCiBusy ? "Reading CircleCI…" : "Send token and load status"}</Button></div>
      </Dialog.Content>
    </Dialog.Root>
  {/if}
</section>
