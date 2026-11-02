<script lang="ts">
  import type { CommandSpec, Manifest } from "../types";
  import CommandForm from "./CommandForm.svelte";

  let { manifest }: { manifest: Manifest } = $props();
  let selected = $state<CommandSpec | null>(null);

  // A different workspace is a different surface — start from its first command.
  $effect(() => {
    selected = manifest.commands[0] ?? null;
  });
</script>

<h2>Command surface ({manifest.commands.length})</h2>
<div class="grid">
  {#each manifest.commands as c (c.id)}
    <div
      class="card"
      style="cursor:pointer;{selected?.id === c.id ? 'outline:2px solid var(--accent);' : ''}"
      role="button"
      tabindex="0"
      onclick={() => (selected = c)}
      onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (selected = c)}
    >
      <h3>
        {c.label}
        <span class={"badge d-" + c.danger}>{c.danger}</span>
        <span class="badge">{c.kind}</span>
      </h3>
      <div class="cid">{c.id}</div>
      {#if c.help}
        <p style="font-size:12.5px;color:var(--muted);margin-bottom:0">{c.help}</p>
      {/if}
    </div>
  {/each}
</div>

{#if selected}
  {#key selected.id}
    <CommandForm {manifest} command={selected} />
  {/key}
{/if}

{#if manifest.device_classes.length > 0}
  <h2>Device classes &amp; honest capability sets</h2>
  <table>
    <thead>
      <tr><th>Class</th><th>Capabilities</th><th>Capability source</th></tr>
    </thead>
    <tbody>
      {#each manifest.device_classes as d (d.id)}
        <tr>
          <td><code>{d.id}</code></td>
          <td>
            {#each d.capabilities as cap (cap)}<span class="badge">{cap}</span>{/each}
          </td>
          <td>{d.capability_source}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
