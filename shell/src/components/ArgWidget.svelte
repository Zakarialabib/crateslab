<script lang="ts">
  import { api } from "../api";
  import type { ArgSpec, UrlProbe } from "../types";
  import { Button } from "$lib/components/ui/button/index.js";
  import { Input } from "$lib/components/ui/input/index.js";
  import * as Select from "$lib/components/ui/select/index.js";
  import { cn } from "$lib/utils.js";

  let {
    arg,
    value,
    onvalue,
  }: { arg: ArgSpec; value: unknown; onvalue: (v: unknown) => void } = $props();

  // What a good input looks like for this arg, so the form explains itself
  // before the user has to read the manifest.
  const typePlaceholder = $derived.by(() => {
    switch (arg.type) {
      case "mac": return "AA-BB-CC-DD-EE-FF";
      case "ipv4": return "192.168.1.40";
      case "url": return "http://host:port/path";
      case "path": return "C:\\path\\to\\file";
      default: return arg.required ? "required" : "optional";
    }
  });
  const placeholder = $derived(
    (arg.example !== undefined && arg.example !== null ? String(arg.example) : typePlaceholder) +
    (arg.help ? ` — ${arg.help}` : ""),
  );

  // The "Test" affordance for url args: one honest probe before the push.
  let probing = $state(false);
  let probe: UrlProbe | null = $state(null);
  async function testUrl() {
    const url = String(value ?? "").trim();
    if (!url || probing) return;
    probing = true;
    probe = null;
    try {
      probe = await api.probeUrl(url);
    } catch (e) {
      probe = { ok: false, status: null, latency_ms: 0, error: String(e) };
    } finally {
      probing = false;
    }
  }
  const probeLine = $derived.by(() => {
    if (!probe) return "";
    if (probe.ok) {
      return `✓ ${probe.status} in ${probe.latency_ms} ms${probe.content_type ? ` · ${probe.content_type}` : ""}`;
    }
    return probe.status != null
      ? `✗ HTTP ${probe.status} in ${probe.latency_ms} ms`
      : `✗ ${probe.error ?? "unreachable"}`;
  });
</script>

{#if arg.type === "enum"}
  <Select.Root
    type="single"
    value={(value as string) ?? ""}
    onValueChange={(v) => v && onvalue(v)}
  >
    <Select.Trigger size="sm" class="w-full">
      <Select.Value {placeholder} />
    </Select.Trigger>
    <Select.Content>
      {#each arg.options ?? [] as o (o)}
        <Select.Item value={o} label={o} />
      {/each}
    </Select.Content>
  </Select.Root>
{:else if arg.type === "bool"}
  <Select.Root
    type="single"
    value={value === undefined ? "" : String(value)}
    onValueChange={(v) => onvalue(v === "true")}
  >
    <Select.Trigger size="sm" class="w-full">
      <Select.Value {placeholder} />
    </Select.Trigger>
    <Select.Content>
      <Select.Item value="true" label="true" />
      <Select.Item value="false" label="false" />
    </Select.Content>
  </Select.Root>
{:else if arg.type === "int" || arg.type === "float"}
  <Input
    type="number"
    value={(value as number) ?? ""}
    {placeholder}
    min={arg.min}
    max={arg.max}
    onchange={(e) => onvalue(e.currentTarget.value === "" ? undefined : Number(e.currentTarget.value))}
  />
{:else if arg.type === "json"}
  <textarea
    class="min-h-[72px] w-full resize-y rounded-md border border-input bg-background px-2.5 py-2 font-mono text-[11px] leading-relaxed outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
    value={typeof value === "string" ? value : JSON.stringify(value ?? {}, null, 2)}
    {placeholder}
    onchange={(e) => {
      const raw = e.currentTarget.value;
      try {
        onvalue(JSON.parse(raw));
      } catch {
        onvalue(raw);
      }
    }}
  ></textarea>
{:else if arg.type === "url"}
  <div class="flex items-center gap-2">
    <Input value={(value as string) ?? ""} {placeholder} onchange={(e) => onvalue(e.currentTarget.value)} />
    <Button variant="ghost" size="sm" disabled={!value || probing} onclick={testUrl}>
      {probing ? "probing…" : "Test"}
    </Button>
  </div>
  {#if probeLine}
    <p class={cn("mt-1 text-xs", probe?.ok ? "text-success" : "text-destructive")}>{probeLine}</p>
  {/if}
{:else}
  <Input value={(value as string) ?? ""} {placeholder} onchange={(e) => onvalue(e.currentTarget.value)} />
{/if}
