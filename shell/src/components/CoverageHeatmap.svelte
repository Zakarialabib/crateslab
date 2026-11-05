<script lang="ts">
  import type { HeatRow } from "../types";
  import * as Table from "$lib/components/ui/table/index.js";
  import { cn } from "$lib/utils.js";

  // Manifest surface vs exercised cells. A refusal colours amber *with
  // respect*, not alarm: it proves the "refusals are part of the API"
  // invariant holds at that cell.
  let { percent, rows }: { percent: number; rows: HeatRow[] } = $props();

  // The status dot is an honest traffic light; uncovered is deliberately mute.
  const dotClass = (status: string): string => {
    switch (status) {
      case "pass":
      case "partial-p":
        return "bg-success";
      case "partial":
        return "bg-warning";
      case "failed":
        return "bg-destructive";
      default:
        return "bg-border";
    }
  };
</script>

<h2 class="mb-2 text-[15px] font-semibold">Coverage heatmap</h2>
<p class="mb-3 text-[12.5px] text-muted-foreground">
  manifest surface vs exercised (workspace, command, arg-bucket) cells — {percent.toFixed(1)}% of declared
  commands covered
</p>
<div class="overflow-hidden rounded-lg border border-border bg-card">
  <Table.Root class="text-[11px]">
    <Table.Header>
      <Table.Row>
        <Table.Head>Status</Table.Head><Table.Head>Command</Table.Head><Table.Head>Label</Table.Head><Table.Head>Danger</Table.Head>
        <Table.Head>Buckets</Table.Head><Table.Head>Runs</Table.Head><Table.Head>Pass</Table.Head><Table.Head>Refused</Table.Head><Table.Head>Timeout</Table.Head><Table.Head>RateLim</Table.Head><Table.Head>Failed</Table.Head>
      </Table.Row>
    </Table.Header>
    <Table.Body>
      {#each rows as r (r.command)}
        <Table.Row>
          <Table.Cell>
            <span class={cn("inline-block size-2.5 rounded-full", dotClass(r.status))}></span>
          </Table.Cell>
          <Table.Cell><code class="font-mono text-[11px]">{r.command}</code></Table.Cell>
          <Table.Cell>{r.label}</Table.Cell>
          <Table.Cell class="text-muted-foreground">{r.danger}</Table.Cell>
          <Table.Cell>{r.buckets_covered}</Table.Cell>
          <Table.Cell>{r.runs}</Table.Cell>
          <Table.Cell>{r.pass}</Table.Cell>
          <Table.Cell>{r.refused}</Table.Cell>
          <Table.Cell>{r.timeout}</Table.Cell>
          <Table.Cell>{r.rate_limited}</Table.Cell>
          <Table.Cell>{r.failed}</Table.Cell>
        </Table.Row>
      {/each}
    </Table.Body>
  </Table.Root>
</div>
