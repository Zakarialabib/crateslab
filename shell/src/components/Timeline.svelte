<script lang="ts">
  import type { ScenarioReport } from "../types";

  // The scenario timeline — exactly the playlist steps, now with outcomes.
  let { report }: { report: ScenarioReport } = $props();
</script>

<h2 style="margin-top:0">
  {report.name}
  <span class={"badge " + (report.ok ? "d-safe" : "d-destructive")}>
    {report.ok ? "PASS" : "FAIL"} · {report.duration_ms} ms
  </span>
</h2>
<div class="timeline">
  {#each report.steps as s, i (i)}
    <div class={"tl-step " + s.status}>
      <span class="t">{s.duration_ms} ms</span>
      <div>
        <strong>{s.name}</strong>
        {#if s.command}<code style="margin-left:8px;font-size:12px;color:var(--muted)">{s.command}</code>{/if}
        <div style="font-size:12.5px;color:{s.status === 'pass' ? 'var(--ok)' : 'var(--bad)'}">
          {s.status}{s.outcome ? ` · ${s.outcome}` : ""}{s.detail ? ` — ${s.detail}` : ""}
        </div>
      </div>
    </div>
  {/each}
</div>
