<script lang="ts">
  import { api, errorToLines } from "../api";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import * as Card from "$lib/components/ui/card/index.js";

  let root = $state(".\\");
  let out = $state<string | null>(null);
  let err = $state<string[] | null>(null);

  async function scan() {
    err = null;
    out = null;
    try {
      out = JSON.stringify(await api.discoverLocal(root), null, 2);
    } catch (e) {
      err = errorToLines(e as never);
    }
  }
</script>

<h2 class="mb-2 text-[15px] font-semibold">Discovery &amp; matching</h2>
<p class="mb-3 text-[12.5px] text-muted-foreground">
  Point the lab at a local checkout (virtual Cargo.toml with <code class="rounded bg-muted px-1.5 py-0.5 font-mono text-[11px]">members</code>) — registry discovery rides
  the crates.io sparse index behind the <code class="rounded bg-muted px-1.5 py-0.5 font-mono text-[11px]">registry</code> feature.
</p>

<Card.Root class="max-w-[520px] gap-3">
  <Label class="flex flex-col items-start gap-1.5 text-[12px]">
    Workspace root
    <Input bind:value={root} />
  </Label>
  <div class="flex items-center gap-3">
    <Button onclick={scan}>Scan</Button>
  </div>
</Card.Root>

{#if err}
  <pre class="mt-3.5 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 font-mono text-[11px] whitespace-pre-wrap text-destructive">{err.join("\n")}</pre>
{/if}
{#if out}
  <pre class="mt-3.5 overflow-x-auto rounded-md border border-border bg-card px-3 py-2 font-mono text-[11px] text-foreground">{out}</pre>
{/if}
