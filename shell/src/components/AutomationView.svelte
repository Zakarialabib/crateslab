<script lang="ts">
  import ForgeView from "./ForgeView.svelte";
  import OpsDashboard from "./OpsDashboard.svelte";
  import * as Tabs from "$lib/components/ui/tabs/index.js";

  // Automation is one concern with two surfaces: the work orchestration
  // (workflows, cases, providers) and the integrations board (scenarios,
  // corpus, CircleCI). Kept as tabs instead of two top-level entries so the
  // sidebar states the mission, not the implementation.
  let { tab = "workflows" }: { tab?: "workflows" | "integrations" } = $props();
  let active = $state("workflows");
  // Sync both ways: the sidebar can deep-link a tab, the tabs can be clicked.
  $effect(() => { active = tab; });
</script>

<Tabs.Root bind:value={active} class="gap-3">
  <Tabs.List>
    <Tabs.Trigger value="workflows">Workflows</Tabs.Trigger>
    <Tabs.Trigger value="integrations">Integrations &amp; scenarios</Tabs.Trigger>
  </Tabs.List>

  <Tabs.Content value="workflows">
    <ForgeView />
  </Tabs.Content>
  <Tabs.Content value="integrations">
    <OpsDashboard />
  </Tabs.Content>
</Tabs.Root>
