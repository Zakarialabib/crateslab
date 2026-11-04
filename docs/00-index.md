# 00 · Index

The single entry point to the cratelab documentation set: what cratelab is, how the docs are organised, and which ones to read first.

> **cratelab docs** · SPDX `MIT` · security: this file must never contain credentials, tokens, private-repo content, or absolute user paths.

**Status:** reference · **Last verified:** 2026-11-04 · **Related:** [01-architecture](01-architecture.md), [02-manifest](02-manifest.md), [07-the-forge](07-the-forge.md), [13-roadmap](13-roadmap.md)

## 1. What cratelab is

cratelab is a Rust workspace plus a Tauri v2 desktop shell. Its premise: a Rust crate already declares its command surface in **typed enums** with doc comments, capability sets and error taxonomies. cratelab reads or generates a **manifest** of that surface, merges it with hand-written metadata, and *derives* everything downstream — the UI panels, the command console, the scenario playlists, the coverage heatmap, the conformance fuzzer. The crate stays the backend; cratelab is the bench it sits on.

There are **two planes** in this repository, and the documentation keeps them separate:

| Plane | What it is | Where it lives | Docs |
|---|---|---|---|
| **The crate bench** | The local, offline test-lab: manifests, the mock device fleet (`lab-mocks::MockFleet`), scenarios, coverage, conformance, and the headless `lab` CLI. | `crates/lab-*`, `shell/` | [01](01-architecture.md)–[06](06-crate-intelligence.md) |
| **The forge** | The renamed, unified agentic crate-evolution subsystem: it studies sources, builds a knowledge base, retrieves grounded evidence, renders scripted prompts, and emits reviewable evolution proposals. It replaces the old scattered `fleet/` control plane and never writes features unattended. | `crates/forge-*`, runtime home `forge/`, shell forge views | [07](07-the-forge.md)–[12](12-operations.md) |

The bench produces evidence; the forge reads that evidence and produces reviewable proposals. A human disposes every proposal. [01-architecture](01-architecture.md) shows how the two connect.

## 2. The doc set at a glance

| # | Document | Status | One line |
|---|---|---|---|
| 00 | [Index](00-index.md) | reference | You are here — orientation and reading paths. |
| 01 | [Architecture](01-architecture.md) | reference | The whole system: the two planes, the manifest spine, the invariants. |
| 02 | [The manifest contract](02-manifest.md) | reference | The authoritative manifest schema, validation rules and worked excerpts. |
| 03 | [CLI, scenarios, conformance, coverage](03-cli-and-workflows.md) | live | Every `lab` subcommand, the scenario format, conformance tiers, the coverage ledger. |
| 04 | [The shell: Tauri + Svelte](04-shell-and-ui.md) | live | The desktop app: IPC surface, manifest-derived UI, the forge screens. |
| 05 | [UI design spec](05-ui-wireframes.md) | design | The quota-first wireframes and design principles for the forge UI. |
| 06 | [Internal crate architecture map](06-crate-intelligence.md) | reference | The verified dependency + responsibility map of cratelab's own crates. |
| 07 | [The forge](07-the-forge.md) | reference | What the forge is: domains, the factory model, proposer/disposer separation, governance. |
| 08 | [Forge SDK and sidecar](08-forge-sdk-and-sidecar.md) | live | The `forge-core` / `forge-rag` / `forge-sidecar` crates and the stdio JSON-RPC surface. |
| 09 | [The workflow model](09-the-workflow-model.md) | reference | The seven-stage spine, the three gates, and the two embedded registries (workflows + tasks). |
| 10 | [Knowledge base and retrieval](10-knowledge-base-and-retrieval.md) | reference | Ingest, the BM25 index, evidence packets, and scripted prompt rendering. |
| 11 | [Providers and capacity](11-providers-and-capacity.md) | reference | Provider ports, free-tier rungs, and which models fit which free hardware. |
| 12 | [Operations](12-operations.md) | live | Day-to-day running: env, credentials, MOCK vs LIVE, quota safety, field test. |
| 13 | [Roadmap](13-roadmap.md) | design | Open findings, the ten concept specs, and the phased plan. |

**Status legend**

- **live** — describes behaviour that exists in the repository today.
- **reference** — a stable specification of a mechanism or map; accurate but not a runbook.
- **design** — a proposed direction; not yet implemented and labelled as such in the text.

## 3. Reading paths

- **Newcomer / evaluator** → [01-architecture](01-architecture.md) → [02-manifest](02-manifest.md) → [06-crate-intelligence](06-crate-intelligence.md) → [13-roadmap](13-roadmap.md).
- **Running the bench** → [03-cli-and-workflows](03-cli-and-workflows.md) → [02-manifest](02-manifest.md) → [04-shell-and-ui](04-shell-and-ui.md).
- **Building a frontend** → [04-shell-and-ui](04-shell-and-ui.md) → [05-ui-wireframes](05-ui-wireframes.md) → [02-manifest](02-manifest.md).
- **Operating the forge** → [07-the-forge](07-the-forge.md) → [08-forge-sdk-and-sidecar](08-forge-sdk-and-sidecar.md) → [12-operations](12-operations.md) → [09-the-workflow-model](09-the-workflow-model.md).
- **Planning work** → [13-roadmap](13-roadmap.md) → [06-crate-intelligence](06-crate-intelligence.md) → [11-providers-and-capacity](11-providers-and-capacity.md).

## 4. Diagram and asset sources

Diagrams that predate this set are kept as sources next to the docs and referenced by relative filename:

| Asset | Purpose |
|---|---|
| [architecture.html](architecture.html) / [architecture.png](architecture.png) | Whole-system architecture overview. |
| [lifecycle.html](lifecycle.html) / [manifest-lifecycle.png](manifest-lifecycle.png) | The manifest lifecycle (generate → author → merge → run). |
| [cover.html](cover.html) | Cover / title graphic. |
| [cratelab-blueprint.pdf](cratelab-blueprint.pdf) | The original design blueprint. |

## 5. Documentation contract

Every document in this set follows the same rules:

- **Brand.** The project is always **`cratelab`** (lowercase). The agentic crate-evolution subsystem is always **the forge** (it replaced the former "fleet" control plane; the lab's unrelated mock *device* fleet is still `lab-mocks::MockFleet`). The legacy mixed-case forms are retired.
- **Header.** Each file opens with `# NN · Title`, a one-sentence purpose, a security blockquote, and a metadata line (`Status` · `Last verified` · `Related`).
- **Verify only what is in the repo.** Claims that cannot be traced to a file in this repository are marked as design or dropped. Live provider quotas are account-specific and labelled as source claims, not assertions.
- **Security.** No document may contain credentials, tokens, private-repository content, or absolute user paths. Credentials are referenced **by name only**.
- **Cross-links** use exact sibling filenames (`[02-manifest](02-manifest.md)`).

## 6. Consolidation notes

- This set **replaces** the earlier, overlapping document collection: the playbook, technical-implementation build sheet, research notes, execution plan, standalone concept plan, and the loose `specs/` files are merged into the numbered documents above and are no longer maintained separately.
- The two subsystems (crate bench and forge) were previously documented under one undifferentiated pile; they are now split cleanly at 06/07.
- **The fleet became the forge.** The scattered `fleet/` control plane was replaced by the unified `crates/forge-*` engine; the provider subsystem is now always called **the forge**, its runtime home is `forge/`, and the old `07-fleet-overview` … `11-model-capacity` filenames no longer exist (see [07-the-forge](07-the-forge.md)). The lab's `lab-mocks::MockFleet` keeps its name — it is a mock *device* fleet, unrelated to this rename.
- Superseded naming (`cratesLab` / `crateslab`), stale stack references (React/npm), and dead helper scripts were dropped in favour of the verified current state (Svelte 5 / Bun, Tauri v2).
- Stale factual claims found during consolidation were corrected against the code (the crate dependency graph, the Bonsai capacity mapping, the tracker of open review findings) rather than carried forward.
