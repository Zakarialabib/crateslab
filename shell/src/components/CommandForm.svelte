<script lang="ts">
  import { api, errorToLines } from "../api";
  import type { CommandSpec, LabErrorValue, Manifest, SinkOutcome } from "../types";
  import ArgWidget from "./ArgWidget.svelte";
  import { Badge } from "$lib/components/ui/badge/index.js";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Label } from "$lib/components/ui/label/index.js";
  import * as Card from "$lib/components/ui/card/index.js";
  import * as Dialog from "$lib/components/ui/dialog/index.js";

  // One command's generated form + result pane. Destructive commands open
  // the confirm step before anything is sent to the engine.
  let { manifest, command }: { manifest: Manifest; command: CommandSpec } = $props();

  function initialValues(c: CommandSpec): Record<string, unknown> {
    const init: Record<string, unknown> = {};
    for (const a of c.args) {
      if (a.default !== undefined) init[a.name] = a.default;
      else if (a.example !== undefined) init[a.name] = a.example;
    }
    return init;
  }

  let values = $state<Record<string, unknown>>({});
  let confirmOpen = $state(false);
  let result = $state<SinkOutcome | null>(null);
  let err = $state<LabErrorValue | null>(null);
  let busy = $state(false);

  // Seeded from the manifest's declared defaults. The parent keys this
  // component by command id, so this runs once per command.
  $effect(() => {
    values = initialValues(command);
  });

  const missingRequired = $derived(
    command.args.filter((a) => a.required && (values[a.name] === undefined || values[a.name] === "")),
  );
  const canRun = $derived(missingRequired.length === 0 && !busy);

  async function run(confirmed: boolean) {
    busy = true;
    err = null;
    result = null;
    confirmOpen = false;
    try {
      result = await api.runCommand(manifest.workspace.name, command.id, values, confirmed);
    } catch (e) {
      err = e as LabErrorValue;
    } finally {
      busy = false;
    }
  }

  function onPrimary() {
    if (command.danger === "destructive") confirmOpen = true;
    else void run(false);
  }
</script>

<h2 class="mb-3 text-[15px] font-semibold">Invoke <code class="rounded bg-muted px-1.5 py-0.5 font-mono text-[12px]">{command.id}</code></h2>
<Card.Root class="max-w-[640px] gap-3">
  <p class="text-[12.5px] text-muted-foreground">
    {command.help ?? command.label} · timeout {command.timeout_ms} ms{command.rate_limit
      ? ` · rate ${command.rate_limit.per_second}/s (burst ${command.rate_limit.burst})`
      : ""}
  </p>
  <div class="flex flex-col gap-3.5">
    {#each command.args as a (a.name)}
      <Label class="flex flex-col items-start gap-1.5 text-[12px]">
        <span class="flex items-center gap-2">
          {a.name}
          <Badge variant="secondary" class="text-[9px] font-semibold tracking-wide uppercase">{a.type}</Badge>
        </span>
        <div class="w-full">
          <ArgWidget arg={a} value={values[a.name]} onvalue={(v) => (values[a.name] = v)} />
        </div>
      </Label>
    {/each}
    <div class="flex items-center gap-3">
      <Button disabled={!canRun} onclick={onPrimary}>
        {busy ? "running…" : "Run"}
      </Button>
      {#if missingRequired.length > 0}
        <span class="text-[12.5px] text-muted-foreground">
          needs: {missingRequired.map((a) => a.name).join(", ")}
        </span>
      {/if}
    </div>
  </div>
</Card.Root>

<Dialog.Root bind:open={confirmOpen}>
  <Dialog.Content showCloseButton={false} class="max-w-[640px] gap-3">
    <Dialog.Title class="text-[14px] font-semibold">Confirm destructive command</Dialog.Title>
    <Dialog.Description class="text-[12.5px] text-muted-foreground">
      <code class="rounded bg-muted px-1.5 py-0.5 font-mono text-[11px]">{command.id}</code> mutates the target. Refusals
      and outcomes still book coverage.
    </Dialog.Description>
    <div class="mt-1 flex items-center gap-3">
      <Button variant="destructive" onclick={() => run(true)}>Yes, run it</Button>
      <Button variant="ghost" onclick={() => (confirmOpen = false)}>Cancel</Button>
    </div>
  </Dialog.Content>
</Dialog.Root>

{#if err}
  <pre class="mt-3.5 w-fit max-w-full overflow-x-auto rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 font-mono text-[11px] whitespace-pre-wrap text-destructive">{errorToLines(err).join("\n")}</pre>
{/if}

{#if result}
  <pre class="mt-3.5 max-w-full overflow-x-auto rounded-md border border-border bg-card px-3 py-2 font-mono text-[11px] text-foreground">{JSON.stringify(result, null, 2)}</pre>
  {#if result.events.length > 0}
    <p class="mt-2 flex flex-wrap items-center gap-1.5 text-[12.5px] text-muted-foreground">
      emitted:
      {#each result.events as e (e.id)}
        <Badge variant="secondary" class="text-[9px] font-semibold tracking-wide">{e.id}</Badge>
      {/each}
    </p>
  {/if}
{/if}
