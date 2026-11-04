<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorToLines } from "./api";
  import type { FleetStatus, Manifest } from "./types";
  import CommandsView from "./components/CommandsView.svelte";
  import CoverageView from "./components/CoverageView.svelte";
  import ScenariosView from "./components/ScenariosView.svelte";
  import DiscoveryView from "./components/DiscoveryView.svelte";
  import DashboardView from "./components/DashboardView.svelte";
  import AutomationView from "./components/AutomationView.svelte";
  import ReportsView from "./components/ReportsView.svelte";
  import FleetSettings from "./components/FleetSettings.svelte";
  import TopHeader from "./components/TopHeader.svelte";

  // The sidebar states the mission, not the implementation:
  // Project (state & evolution) → Automation → Reports → Settings → Lab tools.
  type View = "dashboard" | "fleet" | "ops-dashboard" | "reports" | "fleet-settings" | "commands" | "events" | "servers" | "limits" | "coverage" | "scenarios" | "discovery";
  const groups: { label: string; items: { id: View; label: string }[] }[] = [
    { label: "Project", items: [{ id: "dashboard", label: "Dashboard" }] },
    { label: "Automation", items: [{ id: "fleet", label: "Workflows" }, { id: "ops-dashboard", label: "Integrations & scenarios" }] },
    { label: "Evidence", items: [{ id: "reports", label: "Reports" }] },
    { label: "Settings", items: [{ id: "fleet-settings", label: "Providers & credentials" }] },
    { label: "Lab tools", items: [
      { id: "commands", label: "Commands" }, { id: "events", label: "Events" }, { id: "servers", label: "Servers" },
      { id: "limits", label: "Limits" }, { id: "coverage", label: "Coverage" }, { id: "scenarios", label: "Scenarios" }, { id: "discovery", label: "Discovery" },
    ] },
  ];

  let current = $state("");
  let manifest = $state<Manifest | null>(null);
  let view = $state<View>("dashboard");
  let backend = $state("mock");
  let error = $state<string[] | null>(null);
  let manifestWarns = $state<string[]>([]);
  let fleetStatus = $state<FleetStatus | null>(null);
  let repositoryBusy = $state(false);

  // Header: breadcrumb + counters. Every number here comes from fleet_status.
  const sectionOf = $derived(groups.find((group) => group.items.some((item) => item.id === view))?.label ?? "");
  const viewLabelOf = $derived(groups.flatMap((group) => group.items).find((item) => item.id === view)?.label ?? "");
  const probed = $derived(fleetStatus?.providers.filter((p) => p.probe_ok === true).length ?? 0);
  const configured = $derived(fleetStatus?.providers.filter((p) => p.credential_ready).length ?? 0);
  const providerTotal = $derived(fleetStatus?.providers.length ?? 0);
  const quickCases = $derived(
    (fleetStatus?.runs ?? []).slice(0, 5).map((run) => ({ label: run.task, hint: `${run.target} · ${run.mode}` })),
  );
  const navCounts = $derived<Record<string, string>>({
    fleet: `${fleetStatus?.pilot.crates.length ?? 0} crates`,
    "ops-dashboard": `${configured} configured`,
    reports: `${fleetStatus?.runs.length ?? 0} runs`,
    "fleet-settings": `${probed}/${providerTotal}`,
  });

  onMount(async () => {
    try {
      const ws = await api.workspaces();
      if (ws.length > 0) current = ws.find((item) => item.name === "tv-rs")?.name ?? ws[0].name;
    } catch (e) {
      error = errorToLines(e as never);
    }
    try {
      backend = (await api.backendInfo()).backend;
    } catch {
      // the chip keeps its default and the sidebar stays usable
    }
    api
      .manifestErrors()
      .then((errs) => (manifestWarns = errs ?? []))
      .catch(() => (manifestWarns = []));
    api.fleetStatus().then((next) => { fleetStatus = next; current = next.target; }).catch(() => {});
  });

  async function selectRepository(repository: string) {
    repositoryBusy = true;
    try {
      fleetStatus = await api.fleetSelectRepository(repository);
      current = repository;
    } catch (e) { error = errorToLines(e as never); }
    finally { repositoryBusy = false; }
  }

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
    <div class="brand">cratelab<small>evidence · workflows · learning</small></div>

    <nav class="nav">
      {#each groups as group (group.label)}
        <div class="nav-group"><span class="nav-group-label">{group.label}</span></div>
        {#each group.items as item (item.id)}
          <button class={view === item.id ? "active" : ""} onclick={() => (view = item.id)}>
            {item.label}
            {#if navCounts[item.id]}<span class="nav-count">{navCounts[item.id]}</span>{/if}
          </button>
        {/each}
      {/each}
    </nav>

    <div class="backend-chip">
      backend: {backend}
    </div>
  </aside>

  <div class="content">
    <TopHeader
      repositories={fleetStatus?.repositories ?? []}
      target={fleetStatus?.target ?? ""}
      {repositoryBusy}
      {probed}
      {providerTotal}
      {configured}
      env={backend}
      section={sectionOf}
      viewLabel={viewLabelOf}
      {quickCases}
      onselect={selectRepository}
      onnav={(next) => (view = next as View)}
    />

    <main class="main">
      <div class="workspace-context">
        <div><span class="section-kicker">ACTIVE REPOSITORY</span><strong>{fleetStatus?.target ?? "Selecting workspace…"}</strong><small>{fleetStatus?.target_path ?? "Local Rust source used by Fleet workflows"} · switch scope in the header</small></div>
      </div>
      {#key fleetStatus?.target ?? "tv-rs"}
      {#if view === "dashboard"}
        <DashboardView onnav={(next) => (view = next as View)} />
      {:else if view === "fleet" || view === "ops-dashboard"}
        <AutomationView tab={view === "fleet" ? "workflows" : "integrations"} />
      {:else if view === "reports"}
        <ReportsView />
      {:else if view === "fleet-settings"}
        <FleetSettings />
      {:else}
      {#if error}
        <div class="result err">{error.join("\n")}</div>
      {/if}

      {#if manifestWarns.length > 0}
        <div class="result warn">
          <strong>manifests that failed to load</strong>
          {#each manifestWarns as w (w)}
            <div style="margin-top:4px">{w}</div>
          {/each}
        </div>
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
      {/if}
      {/key}
    </main>
  </div>
</div>
