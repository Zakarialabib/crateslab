<script lang="ts">
  import { api, errorToLines } from "../api";
  import type { CommandSpec, LabErrorValue, Manifest, SinkOutcome } from "../types";
  import ArgWidget from "./ArgWidget.svelte";

  // One command's generated form + result pane. Destructive commands open
  // the confirm step before anything is sent to the engine.
  let { manifest, command }: { manifest: Manifest; command: CommandSpec } = $props();

  function initialValues(c: CommandSpec): Record<string, unknown> {
    const init: Record<string, unknown> = {};
    for (const a of c.args) if (a.default !== undefined) init[a.name] = a.default;
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

<h2>Invoke <code>{command.id}</code></h2>
<div class="card" style="max-width:640px">
  <p class="sub" style="margin:0">
    {command.help ?? command.label} · timeout {command.timeout_ms} ms{command.rate_limit
      ? ` · rate ${command.rate_limit.per_second}/s (burst ${command.rate_limit.burst})`
      : ""}
  </p>
  <div class="form">
    {#each command.args as a (a.name)}
      <label>
        {a.name} <span class="badge">{a.type}</span>
        <ArgWidget arg={a} value={values[a.name]} onvalue={(v) => (values[a.name] = v)} />
      </label>
    {/each}
    <div class="run-row">
      <button class="primary" disabled={!canRun} onclick={onPrimary}>
        {busy ? "running…" : "Run"}
      </button>
      {#if missingRequired.length > 0}
        <span style="color:var(--muted);font-size:12.5px">
          needs: {missingRequired.map((a) => a.name).join(", ")}
        </span>
      {/if}
    </div>
  </div>
</div>

{#if confirmOpen}
  <div class="card" style="max-width:640px;margin-top:12px;border-color:var(--bad)">
    <h3>Confirm destructive command</h3>
    <p class="sub" style="margin:6px 0 10px">
      <code>{command.id}</code> mutates the target. Refusals and outcomes still book coverage.
    </p>
    <div class="run-row">
      <button class="primary" onclick={() => run(true)}>Yes, run it</button>
      <button class="ghost" onclick={() => (confirmOpen = false)}>Cancel</button>
    </div>
  </div>
{/if}

{#if err}
  <div class="result err" style="margin-top:14px">{errorToLines(err).join("\n")}</div>
{/if}

{#if result}
  <div class="result" style="margin-top:14px">{JSON.stringify(result, null, 2)}</div>
  {#if result.events.length > 0}
    <p class="sub" style="margin-top:8px">
      emitted: {#each result.events as e (e.id)}<span class="badge">{e.id}</span>{/each}
    </p>
  {/if}
{/if}
