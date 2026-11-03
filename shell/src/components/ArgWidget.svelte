<script lang="ts">
  import { api } from "../api";
  import type { ArgSpec, UrlProbe } from "../types";

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
  <select value={(value as string) ?? ""} onchange={(e) => onvalue(e.currentTarget.value)}>
    <option value="" disabled>{placeholder}</option>
    {#each arg.options ?? [] as o (o)}
      <option value={o}>{o}</option>
    {/each}
  </select>
{:else if arg.type === "bool"}
  <select value={value === undefined ? "" : String(value)} onchange={(e) => onvalue(e.currentTarget.value === "true")}>
    <option value="" disabled>{placeholder}</option>
    <option value="true">true</option>
    <option value="false">false</option>
  </select>
{:else if arg.type === "int" || arg.type === "float"}
  <input
    type="number"
    value={(value as number) ?? ""}
    {placeholder}
    min={arg.min}
    max={arg.max}
    onchange={(e) => onvalue(e.currentTarget.value === "" ? undefined : Number(e.currentTarget.value))}
  />
{:else if arg.type === "json"}
  <textarea
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
  <div class="url-row">
    <input value={(value as string) ?? ""} {placeholder} onchange={(e) => onvalue(e.currentTarget.value)} />
    <button class="ghost" disabled={!value || probing} onclick={testUrl}>
      {probing ? "probing…" : "Test"}
    </button>
  </div>
  {#if probeLine}
    <p class="probe-line" class:probe-ok={probe?.ok} style="margin:4px 0 0">{probeLine}</p>
  {/if}
{:else}
  <input value={(value as string) ?? ""} {placeholder} onchange={(e) => onvalue(e.currentTarget.value)} />
{/if}
