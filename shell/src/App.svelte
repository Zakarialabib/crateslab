<script lang="ts">
  import { onMount } from "svelte";
  import { api, errorToLines } from "./api";
  import type { ForgeStatus, Manifest } from "./types";
  import CommandsView from "./components/CommandsView.svelte";
  import CoverageView from "./components/CoverageView.svelte";
  import ScenariosView from "./components/ScenariosView.svelte";
  import DiscoveryView from "./components/DiscoveryView.svelte";
  import DashboardView from "./components/DashboardView.svelte";
  import AutomationView from "./components/AutomationView.svelte";
  import ReportsView from "./components/ReportsView.svelte";
  import ForgeSettings from "./components/ForgeSettings.svelte";
  import TopHeader from "./components/TopHeader.svelte";
  import { Badge } from "$lib/components/ui/badge";
  import * as Card from "$lib/components/ui/card";
  import * as Table from "$lib/components/ui/table";
  import { cn } from "$lib/utils";

  // The sidebar states the mission, not the implementation:
  // Project (state & evolution) → Automation → Reports → Settings → Lab tools.
  type View = "dashboard" | "forge" | "ops-dashboard" | "reports" | "forge-settings" | "commands" | "events" | "servers" | "limits" | "coverage" | "scenarios" | "discovery";
  const groups: { label: string; items: { id: View; label: string }[] }[] = [
    { label: "Project", items: [{ id: "dashboard", label: "Dashboard" }] },
    { label: "Automation", items: [{ id: "forge", label: "Workflows" }, { id: "ops-dashboard", label: "Integrations & scenarios" }] },
    { label: "Evidence", items: [{ id: "reports", label: "Reports" }] },
    { label: "Settings", items: [{ id: "forge-settings", label: "Providers & credentials" }] },
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
  let forgeStatus = $state<ForgeStatus | null>(null);
  let repositoryBusy = $state(false);

  // Header: breadcrumb + counters. Every number here comes from forge_status.
  const sectionOf = $derived(groups.find((group) => group.items.some((item) => item.id === view))?.label ?? "");
  const viewLabelOf = $derived(groups.flatMap((group) => group.items).find((item) => item.id === view)?.label ?? "");
  const probed = $derived(forgeStatus?.providers.filter((p) => p.probe_ok === true).length ?? 0);
  const configured = $derived(forgeStatus?.providers.filter((p) => p.credential_ready).length ?? 0);
  const providerTotal = $derived(forgeStatus?.providers.length ?? 0);
  const quickCases = $derived(
    (forgeStatus?.runs ?? []).slice(0, 5).map((run) => ({ label: run.task, hint: `${run.target} · ${run.mode}` })),
  );
  const navCounts = $derived<Record<string, string>>({
    forge: `${forgeStatus?.pilot.crates.length ?? 0} crates`,
    "ops-dashboard": `${configured} configured`,
    reports: `${forgeStatus?.runs.length ?? 0} runs`,
    "forge-settings": `${probed}/${providerTotal}`,
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
    api.forgeStatus().then((next) => { forgeStatus = next; current = next.target; }).catch(() => {});
  });

  async function selectRepository(repository: string) {
    repositoryBusy = true;
    try {
      forgeStatus = await api.forgeSelectRepository(repository);
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

<div class="flex h-screen">
  <aside class="flex w-60 min-w-60 flex-col gap-3.5 bg-brand px-[14px] py-[18px] text-[#e8f0f8]">
    <div class="text-[19px] font-bold tracking-[0.4px]">
      cratelab
      <small class="block text-[11px] font-light uppercase tracking-[2px] text-[#7a9bb8]">evidence · workflows · learning</small>
    </div>

    <nav class="mt-1.5 flex flex-col gap-0.5">
      {#each groups as group (group.label)}
        <span class="mx-2 mb-1 mt-3.5 block text-[9.5px] font-extrabold uppercase tracking-[1.4px] text-[#7a9bb8]">{group.label}</span>
        {#each group.items as item (item.id)}
          <button
            class={cn(
              "cursor-pointer rounded-md px-2.5 py-[7px] text-left text-[13px] text-[#cfe0ee] transition-colors hover:bg-white/10",
              view === item.id && "bg-white/20 font-semibold text-white",
            )}
            onclick={() => (view = item.id)}
          >
            {item.label}
            {#if navCounts[item.id]}<span class={cn("float-right text-[10px] font-semibold text-[#a8c8e0]", view === item.id && "text-[#dbeaf6]")}>{navCounts[item.id]}</span>{/if}
          </button>
        {/each}
      {/each}
    </nav>

    <div class="mt-auto border-t border-white/20 pt-2.5 text-[11px] text-[#7a9bb8]">
      backend: {backend}
    </div>
  </aside>

  <div class="flex min-w-0 flex-1 flex-col">
    <TopHeader
      repositories={forgeStatus?.repositories ?? []}
      target={forgeStatus?.target ?? ""}
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

    <main class="flex-1 overflow-auto px-[26px] py-[22px]">
      <div class="mb-4 flex items-baseline gap-3 rounded-lg border border-border bg-secondary px-4 py-3">
        <span class="text-[10px] font-extrabold uppercase tracking-[1.45px] text-muted-foreground">Active repository</span>
        <strong class="text-sm text-foreground">{forgeStatus?.target ?? "Selecting workspace…"}</strong>
        <small class="text-xs text-muted-foreground">{forgeStatus?.target_path ?? "Local Rust source used by Forge workflows"} · switch scope in the header</small>
      </div>
      {#key forgeStatus?.target ?? "tv-rs"}
      {#if view === "dashboard"}
        <DashboardView onnav={(next) => (view = next as View)} />
      {:else if view === "forge" || view === "ops-dashboard"}
        <AutomationView tab={view === "forge" ? "workflows" : "integrations"} />
      {:else if view === "reports"}
        <ReportsView />
      {:else if view === "forge-settings"}
        <ForgeSettings />
      {:else}
      {#if error}
        <div class="mt-3.5 max-h-80 overflow-auto whitespace-pre-wrap rounded-lg bg-destructive/10 p-3.5 font-mono text-[12.5px] text-destructive">{error.join("\n")}</div>
      {/if}

      {#if manifestWarns.length > 0}
        <div class="mt-3.5 rounded-lg border border-warning/40 bg-warning/10 p-3.5 text-[12.5px] text-warning">
          <strong>manifests that failed to load</strong>
          {#each manifestWarns as w (w)}
            <div class="mt-1">{w}</div>
          {/each}
        </div>
      {/if}

      {#if manifest && current}
        <h1 class="mb-1 flex items-center gap-2 text-xl font-semibold">
          {manifest.workspace.name}
          <Badge variant="outline">v{manifest.workspace.version}</Badge>
        </h1>
        <p class="mb-[18px] text-[13px] text-muted-foreground">
          facade <code>{manifest.workspace.facade}</code> · core <code>{manifest.workspace.core}</code> · {manifest.workspace.repo}
        </p>

        {#if view === "commands"}
          <CommandsView {manifest} />
        {:else if view === "events"}
          <h2 class="mb-2 mt-[22px] text-[15px] font-semibold text-brand">Event surface ({manifest.events.length})</h2>
          <Table.Root class="overflow-hidden rounded-lg border border-border bg-card">
            <Table.Header>
              <Table.Row>
                <Table.Head>Id</Table.Head>
                <Table.Head>Source</Table.Head>
                <Table.Head>Payload</Table.Head>
                <Table.Head>Confidence</Table.Head>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {#each manifest.events as e (e.id)}
                <Table.Row>
                  <Table.Cell><code>{e.id}</code></Table.Cell>
                  <Table.Cell><Badge variant="outline">{e.source}</Badge></Table.Cell>
                  <Table.Cell>{e.payload}</Table.Cell>
                  <Table.Cell>{e.confidence ?? "—"}</Table.Cell>
                </Table.Row>
              {/each}
            </Table.Body>
          </Table.Root>
        {:else if view === "servers"}
          <h2 class="mb-2 mt-[22px] text-[15px] font-semibold text-brand">Servers ({manifest.servers.length})</h2>
          <div class="grid grid-cols-[repeat(auto-fill,minmax(320px,1fr))] gap-3.5">
            {#each manifest.servers as s (s.id)}
              <Card.Root>
                <Card.Header>
                  <Card.Title>{s.id}</Card.Title>
                  <Card.Description class="font-mono text-xs">{s.crate}</Card.Description>
                </Card.Header>
                <Card.Content class="space-y-3">
                  <div class="grid grid-cols-[130px_1fr] gap-x-3.5 gap-y-1 text-[13px]">
                    <span class="text-muted-foreground">port</span><span>{s.default_port}</span>
                    <span class="text-muted-foreground">health</span><span>{s.health_path ?? "—"}</span>
                  </div>
                  <ul class="list-disc pl-[18px] text-[12.5px] text-muted-foreground">
                    {#each s.routes as r (r)}<li>{r}</li>{/each}
                  </ul>
                </Card.Content>
              </Card.Root>
            {/each}
          </div>
        {:else if view === "limits"}
          <h2 class="mb-2 mt-[22px] text-[15px] font-semibold text-brand">Limits the lab exercises ({manifest.limits.length})</h2>
          <Table.Root class="overflow-hidden rounded-lg border border-border bg-card">
            <Table.Header>
              <Table.Row>
                <Table.Head>Id</Table.Head>
                <Table.Head>Kind</Table.Head>
                <Table.Head>Applies to</Table.Head>
                <Table.Head>Notes</Table.Head>
              </Table.Row>
            </Table.Header>
            <Table.Body>
              {#each manifest.limits as l (l.id)}
                <Table.Row>
                  <Table.Cell><code>{l.id}</code></Table.Cell>
                  <Table.Cell><Badge variant="outline">{l.kind}</Badge></Table.Cell>
                  <Table.Cell>{l.applies_to}</Table.Cell>
                  <Table.Cell class="text-muted-foreground">{l.description}</Table.Cell>
                </Table.Row>
              {/each}
            </Table.Body>
          </Table.Root>
        {:else if view === "coverage"}
          <CoverageView workspace={current} />
        {:else if view === "scenarios"}
          <ScenariosView />
        {:else if view === "discovery"}
          <DiscoveryView />
        {/if}
      {/if}

      {#if !manifest && !error}
        <p class="mb-[18px] text-[13px] text-muted-foreground">loading manifests…</p>
      {/if}
      {/if}
      {/key}
    </main>
  </div>
</div>
