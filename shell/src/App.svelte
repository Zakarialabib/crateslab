<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorToLines } from "./api";
  import type { Manifest, WorkspaceSummary } from "./types";
  import CommandsView from "./components/CommandsView.svelte";
  import CoverageView from "./components/CoverageView.svelte";
  import ScenariosView from "./components/ScenariosView.svelte";
  import DiscoveryView from "./components/DiscoveryView.svelte";

  type View = "commands" | "events" | "servers" | "limits" | "coverage" | "scenarios" | "discovery";
  const views: View[] = ["commands", "events", "servers", "limits", "coverage", "scenarios", "discovery"];

  let workspaces = $state<WorkspaceSummary[]>([]);
  let current = $state("");
  let manifest = $state<Manifest | null>(null);
  let view = $state<View>("commands");
  let backend = $state("mock");
  let error = $state<string[] | null>(null);

  onMount(async () => {
    try {
      const ws = await api.workspaces();
      workspaces = ws;
      if (ws.length > 0) current = ws[0].name;
    } catch (e) {
      error = errorToLines(e as never);
    }
    try {
      backend = (await api.backendInfo()).backend;
    } catch {
      // the chip keeps its default and the sidebar stays usable
    }
  });

  $effect(() => {
    const ws = current;
    if (!ws) return;
    api
      .manifest(ws)
      .then((m) => (manifest = m))
      .catch((e) => (error = errorToLines(e as never)));
  });
</script>

<div class="shell">
  <aside class="sidebar">
    <div class="brand">cratelab<small>the crate bench</small></div>

    <div class="ws-list">
      {#each workspaces as w (w.name)}
        <button class={"ws-btn" + (w.name === current ? " active" : "")} onclick={() => (current = w.name)}>
          {w.name} <span class="v">v{w.version}</span>
        </button>
      {/each}
    </div>

    <nav class="nav">
      {#each views as v (v)}
        <button class={view === v ? "active" : ""} onclick={() => (view = v)}>
          {v[0].toUpperCase() + v.slice(1)}
        </button>
      {/each}
    </nav>

    <div class="backend-chip">
      backend: {backend}
    </div>
  </aside>

  <main class="main">
    {#if error}
      <div class="result err">{error.join("\n")}</div>
    {/if}

    {#if manifest && current}
      <h1>
        {manifest.workspace.name}
        <span class="badge">v{manifest.workspace.version}</span>
      </h1>
      <p class="sub">
        facade <code>{manifest.workspace.facade}</code> · core <code>{manifest.workspace.core}</code> · {manifest.workspace.repo}
      </p>

      {#if view === "commands"}
        <CommandsView {manifest} />
      {:else if view === "events"}
        <h2>Event surface ({manifest.events.length})</h2>
        <table>
          <thead>
            <tr><th>Id</th><th>Source</th><th>Payload</th><th>Confidence</th></tr>
          </thead>
          <tbody>
            {#each manifest.events as e (e.id)}
              <tr>
                <td><code>{e.id}</code></td>
                <td><span class="badge">{e.source}</span></td>
                <td>{e.payload}</td>
                <td>{e.confidence ?? "—"}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if view === "servers"}
        <h2>Servers ({manifest.servers.length})</h2>
        <div class="grid">
          {#each manifest.servers as s (s.id)}
            <div class="card">
              <h3>{s.id}</h3>
              <div class="cid">{s.crate}</div>
              <div class="kv" style="margin-top:10px">
                <span class="k">port</span><span>{s.default_port}</span>
                <span class="k">health</span><span>{s.health_path ?? "—"}</span>
              </div>
              <ul style="font-size:12.5px;color:var(--muted);padding-left:18px">
                {#each s.routes as r (r)}<li>{r}</li>{/each}
              </ul>
            </div>
          {/each}
        </div>
      {:else if view === "limits"}
        <h2>Limits the lab exercises ({manifest.limits.length})</h2>
        <table>
          <thead>
            <tr><th>Id</th><th>Kind</th><th>Applies to</th><th>Notes</th></tr>
          </thead>
          <tbody>
            {#each manifest.limits as l (l.id)}
              <tr>
                <td><code>{l.id}</code></td>
                <td><span class="badge">{l.kind}</span></td>
                <td>{l.applies_to}</td>
                <td style="color:var(--muted)">{l.description}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {:else if view === "coverage"}
        <CoverageView workspace={current} />
      {:else if view === "scenarios"}
        <ScenariosView />
      {:else if view === "discovery"}
        <DiscoveryView />
      {/if}
    {/if}

    {#if !manifest && !error}
      <p class="sub">loading manifests…</p>
    {/if}
  </main>
</div>
