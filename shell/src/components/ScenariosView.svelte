<script lang="ts">
  import { api, errorToLines } from "../api";
  import type { ScenarioReport } from "../types";
  import Timeline from "./Timeline.svelte";

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

<h2>Scenario composer</h2>
<p class="sub">Paste or edit a playlist (TOML), run it, read the timeline. Rhai steps need the Rust engine.</p>

<div style="display:flex;gap:18px;align-items:flex-start">
  <div style="flex:1">
    <div class="form">
      <textarea style="min-height:320px" bind:value={toml}></textarea>
      <div class="run-row">
        <button class="primary" disabled={busy} onclick={() => run(false)}>
          {busy ? "running…" : "Run"}
        </button>
        <button class="ghost" disabled={busy} onclick={() => run(true)}>Run (allow destructive)</button>
      </div>
    </div>
  </div>
  <div style="flex:1">
    {#if err}<div class="result err">{err.join("\n")}</div>{/if}
    {#if report}<Timeline {report} />{/if}
  </div>
</div>
