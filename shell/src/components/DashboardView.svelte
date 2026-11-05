<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { ForgeDashboardData, ForgeStatus } from "../types";
  import { Button } from "$lib/components/ui/button";
  import * as Card from "$lib/components/ui/card";
  import { cn } from "$lib/utils";

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

<section class="mx-auto max-w-[1500px] pb-7">
  <header class="mb-5 flex items-end justify-between gap-5 max-[760px]:flex-col max-[760px]:items-start">
    <div>
      <div class="mb-[7px] flex items-center gap-[7px] text-[10px] font-extrabold tracking-[1.45px] text-[#63809a]"><span class="size-[7px] shrink-0 rounded-full bg-[#34a878] shadow-[0_0_0_3px_#34a87822]"></span> CRATELAB · PROJECT STATE &amp; EVOLUTION</div>
      <h1 class="m-0 mb-1 flex items-center gap-3 text-[25px] font-semibold tracking-[-0.4px]">Dashboard</h1>
      <p class="mt-[7px] text-[13px] text-muted-foreground">Is the forge improving the {data?.target ?? "selected"} crates, and what needs a human? Every number below links to the surface that owns it.</p>
    </div>
    <Button variant="outline" size="sm" onclick={() => onnav?.("reports")}>Open reports</Button>
  </header>

  {#if err}<div class="mb-3.5 flex flex-col items-start justify-between gap-2.5 rounded-[9px] border border-destructive/40 bg-destructive/10 px-[13px] py-[10px] text-xs text-destructive" role="alert">{err}</div>{/if}
  {#if loading}<p class="mt-[7px] text-[13px] text-muted-foreground">Loading project state…</p>{/if}

  {#if data}
    <div class="mb-3.5 grid grid-cols-4 gap-[11px] max-[1060px]:grid-cols-2">
      <button class="mt-[9px] w-full cursor-pointer rounded-[10px] border border-border bg-card px-[13px] py-3 text-left transition-colors [font-family:inherit] hover:border-primary hover:shadow-[0_2px_10px_#2d7ab322]" onclick={() => onnav?.("reports")}>
        <span class="text-[10.5px] [font-weight:650] text-muted-foreground">Evidence runs</span><strong class="mt-1 block text-xl tracking-[-0.35px]">{data.runs.total}</strong>
        <small class="mt-px block text-[10.5px] text-muted-foreground">{Object.keys(data.runs.by_repository).length} repositor{Object.keys(data.runs.by_repository).length === 1 ? "y" : "ies"} touched · open Reports</small>
      </button>
      <button class="mt-[9px] w-full cursor-pointer rounded-[10px] border border-border bg-card px-[13px] py-3 text-left transition-colors [font-family:inherit] hover:border-primary hover:shadow-[0_2px_10px_#2d7ab322]" onclick={() => onnav?.("reports")}>
        <span class="text-[10.5px] [font-weight:650] text-muted-foreground">Reports produced</span><strong class="mt-1 block text-xl tracking-[-0.35px]">{data.reports.count}</strong>
        <small class="mt-px block text-[10.5px] text-muted-foreground">latest: {data.reports.latest[0]?.name ?? "—"}</small>
      </button>
      <button class="mt-[9px] w-full cursor-pointer rounded-[10px] border border-border bg-card px-[13px] py-3 text-left transition-colors [font-family:inherit] hover:border-primary hover:shadow-[0_2px_10px_#2d7ab322]" onclick={() => onnav?.("forge-settings")}>
        <span class="text-[10.5px] [font-weight:650] text-muted-foreground">Providers ready</span><strong class="mt-1 block text-xl tracking-[-0.35px]">{probed}/{status?.providers.length ?? data.providers.total} probed</strong>
        <small class="mt-px block text-[10.5px] text-muted-foreground">{data.providers.ready}/{data.providers.total} configured · open Providers</small>
      </button>
      <button class="mt-[9px] w-full cursor-pointer rounded-[10px] border border-border bg-card px-[13px] py-3 text-left transition-colors [font-family:inherit] hover:border-primary hover:shadow-[0_2px_10px_#2d7ab322]" onclick={() => onnav?.("forge")}>
        <span class="text-[10.5px] [font-weight:650] text-muted-foreground">Target repository</span><strong class="mt-1 block text-xl tracking-[-0.35px]">{data.target}</strong>
        <small class="mt-px block text-[10.5px] text-muted-foreground">{data.crates.length} crates in the catalog · open Workflows</small>
      </button>
    </div>

    <div class="grid grid-cols-3 gap-3 max-[1050px]:grid-cols-2 max-[700px]:grid-cols-1">
      <Card.Root class="min-w-0">
        <Card.Header>
          <span class="block text-[10px] font-extrabold tracking-[1.45px] text-[#63809a]">BUDGETS TODAY · FREE TIERS</span>
          <Card.Title>What is left today</Card.Title>
        </Card.Header>
        <Card.Content>
          {#if budgets.length === 0}
            <p class="mt-[7px] min-h-[38px] text-[12px] text-muted-foreground">No usage recorded yet. Local caps come from the engine's ledger.</p>
          {:else}
            <div class="mt-2">
              {#each budgets as row (row.id)}
                <div class="mb-1.5 flex items-center gap-2.5">
                  <span class="min-w-[74px] text-[11.5px] text-muted-foreground">{row.id}</span>
                  <div class="h-[10px] flex-1 overflow-hidden rounded-[5px] bg-muted"><div class={cn("h-full rounded-[5px] bg-primary", row.pct >= 70 && "bg-warning")} style="width:{row.pct}%"></div></div>
                  <span class="min-w-[34px] text-right text-[11.5px] font-bold">{row.used}{row.cap ? ` / ${row.cap}` : ""}</span>
                </div>
              {/each}
            </div>
          {/if}
          <div class="mt-2.5 flex items-baseline gap-2 border-t border-dashed border-border pt-[9px] text-[10px] text-muted-foreground"><span>spend</span><strong class="text-[12px] text-success">€0.00 (always)</strong><small class="ml-auto">free-tier rungs only · no paid endpoint is wired</small></div>
        </Card.Content>
      </Card.Root>

      <Card.Root class="min-w-0">
        <Card.Header>
          <span class="block text-[10px] font-extrabold tracking-[1.45px] text-[#63809a]">EVOLUTION · RUNS PER DAY (UTC)</span>
          <Card.Title>Evidence prepared over time</Card.Title>
        </Card.Header>
        <Card.Content>
          {#if days.length === 0}
            <p class="mt-[7px] min-h-[38px] text-[12px] text-muted-foreground">No runs yet — start with a local case packet in Workflows.</p>
          {:else}
            <div class="mt-2">
              {#each days as [day, count] (day)}
                <div class="mb-1.5 flex items-center gap-2.5"><span class="min-w-[74px] text-[11.5px] text-muted-foreground">{shortDay(day)}</span><div class="h-[10px] flex-1 overflow-hidden rounded-[5px] bg-muted"><div class="h-full rounded-[5px] bg-primary" style="width:{Math.round((count / maxDay) * 100)}%"></div></div><span class="min-w-[34px] text-right text-[11.5px] font-bold">{count}</span></div>
              {/each}
            </div>
          {/if}
        </Card.Content>
      </Card.Root>
    </div>

    <div class="grid grid-cols-3 gap-3 max-[1050px]:grid-cols-2 max-[700px]:grid-cols-1">
      <Card.Root class="min-w-0">
        <Card.Header>
          <span class="block text-[10px] font-extrabold tracking-[1.45px] text-[#63809a]">WORKLOAD MIX (TASK)</span>
          <Card.Title>Where the effort goes</Card.Title>
        </Card.Header>
        <Card.Content>
          {#if tasks.length === 0}
            <p class="mt-[7px] min-h-[38px] text-[12px] text-muted-foreground">No workflow runs recorded yet.</p>
          {:else}
            <div class="mt-2">
              {#each tasks as [task, count] (task)}
                <div class="mb-1.5 flex items-center gap-2.5"><span class="min-w-[74px] text-[11.5px] text-muted-foreground">{task}</span><div class="h-[10px] flex-1 overflow-hidden rounded-[5px] bg-muted"><div class="h-full rounded-[5px] bg-primary" style="width:{Math.round((count / maxTask) * 100)}%"></div></div><span class="min-w-[34px] text-right text-[11.5px] font-bold">{count}</span></div>
              {/each}
            </div>
          {/if}
          <small class="mt-2.5 block text-center text-[10.5px] text-muted-foreground">Every run is one step toward better crates: inventory → evidence → provider analysis → review.</small>
        </Card.Content>
      </Card.Root>

      <Card.Root class="min-w-0">
        <Card.Header>
          <span class="block text-[10px] font-extrabold tracking-[1.45px] text-[#63809a]">COVERAGE · LAB ENGINE</span>
          <Card.Title>Command-surface coverage</Card.Title>
        </Card.Header>
        <Card.Content>
          {#each data.coverage as row (row.workspace)}
            <div class="mb-1.5 flex items-center gap-2.5"><span class="min-w-[74px] text-[11.5px] text-muted-foreground">{row.workspace}</span><div class="h-[10px] flex-1 overflow-hidden rounded-[5px] bg-muted"><div class="h-full rounded-[5px] bg-primary" style="width:{Math.round(row.percent)}%"></div></div><span class="min-w-[34px] text-right text-[11.5px] font-bold">{row.percent}%</span></div>
          {:else}
            <p class="mt-[7px] min-h-[38px] text-[12px] text-muted-foreground">No lab coverage yet.</p>
          {/each}
          <small class="mt-2.5 block text-center text-[10.5px] text-muted-foreground">Coverage moves when scenarios exercise more of each manifest — Lab · Scenarios.</small>
        </Card.Content>
      </Card.Root>
    </div>

    <Card.Root class="min-w-0">
      <Card.Header>
        <span class="block text-[10px] font-extrabold tracking-[1.45px] text-[#63809a]">RECENT CASES</span>
        <Card.Title>Last cases, with provenance</Card.Title>
      </Card.Header>
      <Card.Content>
        {#if recentCases.length === 0}
          <p class="mt-[7px] min-h-[38px] text-[12px] text-muted-foreground">Nothing yet. Prepare a case packet to see it here.</p>
        {:else}
          <ul class="m-0 mt-2 list-none p-0">
            {#each recentCases as run (run.at)}
              <li class="flex items-baseline gap-2.5 py-1 text-[12.5px]">
                <span class="min-w-[130px] text-[11px] text-muted-foreground">{time(run.at)}</span>
                <span><strong>{run.task}</strong> · {run.target} <em class="text-[11.5px] not-italic text-muted-foreground">({run.mode})</em>{#if run.provider} · {run.provider}{/if}{#if run.model} · {run.model}{/if}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </Card.Content>
    </Card.Root>

    <Card.Root class="min-w-0">
      <Card.Header>
        <span class="block text-[10px] font-extrabold tracking-[1.45px] text-[#63809a]">PROJECT · CRATE CATALOG ({data.target})</span>
        <Card.Title>What the lab improves</Card.Title>
      </Card.Header>
      <Card.Content>
        <div class="mt-2.5 flex flex-wrap gap-1.5">
          {#each data.crates as crate (crate.id)}
            <button class="cursor-pointer rounded-full border border-border bg-card px-[10px] py-[3px] text-[11.5px] [font-family:inherit] hover:border-primary hover:text-primary" onclick={() => onnav?.("forge")}>{crate.package}</button>
          {/each}
        </div>
        <div class="mt-3 flex flex-wrap items-center gap-[9px]">
          <Button variant="outline" size="sm" onclick={() => onnav?.("forge")}>Run a workflow</Button>
          <Button variant="outline" size="sm" onclick={() => onnav?.("reports")}>Read the reports</Button>
          <Button variant="outline" size="sm" onclick={() => onnav?.("forge-settings")}>Check providers</Button>
        </div>
      </Card.Content>
    </Card.Root>
  {/if}
</section>
