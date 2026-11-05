<script lang="ts">
  import type { ScenarioReport } from "../types";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { cn } from "$lib/utils.js";

  // The scenario timeline — exactly the playlist steps, now with outcomes.
  let { report }: { report: ScenarioReport } = $props();
</script>

<h2 class="mt-0 mb-3 flex flex-wrap items-center gap-2 text-[15px] font-semibold">
  {report.name}
  <Badge
    variant={report.ok ? "secondary" : "destructive"}
    class={cn(
      "text-[9px] font-semibold tracking-wide",
      report.ok && "border-success/40 bg-success/10 text-success",
    )}
  >
    {report.ok ? "PASS" : "FAIL"} · {report.duration_ms} ms
  </Badge>
</h2>
<div class="flex flex-col gap-2">
  {#each report.steps as s, i (i)}
    <div
      class={cn(
        "flex items-baseline gap-3 rounded-lg border border-l-4 border-border bg-card px-3.5 py-2.5",
        s.status === "pass" ? "border-l-success" : "border-l-destructive",
      )}
    >
      <span class="min-w-[60px] text-[11px] text-muted-foreground">{s.duration_ms} ms</span>
      <div>
        <strong class="text-[13px]">{s.name}</strong>
        {#if s.command}<code class="ml-2 font-mono text-[12px] text-muted-foreground">{s.command}</code>{/if}
        <div class={cn("text-[12.5px]", s.status === "pass" ? "text-success" : "text-destructive")}>
          {s.status}{s.outcome ? ` · ${s.outcome}` : ""}{s.detail ? ` — ${s.detail}` : ""}
        </div>
      </div>
    </div>
  {/each}
</div>
