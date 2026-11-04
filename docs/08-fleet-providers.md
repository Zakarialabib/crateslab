# 08 · Fleet providers

Reference for every provider rung the fleet can use: what each offers, its free-tier limits, how cratelab uses it, and where its credential guide lives.

> **cratelab docs** · SPDX `MIT` · security: this file must never contain credentials, tokens, private-repo content, or absolute user paths.

**Status:** live · **Last verified:** 2026-11-04 · **Related:** [00-index](00-index.md), [04-shell-and-ui](04-shell-and-ui.md), [07-fleet-overview](07-fleet-overview.md), [09-fleet-rag](09-fleet-rag.md), [12-operations](12-operations.md)

## 1. Roster

The fleet is ordered cheapest-first. Session GPU hosts lead (a free GPU is the cheapest token on earth), then free API executors, then windfall quotas. Each rung also carries a default fallback position; ordering and routing live in [07-fleet-overview](07-fleet-overview.md) §6.

Credential guides are the setup links recorded in `fleet/config.json` → `credential_guides`. Where a rung has no entry there, it is marked **no guide configured** — do not invent one.

## 2. Session GPU hosts

### 2.1 Colab — Bonsai 27B host (primary)

- **Offers:** a free T4 GPU session on which the fleet runs Ternary-Bonsai-2-27B (a ternary quantisation of Qwen3.8-27B: 5.95 GB at PTQ1_0 / 7.21 GB at PQ2_0, 262K native context, Apache-2.0) served by the PrismML fork of `llama.cpp`. With an existing Google AI Pro plan: monthly compute units, priority GPUs, longer runtimes.
- **Free-tier constraints:** free T4 sessions, ~12 h cap and a shorter inactivity disconnect; the design is *session-batch* (boot → burn a few hours → save → stop), never a 24/7 endpoint.
- **How cratelab uses it:** primary model host for crate-grounded work (whole-crate analysis, doc rewrites, release hygiene). The session endpoint is configured locally as `BONSAI_URL` and only joins the fleet after it is configured, probed and confirmed in a reviewed request.
- **Credential guide:** `BONSAI_URL` → `https://colab.research.google.com/`.

### 2.2 Kaggle — embedder + backup host

- **Offers:** GPU notebooks (T4×2 / P100) and a weekly GPU-hour allowance; free private datasets for caching model assets.
- **Free-tier constraints:** GPU-hours are metered weekly; CPU-first jobs are the default for the crate-intelligence evaluation. Quotas vary by account and date — check the Kaggle settings page before scheduling.
- **How cratelab uses it:** the optional embedding home turf (dense retrieval experiments) and the backup Bonsai host; hosts the private crate-intelligence evaluation job that compares dense retrieval with BM25.
- **Credential guide:** `KAGGLE_BONSAI_URL` → `https://www.kaggle.com/code`; `KAGGLE_KERNEL_ID` → `https://www.kaggle.com/code`; `KAGGLE_API_KEY` → `https://www.kaggle.com/settings`.

## 3. Free API executors

### 3.1 Groq — primary API executor

- **Offers:** fast OpenAI-compatible inference on open models. Executors used by the fleet: `openai/gpt-oss-120b` (deep analysis, doc rewrites, research synthesis), `openai/gpt-oss-20b` (fast loop: triage, summarise, classify), `qwen/qwen3.8-27b` (consistency checks against Bonsai's base family), and `allam-2-7b` (see §4). A free `llama-prompt-guard-2-22m / 86m` acts as an inbound prompt-injection filter, and `groq/compound` / `compound-mini` provide an agentic multi-step executor.
- **Free-tier limits (verified 2026-11-04):** `openai/gpt-oss-120b` = **30 RPM · 1,000 RPD · 8,000 TPM · 200,000 TPD** — *verified* against the operator's Groq console snapshot and the official rate-limits documentation, corroborated by two independent trackers. **Cached tokens do not count toward rate limits** (byte-stable system prompts are effectively free quota), and limits are enforced at the **organization** level, with `429` plus `retry-after` headers. The same console snapshot shows `groq/compound` at 30 RPM · 250 RPD · 70K TPM.
- **How cratelab uses it:** the default executor for deep-analysis and documentation runs, with `groq/compound` reserved as a capped deep-dive (one per day by policy). Because `gpt-oss-120b` is token-limited (8K TPM) rather than request-limited, the fleet map-reduces long inputs before sending.
- **Credential guide:** `GROQ_API_KEY` → `https://console.groq.com/keys`.

### 3.2 Gemini API — volume executor

- **Offers:** free rows for a family of Flash models (`gemini-3.8-flash`, `3.6-flash`, `3.5-flash-lite`, `3.1-flash-lite`) with a large context window on Flash.
- **Free-tier constraints:** the official rate-limit docs publish **no universal RPD**; the widely quoted ~1,500 RPD is a **soft cap** used only as a router budget input — per-project limits are visible only in AI Studio, and RPD resets at midnight Pacific. Free-tier prompts may be used to improve the products outside some regions; the fleet sends public-crate knowledge only.
- **How cratelab uses it:** the volume executor for long-context research digests and high-count fast-loop work.
- **Credential guide:** `GEMINI_API_KEY` → `https://aistudio.google.com/apikey` (free, no card — do not link billing).

### 3.3 Cloudflare Workers AI — fail-safe rung + query embeddings

- **Offers:** a per-day Neuron allowance over a large model catalogue, including `@cf/openai/gpt-oss-120b/20b`, `@cf/meta/llama-3.3-70b-instruct-fp8-fast`, and `@cf/baai/bge-m3` embeddings — all reachable through the official OpenAI-compatible endpoint (`/ai/v1`, supports `/chat/completions` and `/embeddings`), so no custom adapter is needed.
- **Free-tier constraints:** 10,000 Neurons/day, resets 00:00 UTC. The free plan **fails rather than bills** — exceeding the allowance requires an explicit paid signup, so an exhausted quota is a hard stop, not a charge.
- **How cratelab uses it:** the fail-safe rung at the bottom of the ladder and the query-embedding path that keeps vector work inside the Neuron pool. Model IDs churn, so the exact ID is verified against the catalogue before use.
- **Credential guide:** `CF_ACCOUNT_ID` → `https://dash.cloudflare.com/`; `CF_API_TOKEN` → `https://dash.cloudflare.com/profile/api-tokens` (Workers AI Edit token).

### 3.4 NVIDIA NIM — big-context overflow

- **Offers:** an OpenAI-compatible endpoint with many open models and up to ~1M-token context.
- **Free-tier constraints:** limits are not published and model IDs drift, so the rung is never load-bearing; it is used only when a wider context is genuinely required.
- **How cratelab uses it:** overflow executor for whole-workspace research digests.
- **Credential guide:** `NVIDIA_API_KEY` → `https://build.nvidia.com/`.

### 3.5 OpenRouter — diversity executor

- **Offers:** a `:free` model pool plus an `openrouter/free` auto-router, giving model diversity without a paid plan.
- **Free-tier constraints:** 20 RPM and a hard 50 requests/day without a purchase; **failed attempts count** toward the daily cap, so the router caps retries at one for this rung.
- **How cratelab uses it:** an occasional diversity sprinkle, never a primary path.
- **Credential guide:** `OPENROUTER_API_KEY` → `https://openrouter.ai/settings/keys`.

### 3.6 Zhipu (Z.ai / bigmodel.cn) — bench

- **Offers:** the `glm-*-flash` family (e.g. `glm-4.7-flash`) on a permanent free tier with a large context window.
- **Free-tier constraints:** no published daily figure; an observed ceiling on the order of ~1k requests/day.
- **How cratelab uses it:** bench warmers at the bottom of the ladder for low-risk volume work.
- **Credential guide:** `ZHIPU_API_KEY` → `https://open.bigmodel.cn/usercenter/apikeys`.

### 3.7 ModelScope / SiliconFlow — bench

- **Offers:** ModelScope offers free daily API-inference calls for signed-in users; SiliconFlow offers a free model set (e.g. `Qwen/Qwen3-8B`).
- **Free-tier constraints:** ModelScope's daily call count should be confirmed in-console; SiliconFlow requires mainland-phone SMS and identity verification. Both are bench-only rungs.
- **How cratelab uses it:** bench warmers below Zhipu.
- **Credential guide:** `MODELSCOPE_TOKEN` → `https://modelscope.cn/my/myaccesstoken`. **SiliconFlow: no guide configured** in `fleet/config.json`.

## 4. Optional and windfall rungs

### 4.1 allam-2-7b (via the Groq console)

- **Offers / limits:** present on the operator's Groq console snapshot (2026-11-04) at 30 RPM · 7K RPD · 500K TPD — the largest request budget on the account. It is SDAIA's ALLaM model, **not** a Llama-family model.
- **Caveat:** it does not appear in Groq's general rate-limit documentation, so it may be withdrawn; the rung is optional and never load-bearing (Arabic-leaning, trivial bulk only).

### 4.2 Alibaba Model Studio / BytePlus (Doubao)

- **Offers:** per-model new-user free quotas.
- **How cratelab uses it:** **windfall only** — used opportunistically, never scheduled around.
- **Credential guide:** **no guide configured** in `fleet/config.json`.

### 4.3 Hugging Face (storage, not inference)

- **Offers:** GGUF hosting and free **private** dataset repositories — the fleet's knowledge store of record.
- **How cratelab uses it:** stores the pinned corpus datasets the retrieval path reads. Inference Providers are demoted to storage only.
- **Credential guide:** `HF_TOKEN` → `https://huggingface.co/settings/tokens`; `HF_READ_TOKEN` → `https://huggingface.co/settings/tokens`; `HF_DATASET_REPO` → `https://huggingface.co/new-dataset`; `HF_CRATES_DATASET_REPO` → `https://huggingface.co/new-dataset`.

### 4.4 Modal and CircleCI (glue, non-LLM)

- **Modal** hosts the authenticated, read-only retrieval search apps and the dispatch glue; it never burns inference quota. Guide: `MODAL_TOKEN_ID` / `MODAL_TOKEN_SECRET` → `https://modal.com/docs/cli/latest/token`; `MODAL_SEARCH_URL` → `https://modal.com/docs/cli/latest/deploy`.
- **CircleCI** runs the fmt/clippy/test/rustdoc gates and the weekly heartbeat. Guide: `CIRCLECI_TOKEN` → `https://app.circleci.com/settings/user/tokens`; `CIRCLECI_PROJECT_SLUGS` → `https://app.circleci.com/projects`.

### 4.5 Qdrant Cloud (optional serving layer)

- **Offers:** a free forever single-node cluster intended to hold roughly one million 768-dim vectors.
- **How cratelab uses it:** an **optional** retrieval serving layer, not part of the active lexical pilot (see [09-fleet-rag](09-fleet-rag.md)).
- **Credential guide:** `QDRANT_URL` / `QDRANT_API_KEY` → `https://cloud.qdrant.io/`.

## 5. Capability and limit matrix

| Provider | Free limits | Best-fit task |
|---|---|---|
| Colab (Bonsai 27B) | free T4 session, ~12 h cap | whole-crate analysis, doc rewrites, release hygiene |
| Kaggle | metered weekly GPU-hours | embedding experiments, backup host, retrieval eval |
| Groq `gpt-oss-120b` | 30 RPM · 1K RPD · 8K TPM · 200K TPD (verified); cached tokens free | deep analysis, research synthesis |
| Groq `gpt-oss-20b` | same class as 120b | fast loop: triage, summarise, classify |
| Groq `qwen3.8-27b` | same class | consistency checks vs Bonsai's base family |
| Groq `compound(-mini)` | 30 RPM · 250 RPD · 70K TPM (verified) | one agentic deep-dive/day |
| Gemini API | soft cap ~1,500 RPD (AI Studio is truth) | volume: long-context digests |
| Cloudflare Workers AI | 10,000 Neurons/day, resets 00:00 UTC; fails, never bills | fail-safe rung + query embeddings |
| NVIDIA NIM | unpublished, IDs drift | big-context overflow |
| OpenRouter | 20 RPM · 50/day; failed attempts count | diversity |
| Zhipu `glm-4.7-flash` | permanent free, ~1k/day observed | bench |
| ModelScope | free daily calls (verify console) | bench |
| SiliconFlow | free set, CN phone required | bench |
| allam-2-7b | 30 RPM · 7K RPD · 500K TPD (console) | trivial bulk (optional) |
| Alibaba / BytePlus | new-user quotas | windfall overflow |
| Hugging Face | free private datasets | knowledge store |
| Modal | free-tier credits + cron | RAG search + dispatch |
| CircleCI | free-credits tier | gates + heartbeat |
| Qdrant Cloud | free single node | optional serving layer |

## 6. Circuit breaker and "fails, not bills"

Two safety behaviours keep a free fleet from turning into a cost or a hang:

- **Circuit breaker.** The provider router counts consecutive failures per rung; after **three** consecutive failures a rung is tripped into a **30-minute cooldown**, during which it is skipped. Quota walls are treated as explicit signals (`HTTP 402/403/429`), and the router spends at most **80 %** of any published daily budget before moving on. The router only ever calls providers that have been configured, probed and confirmed.
- **Fails, not bills.** Cloudflare's free plan rejects requests once the Neuron allowance is exhausted rather than silently charging — it is the fleet's designated fail-safe rung for exactly this property. The general rule is that the fleet never schedules paid usage; free-tier terms and quotas are account-specific and must be checked in each provider's dashboard.

## 7. Credentials

Credentials live only in the ignored env file (`fleet/.env`, per `fleet/README.md`); the earlier env-file path in the build sheet is superseded. Secret values are never returned to the UI, and an existing value is preserved when a secret input is left blank. Provider fields surface their official setup link (the `credential_guides` above) when missing. HF CLI and Modal CLI profiles stay local; secret transfer to Modal happens only in the confirmed deploy flow. See [04-shell-and-ui](04-shell-and-ui.md) for the settings surfaces and [12-operations](12-operations.md) for operational handling.

## Consolidation notes

- Consolidated the two provider lists (playbook §4 role matrix and build sheet §1 verified roster) into one rung-per-subsection layout plus a single capability/limit matrix.
- **Verified** the Groq claim as required: `openai/gpt-oss-120b` = 30 RPM / 1,000 RPD / 8,000 TPM / 200,000 TPD, with cached tokens not counted and organization-level enforcement — corroborated by the operator console snapshot, the official rate-limit docs and two independent trackers.
- Kept the circuit-breaker (3 consecutive failures → 30-minute cooldown; 402/403/429 walls; 80 % budget ceiling) and the Cloudflare "fails, not bills" property from the build sheet.
- Corrected the credential location to the ignored `fleet/.env` (the build sheet's earlier env-file path is stale) and restricted provider setup links to those actually present in `fleet/config.json`.
- Marked SiliconFlow, Alibaba and BytePlus as **no guide configured** because `fleet/config.json` has no `credential_guides` entry for them, rather than inventing one.
- **Dropped** all per-key secret examples and absolute paths; credentials are referred to by name only.
