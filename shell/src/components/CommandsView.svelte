<script lang="ts">
  import type { CommandSpec, Manifest } from "../types";
  import CommandForm from "./CommandForm.svelte";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Table from "$lib/components/ui/table/index.js";
  import { cn } from "$lib/utils.js";
  import type { BadgeVariant } from "$lib/components/ui/badge/index.js";

  let { manifest }: { manifest: Manifest } = $props();
  let selected = $state<CommandSpec | null>(null);

  // A different workspace is a different surface — start from its first command.
  $effect(() => {
    selected = manifest.commands[0] ?? null;
  });

  // Danger reads as an honest traffic light, not decoration.
  const dangerVariant = (danger: string): BadgeVariant =>
    danger === "destructive" ? "destructive" : danger === "caution" ? "outline" : "secondary";
  const dangerClass = (danger: string): string =>
    danger === "safe" ? "border-success/40 bg-success/10 text-success" : "";
</script>

<h2 class="mb-3 text-[15px] font-semibold">Command surface ({manifest.commands.length})</h2>
<div class="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
  {#each manifest.commands as c (c.id)}
    <Card.Root
      class={cn(
        "cursor-pointer gap-2 transition-shadow hover:shadow-md",
        selected?.id === c.id && "ring-2 ring-ring",
      )}
      role="button"
      tabindex={0}
      onclick={() => (selected = c)}
      onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (selected = c)}
    >
      <Card.Header>
        <Card.Title class="flex flex-wrap items-center gap-1.5 text-[13px] font-semibold">
          {c.label}
          <Badge variant={dangerVariant(c.danger)} class={cn("text-[9px] font-semibold tracking-wide uppercase", dangerClass(c.danger))}>
            {c.danger}
          </Badge>
          <Badge variant="secondary" class="text-[9px] font-semibold tracking-wide uppercase">{c.kind}</Badge>
        </Card.Title>
        <Card.Description class="font-mono text-[11px] text-muted-foreground">{c.id}</Card.Description>
      </Card.Header>
      {#if c.help}
        <Card.Content>
          <p class="text-[12.5px] text-muted-foreground">{c.help}</p>
        </Card.Content>
      {/if}
    </Card.Root>
  {/each}
</div>

{#if selected}
  {#key selected.id}
    <CommandForm {manifest} command={selected} />
  {/key}
{/if}

{#if manifest.device_classes.length > 0}
  <h2 class="mt-5 mb-3 text-[15px] font-semibold">Device classes &amp; honest capability sets</h2>
  <div class="overflow-hidden rounded-lg border border-border bg-card">
    <Table.Root>
      <Table.Header>
        <Table.Row>
          <Table.Head>Class</Table.Head>
          <Table.Head>Capabilities</Table.Head>
          <Table.Head>Capability source</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {#each manifest.device_classes as d (d.id)}
          <Table.Row>
            <Table.Cell><code class="font-mono text-[12px]">{d.id}</code></Table.Cell>
            <Table.Cell>
              <span class="flex flex-wrap gap-1.5">
                {#each d.capabilities as cap (cap)}
                  <Badge variant="secondary" class="text-[9px] font-semibold tracking-wide">{cap}</Badge>
                {/each}
              </span>
            </Table.Cell>
            <Table.Cell class="text-muted-foreground">{d.capability_source}</Table.Cell>
          </Table.Row>
        {/each}
      </Table.Body>
    </Table.Root>
  </div>
{/if}
