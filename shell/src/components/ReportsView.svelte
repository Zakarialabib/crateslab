<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { ForgeReport, ForgeReportContent } from "../types";

  let reports = $state<ForgeReport[]>([]);
  let selected = $state<ForgeReportContent | null>(null);
  let loading = $state(true);
  let opening = $state(false);
  let err = $state("");
  let filter = $state("");

  onMount(load);

  async function load() {
    loading = true;
    err = "";
    try { reports = await api.forgeReports(); }
    catch (e) { err = String(e); }
    finally { loading = false; }
  }

  function time(ts: number): string { return new Date(ts * 1000).toLocaleString(); }
  function size(bytes: number): string { return bytes > 2048 ? `${Math.round(bytes / 1024)} KB` : `${bytes} B`; }

  const visible = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return reports;
    return reports.filter((r) =>
      [r.name, r.kind, r.repository ?? "", r.crate ?? "", r.task ?? ""].join(" ").toLowerCase().includes(q));
  });

  async function open(row: ForgeReport) {
    err = "";
    try { selected = await api.forgeReadReport(row.name); }
    catch (e) { err = String(e); }
  }

  async function reveal() {
    if (!selected) return;
    opening = true;
    err = "";
    try { await api.forgeOpenReport(selected.name); }
    catch (e) { err = String(e); }
    finally { opening = false; }
  }
</script>

<section class="ops-dashboard">
  <header class="forge-header">
    <div>
      <div class="eyebrow"><span class="eyebrow-dot"></span> EVIDENCE · EVERY CASE LEAVES A REPORT</div>
      <h1>Reports <span class="safe-pill">{reports.length}</span></h1>
      <p class="forge-lede">Case packets and provider audits, as written to <code>forge/reports</code>. Nothing here is paraphrased — the viewer shows the file bytes.</p>
    </div>
    <button class="ghost" onclick={load} disabled={loading}>{loading ? "Loading…" : "Refresh"}</button>
  </header>

  {#if err}<div class="forge-alert" role="alert">{err}</div>{/if}

  <label class="field-label" for="report-filter">Filter</label>
  <input id="report-filter" bind:value={filter} placeholder="repo, crate, task, kind…" />

  {#if !loading && visible.length === 0}
    <p class="forge-lede" style="margin-top:14px">No reports match. Reports appear here the moment a workflow prepares a case packet or a provider audit completes.</p>
  {/if}

  <div class="reports-layout">
    <div class="reports-list">
      {#each visible as row (row.name)}
        <button class={"report-row" + (selected?.name === row.name ? " active" : "")} onclick={() => open(row)}>
          <span class="report-name">{row.name}</span>
          <span class="report-meta">
            {#if row.repository}<span class="badge">{row.repository}</span>{/if}
            {#if row.task}<span class="badge">{row.task}</span>{/if}
            <span class="badge">{row.kind}</span>
            <small>{time(row.modified)} · {size(row.size)}</small>
          </span>
        </button>
      {/each}
    </div>

    {#if selected}
      <div class="report-viewer">
        <div class="report-viewer-head">
          <div><strong>{selected.name}</strong><small>{selected.path}</small></div>
          <button class="ghost" onclick={reveal} disabled={opening}>
            {opening ? "Opening…" : "Reveal in Explorer"}
          </button>
        </div>
        <pre>{selected.content}</pre>
      </div>
    {:else}
      <div class="report-viewer empty"><p class="forge-lede">Select a report to read it here.</p></div>
    {/if}
  </div>
</section>
