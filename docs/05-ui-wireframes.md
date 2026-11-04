# 05 · UI design spec (quota-first)

The design rules and per-screen wireframes for the cratelab shell, and how far the shipped Svelte UI follows them.

> **cratelab docs** · SPDX `MIT` · security: this file must never contain credentials, tokens, private-repo content, or absolute user paths.

**Status:** design · **Last verified:** 2026-11-04 · **Related:** [00-index](00-index.md) · [04-shell-and-ui](04-shell-and-ui.md) · [09-the-workflow-model](09-the-workflow-model.md) · [11-providers-and-capacity](11-providers-and-capacity.md) · [12-operations](12-operations.md) · [architecture.html](architecture.html) · [architecture.png](architecture.png)

This document is the **quota-first** redesign of the earlier wireframes and supersedes both prior versions (see [Consolidation notes](#consolidation-notes)). Each wireframe carries a status tag: **[implemented]** (matches a shipped component — see [04-shell-and-ui](04-shell-and-ui.md) for the component map), **[partial]**, or **[proposed]** (design only, nothing in `shell/src` yet). Medium is ASCII / HTML+CSS with CSS bars — **no chart library, no framework** required for the wireframes themselves.

## 1 · Design principles

1. **Quota first.** Every screen answers "what is left today". Budgets are as prominent as results.
2. **Judgment is the product.** Merge / reject / redirect is first-class; it deserves its own surface.
3. **Nothing costs silently.** A session starts `MOCK`. A live run is enabled only when the rung is probed, the budget pre-check passes, and the forbidden-path scan is clean — then a confirm step shows the after-state.
4. **One screen per question** — and the screen title is that question.
5. **Provenance everywhere.** Every result links rung, model, prompt hash, RAG index version, and sources used.
6. **Small data, honest pixels.** Counts and CSS bars; tabular numerals; monospace for every id, path and hash.

## 2 · Shell — header + persistent nav

**[implemented — `TopHeader.svelte` + sidebar in `App.svelte`]**

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ cratelab · the crate bench   scope: tv-rs ▾   [MOCK]   rungs 7/16 probed       │
│ Project / Dashboard                          [Run ▾]  [Reports]               │
├────────────────────┬─────────────────────────────────────────────────────────┤
│ SIDEBAR (240px)    │ ACTIVE REPOSITORY  tv-rs                                │
│ Project            │ (target path · switch scope in the header)              │
│  Dashboard         │                                                         │
│ Automation         │                                                         │
│  Workflows   3 crates                                                       │
│  Integrations 7 configured                                                  │
│ Evidence           │                                                         │
│  Reports     9 runs│                                                         │
│ Settings           │                                                         │
│  Providers   7/16  │                                                         │
│ Lab tools          │                                                         │
│  Commands · Events · Servers · Limits · Coverage · Scenarios · Discovery     │
└────────────────────┴─────────────────────────────────────────────────────────┘
```

| Slot | What the code actually renders | Source |
|---|---|---|
| brand | `cratelab` (sidebar) · header markup currently says `cratesLab` — normalise to `cratelab` | `App.svelte`, `TopHeader.svelte` |
| scope | `<select>` of repositories with crate counts / "path unavailable" | `TopHeader.svelte` |
| env chip | `[MOCK]` default; `[LIVE]` when `backend !== "mock"` | `TopHeader.svelte` (`live` $derived) |
| readiness | `rungs {probed}/{providerTotal} probed`; click → Providers; tooltip: *"Configured and Probed are different claims"* | `TopHeader.svelte`, `App.svelte` |
| breadcrumb | `section / viewLabel` | `App.svelte` |
| nav | grouped sidebar with per-item counters (`Workflows 3 crates`, `Integrations 7 configured`, `Reports 9 runs`, `Providers 7/16`) | `App.svelte` `navCounts` |
| actions | `Run ▾` (up to 5 quick cases + "Open case workbench"), `Reports` | `TopHeader.svelte` |

> Design note vs the earlier spec: the readiness chip counts **`forge_status().providers.length`** rows (currently 16 rungs: `gemini, groq, cloudflare, nvidia, openrouter, zhipu, colab-bonsai, kaggle-bonsai, huggingface, kaggle, modal, qdrant, circleci, github-read, allam, modelscope`), **not** the "10 rungs" the first draft assumed. An earlier draft also proposed a plain row-nav; the shipped shell uses the grouped sidebar above.

## 3 · Dashboard — "Is the forge improving the crates, and what needs me?"

**[implemented — `DashboardView.svelte`, eyebrow `CRATELAB · PROJECT STATE & EVOLUTION`]**

```
┌─ CRATELAB · PROJECT STATE & EVOLUTION ───────────────────────── [Open reports] ┐
│ Is the forge improving the tv-rs crates, and what needs a human?               │
├─ METRICS (every card links to its surface) ───────────────────────────────────┤
│ Evidence runs 12 │ Reports produced 9 │ Providers ready 7/16 │ Target: tv-rs    │
├─ BUDGETS TODAY · FREE TIERS ────────┬─ EVOLUTION · RUNS PER DAY (UTC) ─────────┤
│ gemini     ▇▇▇░░░░  3 / 20          │ 2026-10-29 ▇▇▇▇▇ 5                      │
│ groq       ▇▇░░░░░  2 / 20          │ 2026-10-30 ▇▇▇ 3                        │
│ cloudflare ▇░░░░░░  1 / —           │ … last 14 days; bar width ∝ max         │
│ openrouter ░░░░░░░  — / 50          │                                         │
│ spend  €0.00 (always)  [invariant]  │                                         │
├─ WORKLOAD MIX (TASK) ───────────────┴─ COVERAGE · LAB ENGINE ──────────────────┤
│ rustdoc-audit ▇▇▇▇ 4 · research-summary ▇▇ 2 · issue-triage ▇ 1               │
│                                     │ tv-rs ▇▇▇▇░ 78% · cctv-rs ▇▇▇░░ 64%     │
├─ RECENT CASES (with provenance) ──────────────────────────────────────────────┤
│ 2026-11-04 09:12  rustdoc-audit · tv-rs (live) · groq · llama-3.1-8b          │
├─ PROJECT · CRATE CATALOG (tv-rs) ─────────────────────────────────────────────┤
│ [tv-rs][tv-rs-discovery][tv-rs-bluetooth]…  [Run a workflow][Reports][Providers]│
└──────────────────────────────────────────────────────────────────────────────┘
```

Rules the code enforces: budgets lead and come from the engine ledger (`usage.requests` × `daily_caps`); an **unknown cap renders `—`, never a guess** (`cap=null` → `"—"`); `spend €0.00 (always)` is a literal invariant row, not a statistic; every metric card navigates on click.

**Deltas from the earlier draft:** the shipped Dashboard has no "NEEDS YOU" slot (PR-merge / failed-run), its workload mix is keyed by **task** (not class A–E), and the forge panel is a **crate catalog** for the selected repository instead of a public-repo list. Those remain proposals — see §4.

## 4 · Queue — "What needs my judgment?"

**[proposed — no Queue view exists in `App.svelte`]**

The closest shipped thing is a **local staged case queue**: `ForgeView` stages up to `batch_size` (3) crate/task pairs into `localStorage["cratelab-forge-queue"]` ("Stage next batch of 3"), and `OpsDashboard` stages research/rustdoc batches into the same key. There is **no** merge / reject / failed-run queue, no diff review, and no one-click "retry on another rung".

```
┌─ AWAITING MERGE (3) ─────────────────────────────────────────────────────────┐
│ tv-rs-app #14 · ai:docs · rustdoc audit, 6 files · Bonsai-27B                │
│   diff +212 −48 · gates ✓ · scan ✓ · RAG v7 (6 hits) · brief B-2026-11-05    │
│   [Review diff] [Merge] [Reject] [Follow-up issue]                           │
├─ FAILED RUNS (1) ────────────────────────────────────────────────────────────┤
│ cctv-core · research-summary · groq 429 ×3 → circuit open, 18 min left       │
│   [Retry on gemini] [Discard]                                                │
├─ ROADMAP (from class C) ─────────────────────────────────────────────────────┤
│ cctv-agent #22 · ecosystem scan — 4 adoption notes [Open]                    │
└──────────────────────────────────────────────────────────────────────────────┘
```

Rule: every PR card carries the full provenance line (rung · model · RAG version · brief hash) so a reviewer merges **with** context; `[Merge]` always requires opening the diff first.

## 5 · Workbench — "What runs now, and what will it cost?"

**[implemented — `ForgeView.svelte`, eyebrow `LOCAL CONTROL PLANE`]**

```
┌─ CASE WORKBENCH ───────────────────────────────────────────── [DRY RUN DEFAULT]┐
│ Repository tv-rs (switch in header)  Crate [discovery ▾]  Task [Docs ▾]        │
│ Provider (rung) [colab-bonsai ▾ · credential present]  Model [bonsai-2-27b ▾]  │
├─ STEP RAIL (local evidence is steps 1–2, before any provider call) ───────────┤
│ 1 Scope one crate ✓ · 2 Collect source evidence ✓ · 3 Research context ▶ ·     │
│ 4 Review claims ○ · 5 Capture learning ○                                       │
├─ CASE DETAILS (optional issue text) ──────────────────────────────────────────┤
│ Learning notes carried into this workflow …  [useful ▾] [note] [Save note]     │
├─ KNOWLEDGE PIPELINE ──────────────────────────────────────────────────────────┤
│ 01 SOURCE signage docs → 02 INDEX local BM25 → 03 STORE Hugging Face →         │
│ 04 SEARCH Modal /search            [LOCAL CORPUS | CONNECTED]                  │
├─ DISPATCH POLICY (classes A–E decide the route) ──────────────────────────────┤
│ B Documentation → Gemini/Bonsai → Groq  · A Cleanup → Bonsai → Gemini → Groq  │
│ C Research → Gemini → NIM → Compound   · D Knowledge → FlashLite → Groq → Zhipu│
├─ SERVICE MAP (providers grouped) ─────────────────────────────────────────────┤
│ Inference · Model hosts · Knowledge & compute · Delivery & source             │
├─ [▶ Dry run]                       [Run live (review first)]                  │
│ gates: rung probed ✓ · model explicit ✓ · local evidence prepared ✓           │
├─ RECENT RUNS (local ledger) ──────────────────────────────────────────────────┤
└──────────────────────────────────────────────────────────────────────────────┘
```

Rules the code enforces: **local evidence is collected before any provider call** (dry run is local and free); the primary button is `Dry run`; `Run live (review first)` is disabled until the selected rung has a credential, `probe_ok === true`, **and** an explicit model. Pressing it opens an outbound-request modal that lists the files and source size sent, the eligible fallback route with remaining caps, and a routing/cost note ("calls may consume quota… verify every eligible model is free before continuing").

> Design note: the section titled **COST PREVIEW** in the earlier draft is realised as the run-live **modal's** "Files and size / Eligible route / Routing and cost" block — there is no separate inline cost-preview panel. `Dispatch policy` routes are labelled *"architecture defaults from the plan, not verified live availability"*.

## 6 · Research — "What can other ecosystems teach us?"

**[proposed — no Research view exists]**

The capability exists as the engine-authored task `cross-language-research` in `crates/forge-core/forge.tasks.toml` (6 steps: *Pick a capability · Find comparable SDKs · Extract the excellent parts · Diff against the crate · Write adoption notes · Capture learning*), selectable from the Workbench's Task dropdown (served to the shell through `forge_catalog`). There is **no** dedicated shortlist / scorecard / adoption-notes screen. The method and rubric are the task's own steps, under the workflow model in [09-the-workflow-model](09-the-workflow-model.md).

```
┌─ CAPABILITY ─────────────────────────────────────────────────────────────────┐
│ [device discovery ▾]  ecosystems: [x] Kotlin [x] C#/.NET [x] Go [ ] TS [ ] Py  │
├─ SHORTLIST (verified links only) ────────────────────────────────────────────┤
│ candidate        lang   license     last push  status                         │
│ …                Go     Apache-2.0  3d ago     ✓ verified · evidence pack     │
├─ SCORECARD (0–2 × 8 axes) ───────────────────────────────────────────────────┤
├─ ADOPTION NOTES  docs-missing 2 · features 1 · issues 0 · research 1          │
│ each note: effort S/M/L · impact A/B/C · lands-as PR/issue/roadmap            │
└──────────────────────────────────────── budget: compound 1/1 · gemini 1 ──────┘
```

## 7 · Reports — "What did we learn, and can I trust it?"

**[implemented — `ReportsView.svelte`, eyebrow `EVIDENCE · EVERY CASE LEAVES A REPORT`]**

```
┌─ EVIDENCE · EVERY CASE LEAVES A REPORT ──────────────────────────── [Refresh] ┐
│ Filter [ repo, crate, task, kind… ]                                          │
├─ LIST ─────────────────────────────────┬─ VIEWER ────────────────────────────┤
│ dry-run-tv-rs-discovery-…md            │ dry-run-…md                        │
│  [tv-rs] [rustdoc-audit] [dry]         │ …/forge/reports/…                  │
│  modified · 12 KB                      │ [Reveal in Explorer]              │
│ provider-audit-….md  [tv-rs][live]     │ <file bytes, unparaphrased>        │
└────────────────────────────────────────┴────────────────────────────────────┘
```

The viewer shows the **file bytes**, not a paraphrase. Implemented: filter, list with repo/task/kind badges, viewer, "Reveal in Explorer". **Proposed but not built:** the report-card provenance block (rung · prompt hash · RAG version · brief) and the verification count / `Promote to issue/PR` action; the shipped filter is a single free-text box, not the four dropdowns of the draft.

## 8 · Providers — "Which free AI is usable right now?"

**[implemented — `ForgeSettings.svelte` (credentials) + `OpsDashboard` "Rungs · one row each" table]**

```
┌─ FREE-TIER RUNGS · LOCAL CREDENTIALS ─────────────────── [Save settings] ─────┐
│ Local settings file  …/forge/.env   9/24 set · status only, values never shown │
├─ Inference providers ─────────────────────────────────────────────────────────┤
│ Gemini Api Key   Configured · Probed ✓   ••••••••  [Probe]  [How to get ↗]     │
│ Groq Api Key     Configured · unprobed   ••••••••  [Probe]  [How to get ↗]     │
│ Nvidia Api Key   Missing                 not set            [How to get ↗]     │
├─ Knowledge and search · Automation and source · Local repositories ───────────┤
├─ RUNGS (OpsDashboard table) ──────────────────────────────────────────────────┤
│ Provider            State       Today        Role           Action            │
│ Colab Bonsai        Probed ✓    —            Inference host  [Recheck]         │
│ Cloudflare …        Probed ✓    1 / 10       Inference+emb   [Recheck]         │
├─ LOCAL ACTIONS ───────────────────────────────────────────────────────────────┤
│ [Build index] [Validate index] [Check HF access] [Review upload] [Deploy] …   │
└───────────────────────────── Secrets stay local in forge/.env · spend €0.00 ──┘
```

States rendered: `Probed ✓` (cheap GET passed) · `Configured · unprobed` (key present — amber) · `Probe failed` · `Missing` (with setup link). `[Probe]` is **always a cheap GET** to a rung's identity/health endpoint — never a spend endpoint (`forge_probe_spec`; OpenRouter is filtered to `:free`). Secrets render only as `••••••••`; the input placeholder says *"Enter locally; never shown again"*, and the save path returns *"Secret values were not returned to the UI."* Upload/deploy are separate confirmed external actions.

> Delta from the draft: the "today / reset clock" columns come from the engine (`usage.requests`, `daily_caps`); an unknown cap shows `—`. Per-rung reset clocks are **proposed**.

## 9 · Settings — "What is wired, and what is guarded?"

**[partial]** — the shipped `ForgeSettings` view is the credentials/rungs screen of §8 (env path, masked fields, local actions). The guardrail / cadence / archive panels below are **proposed**.

```
┌─ SCOPE ──────────────────────────────────────────────────────────────────────┐
│ public repos (7) …  ·  private (build consumers only, never in RAG)          │
├─ GUARDRAILS (read-only, from forge.config.json) ─────────────────────────────┤  [proposed]
│ forbidden paths: .git · target · .circleci · .github · secrets · Cargo.toml… │
│ write rules: class C/D → issues only · every PR human-merged                 │
├─ ENV FILE  …/forge/.env ──────────────────────────────────────────────────────┤  [implemented – in §8]
│ loaded ✓ · N set · missing keys marked                                        │
├─ CADENCE ─────────────────────────────────────────────────────────────────────┘  [proposed]
│ heartbeat Mon 06:00 UTC · re-embed Sat · monthly sweep 1st                    │
└───────────────────────────────────────────────────────────────────────────────┘
```

## 10 · Bench screens (the `lab-engine` surface)

These are the manifest-driven tool screens — the origin of the whole project — and all four are **implemented**.

```
┌─ Commands console ──────────────────────────────── [implemented: CommandsView]┐
│ Command surface (10)                                                          │
│ ┌ tv.power.state [safe][command] ┐ ┌ tv.power.off [destructive][command] ┐    │
│ │ …                              │ │ …                                     │    │
│ Invoke tv.power.wake                                                          │
│  ip   [ipv4]  192.168.1.40                                                    │
│  mac  [mac ]  AA-BB-CC-DD-EE-FF          needs: mac          [Run]            │
│  (destructive → Confirm modal: "Refusals and outcomes still book coverage")   │
│ Device classes & honest capability sets │ Class │ Capabilities │ Source │     │
└───────────────────────────────────────────────────────────────────────────────┘

┌─ Coverage heatmap ──────────────────────── [implemented: CoverageHeatmap.svelte]┐
│ 78.0% of declared commands covered                                            │
│ Status │ Command │ Label │ Danger │ Buckets │ Runs │ Pass │ Refused │ Timeout …│
│  ●     │ tv.power.wake │ … │ safe │ 2 │ 4 │ 4 │ 0 │ 0 │ 0 │ 0 │               │
│ (a refusal colours amber with respect — it proves the API refused, by design) │
└───────────────────────────────────────────────────────────────────────────────┘

┌─ Scenarios composer ──────────────────────── [implemented: ScenariosView.svelte]┐
│ [scenario] name="tv-morning-loop" …  (TOML textarea prefilled)                │
│ [Run]  [Run (allow destructive)]        → Timeline: step · status · ms        │
└───────────────────────────────────────────────────────────────────────────────┘

┌─ Discovery ─────────────────────────────────── [implemented: DiscoveryView.svelte]┐
│ Workspace root [ .\ ]   [Scan]   → JSON workspace.json (members)              │
│ registry discovery rides the crates.io sparse index behind the `registry` feat.│
└───────────────────────────────────────────────────────────────────────────────┘
```

**Discovery-drift** (matching + drift against a registry version) exists in `lab-engine` but is **not surfaced in the UI** — the Discovery screen only runs `discover_local` and prints JSON. A drift panel is **proposed**.

## 11 · Bonsai model picker (corrected facts)

The session serves the model — nothing is downloaded locally; the picker is driven by what `/models` reports plus a hardware hint. **Correction to the first draft: the 27B model does *not* need an A100.** Its ternary weights are 7.21 GB (PQ2_0), which fits a **free T4 15 GB** — that is the whole Colab plan. Full capacity analysis: [11-providers-and-capacity](11-providers-and-capacity.md).

| Session hardware | Pick | Why |
|---|---|---|
| Colab free · T4 15 GB | `Ternary-Bonsai-2-27B` · **PQ2_0** (7.21 GB) | fits with 32K+ KV room — the default |
| T4 tight / OOM | `Ternary-Bonsai-2-27B` · **PTQ1_0** (5.95 GB) | smaller weights, more KV headroom |
| L4 · 22.5 GB | `Qwen3-Coder-30B-A3B` / `Qwen2.5-Coder-32B` Q4_K_M | best code-generation tier |
| A100 40 GB | `Qwen2.5-Coder-32B` Q6_K–Q8_0, long ctx | 70B-dense is a trap: weights eat the card, context starves |
| A100 80 GB / H100 | `Llama-3.3-70B` Q4_K_M; `gpt-oss-120b` tight | where 70B becomes usable |
| Colab G4 · RTX PRO 6000 ~96 GB | 120B-class MoE (`gpt-oss-120b`, GLM-4.5-Air, Mistral-Large-2) | ceiling of the platform — still not 405B |
| Kaggle backup · 2×T4 ~30 GB | `Ternary-Bonsai-2-27B` PQ2_0 (one card) | same class as the L4, capped at 30 GPU-h/week |
| CPU / unknown | avoid for audits; probe first | too slow / let the served `/models` decide |

Status: the tier data and helpers live in `shell/src/bonsai-models.ts` (`bonsaiTiers`, `bonsaiModelFit`, `recommendedBonsaiModel`), which already encodes the corrected mapping (`T4 → 27B fits`). **However, no view imports the module yet** — the picker is code-complete but not wired into a screen. Models that do not fit the detected tier are meant to render grey with `too large for this session`.

## 12 · Global interaction rules

**[implemented unless tagged]**

- A session starts `[MOCK]`; `[LIVE]` reflects a non-mock backend and runs are confirmed per request via the run-live modal.
- Every empty state says why and links the fix (the code uses `forge-lede` empty copy throughout); a `Missing` rung links `How to get ↗`.
- Status colours: green `--ok` = Probed ✓ · amber `--warn` = Configured-unprobed / near cap / circuit open · red `--bad` = failed gate or exhausted budget · grey = Missing (`styles.css`; `provider-state` classes `state-ready` / `state-pending` / `state-failed`).
- Monospace for ids, paths and hashes; tabular numerals.
- The UI **reads** state (ledger, settings) and never holds secrets: credentials live only in `forge/.env`; secret values are `null` over IPC. **[implemented]**
- Bars are plain `<div>`s (`.evo-bar`); **no chart library**. **[implemented]**
- Keyboard-first screen jumps (`g d / g q / g w / g r / g p / g s`) and `/`-to-filter. **[proposed]**

## 13 · Build order (smallest useful thing first)

1. Shell + Dashboard (reads the ledger) — the forge becomes observable on day 1. **[done]**
2. Queue + Providers — judgment and readiness. **[Providers done; Queue proposed]**
3. Workbench with cost review — the daily driver. **[done]**
4. Reports + Research — after the first cross-language cycle exists. **[Reports done; Research proposed]**

## Consolidation notes

- **Renamed the fleet surface to the forge.** The provider/evolution subsystem this file describes is now **the forge**: `FleetView`/`FleetSettings` are `ForgeView`/`ForgeSettings`, the queue key is `cratelab-forge-queue`, the empty-state class is `forge-lede`, the probe helper is `forge_probe_spec`, the runtime home is `forge/` (reports and `.env` live under `forge/`), and the workflow registry + task catalog live in `crates/forge-core/forge.workflows.toml` / `forge.tasks.toml` (served through `forge_catalog`). The lab's unrelated mock *device* fleet (`MockFleet`) keeps its name.
- **Merges UI wireframes v1 and v2.** The v2 quota-first framing (budgets lead; judgment is the product; nothing costs silently) **supersedes v1's** vanity-count / breadcrumb-only model; v1's keep-worthy parts (env chip, dry-run default, explicit provider, per-credential Probe, "How to get" links, hardware-hinted Bonsai picker, CSS bars, "every number is a link") are retained.
- **Corrected the Bonsai fact**: 27B PQ2_0 = 7.21 GB fits a free T4 — the old "27B needs an A100" statement is dropped. The capacity numbers now live in [11-providers-and-capacity](11-providers-and-capacity.md), and `shell/src/bonsai-models.ts` already maps `T4 → 27B fits`.
- **Brand normalised to `cratelab`** (lowercase) throughout; the `cratesLab` spelling in the source spec and in `TopHeader.svelte`'s markup is not repeated.
- **Aspirational vs implemented** is tagged per screen; the honest gaps are: no Queue view, no Research view, no cost-preview panel (folded into the run-live modal), no provenance/verification block in Reports, Settings guardrails/cadence panels, Discovery drift, keyboard shortcuts, and the Bonsai picker not yet wired to a view.
