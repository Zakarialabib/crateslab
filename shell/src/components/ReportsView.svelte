<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { ForgeReport, ForgeReportContent } from "../types";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import * as Card from "$lib/components/ui/card";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import * as ScrollArea from "$lib/components/ui/scroll-area";
  import { Separator } from "$lib/components/ui/separator";
  import { cn } from "$lib/utils";

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

<section class="mx-auto max-w-[1500px] pb-7">
  <header class="mb-5 flex items-end justify-between gap-5 max-[760px]:flex-col max-[760px]:items-start">
    <div>
      <div class="mb-[7px] flex items-center gap-[7px] text-[10px] font-extrabold tracking-[1.45px] text-[#63809a]"><span class="size-[7px] shrink-0 rounded-full bg-[#34a878] shadow-[0_0_0_3px_#34a87822]"></span> EVIDENCE · EVERY CASE LEAVES A REPORT</div>
      <h1 class="m-0 mb-1 flex items-center gap-3 text-[25px] font-semibold tracking-[-0.4px]">Reports <Badge variant="secondary" class="border-success text-[9px] font-extrabold tracking-[0.8px] text-success">{reports.length}</Badge></h1>
      <p class="mt-[7px] text-[13px] text-muted-foreground">Case packets and provider audits, as written to <code>forge/reports</code>. Nothing here is paraphrased — the viewer shows the file bytes.</p>
    </div>
    <Button variant="outline" size="sm" onclick={load} disabled={loading}>{loading ? "Loading…" : "Refresh"}</Button>
  </header>

  {#if err}<div class="mb-3.5 flex flex-col items-start justify-between gap-2.5 rounded-[9px] border border-destructive/40 bg-destructive/10 px-[13px] py-[10px] text-xs text-destructive" role="alert">{err}</div>{/if}

  <Label class="mb-1 mt-2.5 block text-[10.5px] font-bold text-muted-foreground" for="report-filter">Filter</Label>
  <Input id="report-filter" bind:value={filter} placeholder="repo, crate, task, kind…" />

  {#if !loading && visible.length === 0}
    <p class="mt-3.5 text-[13px] text-muted-foreground">No reports match. Reports appear here the moment a workflow prepares a case packet or a provider audit completes.</p>
  {/if}

  <div class="mt-3.5 grid grid-cols-[minmax(300px,380px)_1fr] items-start gap-3.5 max-[900px]:grid-cols-1">
    <ScrollArea.Root class="max-h-[70vh]">
      <div class="flex flex-col gap-1.5 pr-3">
        {#each visible as row (row.name)}
          <button class={cn("flex flex-col gap-[5px] rounded-lg border border-border bg-card px-[11px] py-[9px] text-left transition-colors hover:border-primary", selected?.name === row.name && "ring-2 ring-primary")} onclick={() => open(row)}>
            <span class="text-xs font-semibold [word-break:break-all]">{row.name}</span>
            <span class="flex flex-wrap items-center gap-[5px]">
              {#if row.repository}<Badge variant="outline">{row.repository}</Badge>{/if}
              {#if row.task}<Badge variant="outline">{row.task}</Badge>{/if}
              <Badge variant="outline">{row.kind}</Badge>
              <small class="text-[10.5px] text-muted-foreground">{time(row.modified)} · {size(row.size)}</small>
            </span>
          </button>
        {/each}
      </div>
    </ScrollArea.Root>

    {#if selected}
      <Card.Root class="flex max-h-[70vh] flex-col gap-0 overflow-hidden py-0">
        <Card.Header class="flex flex-row items-center justify-between gap-2.5 px-3 py-2.5">
          <div><strong>{selected.name}</strong><small class="block text-[10.5px] text-muted-foreground [word-break:break-all]">{selected.path}</small></div>
          <Button variant="outline" size="sm" onclick={reveal} disabled={opening}>
            {opening ? "Opening…" : "Reveal in Explorer"}
          </Button>
        </Card.Header>
        <Separator />
        <ScrollArea.Root class="min-h-0 flex-1">
          <pre class="m-0 whitespace-pre-wrap px-[14px] py-[12px] text-xs leading-[1.55]">{selected.content}</pre>
        </ScrollArea.Root>
      </Card.Root>
    {:else}
      <Card.Root class="flex min-h-[200px] items-center justify-center">
        <p class="mt-[7px] text-[13px] text-muted-foreground">Select a report to read it here.</p>
      </Card.Root>
    {/if}
  </div>
</section>
