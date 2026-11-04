<script lang="ts">
  import type { FleetRepository } from "../types";

  // The persistent header: identity, scope, environment, readiness,
  // breadcrumb and the two actions people actually open. It reads state;
  // it never owns it (ledger and env file stay in the Rust engine).
  let {
    repositories = [],
    target = "",
    repositoryBusy = false,
    probed = 0,
    providerTotal = 0,
    configured = 0,
    env = "mock",
    section = "",
    viewLabel = "",
    quickCases = [],
    onselect,
    onnav,
  }: {
    repositories?: FleetRepository[];
    target?: string;
    repositoryBusy?: boolean;
    probed?: number;
    providerTotal?: number;
    configured?: number;
    env?: string;
    section?: string;
    viewLabel?: string;
    quickCases?: { label: string; hint: string }[];
    onselect?: (repository: string) => void;
    onnav?: (view: string) => void;
  } = $props();

  const live = $derived(env !== "" && env !== "mock");
</script>

<header class="topbar">
  <div class="topbar-brand">cratesLab<small>· the crate bench</small></div>

  <label class="topbar-scope" for="topbar-repository">
    <span>scope</span>
    <select
      id="topbar-repository"
      value={target}
      disabled={repositoryBusy || repositories.length === 0}
      onchange={(event) => onselect?.(event.currentTarget.value)}
    >
      {#each repositories as repo (repo.id)}
        <option value={repo.id}>{repo.name} · {repo.available ? `${repo.crate_count} crates` : "path unavailable"}</option>
      {/each}
      {#if repositories.length === 0}
        <option value={target}>{target || "no repository"}</option>
      {/if}
    </select>
  </label>

  <span
    class="env-chip"
    class:env-live={live}
    title={live
      ? "Live provider calls are confirmed per run and stay on free tiers."
      : "Every session starts MOCK: dry runs are local and cost nothing."}
  >
    [{live ? "LIVE" : "MOCK"}]
  </span>

  <button
    class="readiness-chip"
    onclick={() => onnav?.("fleet-settings")}
    title={`${configured} configured · ${Math.max(0, providerTotal - probed)} unprobed. Configured and Probed are different claims.`}
  >
    rungs {probed}/{providerTotal} probed
  </button>

  <nav class="breadcrumb" aria-label="Section and view">
    <span class="crumb-section">{section}</span>
    <span class="crumb-sep">/</span>
    <span class="crumb-view">{viewLabel}</span>
  </nav>

  <div class="topbar-actions">
    <details class="run-menu">
      <summary>Run ▾</summary>
      <div class="run-menu-list">
        {#if quickCases.length === 0}
          <span class="run-menu-empty">No case prepared yet — dry runs are local and free.</span>
        {:else}
          {#each quickCases as item (item.label + item.hint)}
            <button onclick={() => onnav?.("fleet")}>{item.label}<small>{item.hint}</small></button>
          {/each}
        {/if}
        <button onclick={() => onnav?.("fleet")}>Open case workbench<small>repo · crate · task · provider</small></button>
      </div>
    </details>
    <button class="ghost topbar-action" onclick={() => onnav?.("reports")}>Reports</button>
  </div>
</header>
