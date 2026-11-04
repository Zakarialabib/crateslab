<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { ForgeDashboardData, ForgeStatus } from "../types";

  let { onnav }: { onnav?: (view: string) => void } = $props();

  let data = $state<ForgeDashboardData | null>(null);
  let status = $state<ForgeStatus | null>(null);
  let loading = $state(true);
  let err = $state("");

  onMount(async () => {
    try {
      const [nextData, nextStatus] = await Promise.all([api.forgeDashboard(), api.forgeStatus()]);
      data = nextData;
      status = nextStatus;
    } catch (e) { err = String(e); }
    finally { loading = false; }
  });

  function time(ts: number): string { return new Date(ts * 1000).toLocaleString(); }
  function shortDay(day: string): string { return day.slice(5); }
  const maxDay = $derived(data ? Math.max(1, ...Object.values(data.runs.by_day)) : 1);
  const days = $derived(data ? Object.entries(data.runs.by_day).slice(-14) : []);
  const tasks = $derived(data ? Object.entries(data.runs.by_task).sort((a, b) => b[1] - a[1]) : []);
  const maxTask = $derived(Math.max(1, ...tasks.map(([, count]) => count)));

  // Budgets lead: every row is measured against the cap the engine reports.
  // Nothing here is invented — unknown caps render as "—", not as a guess.
  const budgets = $derived.by(() => {
    if (!status) return [] as { id: string; used: number; cap: number | null; pct: number }[];
    const used = status.usage?.requests ?? {};
    const caps = status.daily_caps ?? {};
    const ids = Array.from(new Set([...Object.keys(caps), ...Object.keys(used)])).sort();
    return ids.map((id) => {
      const cap = caps[id] ?? null;
      const count = used[id] ?? 0;
      return { id, used: count, cap, pct: cap && cap > 0 ? Math.min(100, Math.round((count / cap) * 100)) : 0 };
    });
  });
  const probed = $derived(status?.providers.filter((p) => p.probe_ok === true).length ?? 0);

  // Recent cases prefer the ledger rows (they carry provider + model); the
  // dashboard's own recent list is the fallback when the ledger is empty.
  const recentCases = $derived.by(() => {
    const ledger = (status?.runs ?? []).map((run) => ({
      at: run.completed_at, task: run.task, target: run.target, mode: run.mode,
      provider: run.provider, model: run.model,
    }));
    if (ledger.length) return ledger.slice(0, 8);
    return (data?.runs.recent ?? []).map((run) => ({
      at: run.completed_at, task: run.task, target: run.target ?? run.crate_id, mode: run.mode,
      provider: undefined, model: undefined,
    }));
  });
</script>

<section class="ops-dashboard">
  <header class="forge-header">
    <div>
      <div class="eyebrow"><span class="eyebrow-dot"></span> CRATELAB · PROJECT STATE &amp; EVOLUTION</div>
      <h1>Dashboard</h1>
      <p class="forge-lede">Is the forge improving the {data?.target ?? "selected"} crates, and what needs a human? Every number below links to the surface that owns it.</p>
    </div>
    <button class="ghost" onclick={() => onnav?.("reports")}>Open reports</button>
  </header>

  {#if err}<div class="forge-alert" role="alert">{err}</div>{/if}
  {#if loading}<p class="forge-lede">Loading project state…</p>{/if}

  {#if data}
    <div class="forge-metrics">
      <button class="target-card metric-link" onclick={() => onnav?.("reports")}>
        <span class="metric-label">Evidence runs</span><strong>{data.runs.total}</strong>
        <small>{Object.keys(data.runs.by_repository).length} repositor{Object.keys(data.runs.by_repository).length === 1 ? "y" : "ies"} touched · open Reports</small>
      </button>
      <button class="target-card metric-link" onclick={() => onnav?.("reports")}>
        <span class="metric-label">Reports produced</span><strong>{data.reports.count}</strong>
        <small>latest: {data.reports.latest[0]?.name ?? "—"}</small>
      </button>
      <button class="target-card metric-link" onclick={() => onnav?.("forge-settings")}>
        <span class="metric-label">Providers ready</span><strong>{probed}/{status?.providers.length ?? data.providers.total} probed</strong>
        <small>{data.providers.ready}/{data.providers.total} configured · open Providers</small>
      </button>
      <button class="target-card metric-link" onclick={() => onnav?.("forge")}>
        <span class="metric-label">Target repository</span><strong>{data.target}</strong>
        <small>{data.crates.length} crates in the catalog · open Workflows</small>
      </button>
    </div>

    <div class="ops-actions-grid">
      <article class="ops-action-card">
        <span class="section-kicker">BUDGETS TODAY · FREE TIERS</span>
        <h3>What is left today</h3>
        {#if budgets.length === 0}
          <p class="forge-lede">No usage recorded yet. Local caps come from the engine's ledger.</p>
        {:else}
          <div class="evo-bars">
            {#each budgets as row (row.id)}
              <div class="evo-row">
                <span class="evo-day">{row.id}</span>
                <div class="evo-bar-track"><div class="evo-bar" class:evo-bar-warn={row.pct >= 70} style="width:{row.pct}%"></div></div>
                <span class="evo-count">{row.used}{row.cap ? ` / ${row.cap}` : ""}</span>
              </div>
            {/each}
          </div>
        {/if}
        <div class="invariant-row"><span>spend</span><strong>€0.00 (always)</strong><small>free-tier rungs only · no paid endpoint is wired</small></div>
      </article>

      <article class="ops-action-card">
        <span class="section-kicker">EVOLUTION · RUNS PER DAY (UTC)</span>
        <h3>Evidence prepared over time</h3>
        {#if days.length === 0}
          <p class="forge-lede">No runs yet — start with a local case packet in Workflows.</p>
        {:else}
          <div class="evo-bars">
            {#each days as [day, count] (day)}
              <div class="evo-row"><span class="evo-day">{shortDay(day)}</span><div class="evo-bar-track"><div class="evo-bar" style="width:{Math.round((count / maxDay) * 100)}%"></div></div><span class="evo-count">{count}</span></div>
            {/each}
          </div>
        {/if}
      </article>
    </div>

    <div class="ops-actions-grid">
      <article class="ops-action-card">
        <span class="section-kicker">WORKLOAD MIX (TASK)</span>
        <h3>Where the effort goes</h3>
        {#if tasks.length === 0}
          <p class="forge-lede">No workflow runs recorded yet.</p>
        {:else}
          <div class="evo-bars">
            {#each tasks as [task, count] (task)}
              <div class="evo-row"><span class="evo-day">{task}</span><div class="evo-bar-track"><div class="evo-bar" style="width:{Math.round((count / maxTask) * 100)}%"></div></div><span class="evo-count">{count}</span></div>
            {/each}
          </div>
        {/if}
        <small class="button-caption">Every run is one step toward better crates: inventory → evidence → provider analysis → review.</small>
      </article>

      <article class="ops-action-card">
        <span class="section-kicker">COVERAGE · LAB ENGINE</span>
        <h3>Command-surface coverage</h3>
        {#each data.coverage as row (row.workspace)}
          <div class="evo-row"><span class="evo-day">{row.workspace}</span><div class="evo-bar-track"><div class="evo-bar" style="width:{Math.round(row.percent)}%"></div></div><span class="evo-count">{row.percent}%</span></div>
        {:else}
          <p class="forge-lede">No lab coverage yet.</p>
        {/each}
        <small class="button-caption">Coverage moves when scenarios exercise more of each manifest — Lab · Scenarios.</small>
      </article>
    </div>

    <article class="ops-action-card">
      <span class="section-kicker">RECENT CASES</span>
      <h3>Last cases, with provenance</h3>
      {#if recentCases.length === 0}
        <p class="forge-lede">Nothing yet. Prepare a case packet to see it here.</p>
      {:else}
        <ul class="evo-recent">
          {#each recentCases as run (run.at)}
            <li>
              <span class="t">{time(run.at)}</span>
              <span><strong>{run.task}</strong> · {run.target} <em>({run.mode})</em>{#if run.provider} · {run.provider}{/if}{#if run.model} · {run.model}{/if}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </article>

    <article class="ops-action-card">
      <span class="section-kicker">PROJECT · CRATE CATALOG ({data.target})</span>
      <h3>What the lab improves</h3>
      <div class="crate-chips">
        {#each data.crates as crate (crate.id)}
          <button class="crate-chip" onclick={() => onnav?.("forge")}>{crate.package}</button>
        {/each}
      </div>
      <div class="ops-button-row" style="margin-top:12px">
        <button class="ghost" onclick={() => onnav?.("forge")}>Run a workflow</button>
        <button class="ghost" onclick={() => onnav?.("reports")}>Read the reports</button>
        <button class="ghost" onclick={() => onnav?.("forge-settings")}>Check providers</button>
      </div>
    </article>
  {/if}
</section>
