<script lang="ts">
  import type { ArgSpec } from "../types";

  let {
    arg,
    value,
    onvalue,
  }: { arg: ArgSpec; value: unknown; onvalue: (v: unknown) => void } = $props();

  const placeholder = $derived(arg.help ?? (arg.required ? "required" : "optional"));
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
{:else}
  <input value={(value as string) ?? ""} {placeholder} onchange={(e) => onvalue(e.currentTarget.value)} />
{/if}
