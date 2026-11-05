<script lang="ts">
  import { api, errorToLines } from "../api";
  import type { ScenarioReport } from "../types";
  import Timeline from "./Timeline.svelte";
  import { Button } from "$lib/components/ui/button/index.js";
  import * as Card from "$lib/components/ui/card/index.js";

  // Prefilled with the canonical playlist from `scenarios/tv-morning-loop.toml`.
  const DEFAULT = `[scenario]
name = "tv-morning-loop"
target = "mock"
on_fail = "abort"
vars = { vendor = "roku", ip = "192.168.1.40" }

[[steps]]
run = "tv.power.state"
args = { ip = "$vars.ip", vendor = "$vars.vendor" }
expect = "ok"

[[steps]]
run = "tv.power.wake"
args = { mac = "AA:BB:CC:DD:EE:FF" }
expect = "ok"
within_ms = 3000

[[steps]]
run = "tv.power.state"
args = { ip = "$vars.ip", vendor = "$vars.vendor" }
expect = { state = { key = "power", equals = "On" } }
poll = { attempts = 5, interval_ms = 100 }

[[steps]]
run = "tv.key.press"
args = { ip = "$vars.ip", vendor = "$vars.vendor", key = "home" }
expect = "ok"

[[steps]]
run = "tv.launch.app"
args = { ip = "$vars.ip", app_id = "netflix" }
expect = "ok"
`;

  let toml = $state(DEFAULT);
  let report = $state<ScenarioReport | null>(null);
  let err = $state<string[] | null>(null);
  let busy = $state(false);

  async function run(confirm: boolean) {
    err = null;
    report = null;
    busy = true;
    try {
      report = await api.runScenario(toml, confirm);
    } catch (e) {
      err = errorToLines(e as never);
    } finally {
      busy = false;
    }
  }
</script>

<h2 class="mb-2 text-[15px] font-semibold">Scenario composer</h2>
<p class="mb-3 text-[12.5px] text-muted-foreground">Paste or edit a playlist (TOML), run it, read the timeline. Rhai steps need the Rust engine.</p>

<div class="flex items-start gap-[18px]">
  <div class="flex-1">
    <Card.Root class="gap-3">
      <textarea
        class="min-h-[320px] w-full resize-y rounded-md border border-input bg-background px-2.5 py-2 font-mono text-[11px] leading-relaxed outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
        bind:value={toml}
      ></textarea>
      <div class="flex items-center gap-3">
        <Button disabled={busy} onclick={() => run(false)}>
          {busy ? "running…" : "Run"}
        </Button>
        <Button variant="ghost" disabled={busy} onclick={() => run(true)}>Run (allow destructive)</Button>
      </div>
    </Card.Root>
  </div>
  <div class="flex-1">
    {#if err}
      <pre class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 font-mono text-[11px] whitespace-pre-wrap text-destructive">{err.join("\n")}</pre>
    {/if}
    {#if report}<Timeline {report} />{/if}
  </div>
</div>
