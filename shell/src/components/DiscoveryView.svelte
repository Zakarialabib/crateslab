<script lang="ts">
  import { api, errorToLines } from "../api";

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

<h2>Discovery &amp; matching</h2>
<p class="sub">
  Point the lab at a local checkout (virtual Cargo.toml with <code>members</code>) — registry discovery rides
  the crates.io sparse index behind the <code>registry</code> feature.
</p>

<div class="form" style="max-width:520px">
  <label>
    Workspace root
    <input bind:value={root} />
  </label>
  <div class="run-row">
    <button class="primary" onclick={scan}>Scan</button>
  </div>
</div>

{#if err}<div class="result err" style="margin-top:14px">{err.join("\n")}</div>{/if}
{#if out}<div class="result" style="margin-top:14px">{out}</div>{/if}
