<script lang="ts">
  import type { HeatRow } from "../types";

  // Manifest surface vs exercised cells. A refusal colours amber *with
  // respect*, not alarm: it proves the "refusals are part of the API"
  // invariant holds at that cell.
  let { percent, rows }: { percent: number; rows: HeatRow[] } = $props();
</script>

<h2>Coverage heatmap</h2>
<p class="sub">
  manifest surface vs exercised (workspace, command, arg-bucket) cells — {percent.toFixed(1)}% of declared
  commands covered
</p>
<table>
  <thead>
    <tr>
      <th>Status</th><th>Command</th><th>Label</th><th>Danger</th>
      <th>Buckets</th><th>Runs</th><th>Pass</th><th>Refused</th><th>Timeout</th><th>RateLim</th><th>Failed</th>
    </tr>
  </thead>
  <tbody>
    {#each rows as r (r.command)}
      <tr>
        <td><span class={"dot " + r.status}></span></td>
        <td><code>{r.command}</code></td>
        <td>{r.label}</td>
        <td>{r.danger}</td>
        <td>{r.buckets_covered}</td>
        <td>{r.runs}</td>
        <td>{r.pass}</td>
        <td>{r.refused}</td>
        <td>{r.timeout}</td>
        <td>{r.rate_limited}</td>
        <td>{r.failed}</td>
      </tr>
    {/each}
  </tbody>
</table>
