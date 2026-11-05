<script lang="ts">
  import { api } from "../api";
  import type { HeatRow } from "../types";
  import CoverageHeatmap from "./CoverageHeatmap.svelte";

  let { workspace }: { workspace: string } = $props();
  let data = $state<{ coverage_percent: number; rows: HeatRow[] } | null>(null);

  $effect(() => {
    const ws = workspace;
    data = null;
    api
      .coverage(ws)
      .then((d) => (data = d))
      .catch(() => (data = { coverage_percent: 0, rows: [] }));
  });
</script>

{#if !data}
  <p class="text-[12.5px] text-muted-foreground">loading ledger…</p>
{:else}
  <CoverageHeatmap percent={data.coverage_percent} rows={data.rows} />
{/if}
