<script lang="ts">
  import type { ForgeRepository } from "../types";
  import * as Select from "$lib/components/ui/select";
  import * as DropdownMenu from "$lib/components/ui/dropdown-menu";
  import * as Tooltip from "$lib/components/ui/tooltip";
  import { Button } from "$lib/components/ui/button";
  import { cn } from "$lib/utils";

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
    repositories?: ForgeRepository[];
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
  const readinessHint = $derived(
    `${configured} configured · ${Math.max(0, providerTotal - probed)} unprobed. Configured and Probed are different claims.`,
  );
  const envHint = $derived(
    live
      ? "Live provider calls are confirmed per run and stay on free tiers."
      : "Every session starts MOCK: dry runs are local and cost nothing.",
  );
  const scopeLabel = (repo: ForgeRepository) =>
    `${repo.name} · ${repo.available ? `${repo.crate_count} crates` : "path unavailable"}`;
</script>

<Tooltip.Provider>
  <header
    class="sticky top-0 z-5 flex flex-wrap items-center gap-x-3.5 gap-y-2.5 border-b border-border bg-linear-to-b from-background to-secondary/60 px-5 py-2.5 shadow-[0_1px_0_var(--color-border)]"
  >
    <div class="text-[15px] font-extrabold tracking-[0.2px] text-brand">
      cratelab<small class="block text-[9px] font-normal uppercase tracking-[1.4px] text-muted-foreground"
        >· the crate bench</small
      >
    </div>

    <div class="flex items-center gap-1.5">
      <span class="text-[10px] font-bold uppercase tracking-[0.8px] text-muted-foreground">scope</span>
      <Select.Root
        type="single"
        value={target}
        disabled={repositoryBusy || repositories.length === 0}
        onValueChange={(next) => {
          if (next) onselect?.(next);
        }}
      >
        <Select.Trigger size="sm" class="h-7 max-w-[260px] text-xs">
          <Select.Value placeholder="no repository" />
        </Select.Trigger>
        <Select.Content>
          {#if repositories.length === 0}
            <Select.Item value={target || "__none"} label={target || "no repository"} disabled>
              {target || "no repository"}
            </Select.Item>
          {:else}
            {#each repositories as repo (repo.id)}
              <Select.Item value={repo.id} label={scopeLabel(repo)}>{scopeLabel(repo)}</Select.Item>
            {/each}
          {/if}
        </Select.Content>
      </Select.Root>
    </div>

    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <span
            {...props}
            class={cn(
              "cursor-default rounded-full border px-2 py-[3px] font-mono text-[10.5px] font-extrabold tracking-[0.5px]",
              live
                ? "border-warning bg-warning/10 text-warning"
                : "border-primary/40 bg-primary/10 text-primary",
            )}
          >
            [{live ? "LIVE" : "MOCK"}]
          </span>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>{envHint}</Tooltip.Content>
    </Tooltip.Root>

    <Tooltip.Root>
      <Tooltip.Trigger>
        {#snippet child({ props })}
          <button
            {...props}
            class="cursor-pointer rounded-full border border-border bg-card px-2.5 py-[3px] text-[10.5px] font-bold text-muted-foreground transition-colors hover:border-primary hover:text-primary"
            onclick={() => onnav?.("forge-settings")}
          >
            rungs {probed}/{providerTotal} probed
          </button>
        {/snippet}
      </Tooltip.Trigger>
      <Tooltip.Content>{readinessHint}</Tooltip.Content>
    </Tooltip.Root>

    <nav class="ml-1 flex items-center gap-1.5 text-[11px]" aria-label="Section and view">
      <span class="text-[9.5px] font-extrabold uppercase tracking-[1.3px] text-muted-foreground">{section}</span>
      <span class="text-border">/</span>
      <span class="font-bold text-foreground">{viewLabel}</span>
    </nav>

    <div class="ml-auto flex items-center gap-2">
      <DropdownMenu.Root>
        <DropdownMenu.Trigger>
          {#snippet child({ props })}
            <Button {...props} variant="outline" size="sm" class="text-xs font-bold">Run ▾</Button>
          {/snippet}
        </DropdownMenu.Trigger>
        <DropdownMenu.Content align="end" class="w-72">
          {#if quickCases.length === 0}
            <div class="px-2 py-1.5 text-xs text-muted-foreground">
              No case prepared yet — dry runs are local and free.
            </div>
          {:else}
            {#each quickCases as item (item.label + item.hint)}
              <DropdownMenu.Item
                onSelect={() => onnav?.("forge")}
                class="flex-col items-start gap-0.5"
              >
                <span>{item.label}</span>
                <span class="text-xs text-muted-foreground">{item.hint}</span>
              </DropdownMenu.Item>
            {/each}
          {/if}
          <DropdownMenu.Separator />
          <DropdownMenu.Item onSelect={() => onnav?.("forge")} class="flex-col items-start gap-0.5">
            <span>Open case workbench</span>
            <span class="text-xs text-muted-foreground">repo · crate · task · provider</span>
          </DropdownMenu.Item>
        </DropdownMenu.Content>
      </DropdownMenu.Root>

      <Button variant="outline" size="sm" class="text-xs" onclick={() => onnav?.("reports")}>Reports</Button>
    </div>
  </header>
</Tooltip.Provider>
