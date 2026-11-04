<script lang="ts">
  import FleetView from "./FleetView.svelte";
  import OpsDashboard from "./OpsDashboard.svelte";

  // Automation is one concern with two surfaces: the work orchestration
  // (workflows, cases, providers) and the integrations board (scenarios,
  // corpus, CircleCI). Kept as tabs instead of two top-level entries so the
  // sidebar states the mission, not the implementation.
  let { tab = "workflows" }: { tab?: "workflows" | "integrations" } = $props();
  let active = $state<"workflows" | "integrations">("workflows");
  // Sync both ways: the sidebar can deep-link a tab, the tabs can be clicked.
  $effect(() => { active = tab; });
</script>

<div class="automation-tabs">
  <button class={active === "workflows" ? "active" : ""} onclick={() => (active = "workflows")}>Workflows</button>
  <button class={active === "integrations" ? "active" : ""} onclick={() => (active = "integrations")}>Integrations &amp; scenarios</button>
</div>

{#if active === "workflows"}
  <FleetView />
{:else}
  <OpsDashboard />
{/if}
