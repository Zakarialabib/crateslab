# 11 · Model capacity

Which models fit which free hardware, the VRAM arithmetic behind those fits, and how capacity constrains how the fleet batches work.

> **cratelab docs** · SPDX `MIT` · security: this file must never contain credentials, tokens, private-repo content, or absolute user paths.

**Status:** reference · **Last verified:** 2026-11-04 · **Related:** [00-index](00-index.md), [08-fleet-providers](08-fleet-providers.md), [10-fleet-workflows](10-fleet-workflows.md), [04-shell-and-ui](04-shell-and-ui.md)

## 1 · Why capacity matters to the fleet

The Bonsai rung is served by a **session** (a Colab or Kaggle notebook), not downloaded locally: the session serves whatever fits its accelerator, and the model picker is driven by what the session's `/models` reports plus a hardware-capability hint (`shell/src/bonsai-models.ts`). Capacity therefore decides which tasks can run on the Bonsai rung at all, and everything else falls to the API ladder in [08-fleet-providers](08-fleet-providers.md).

> **Correction (this doc supersedes the pre-correction mapping).** The old claim that "**Ternary-Bonsai-2-27B needs an A100**" is **wrong**. Ternary-Bonsai-2-27B **PQ2_0 is ~7.21 GB and fits a free T4 (15 GB usable)** — that fit is the entire reason the Colab plan works. The 27B size class is normally A100-only *for a dense FP16 model* (~54 GB); Bonsai is a ternary packing, so it is not. The pre-correction picker treated `-27b` as A100-only and mapped T4 → 9B; the shipped `shell/src/bonsai-models.ts` now maps **T4 → 27B (fits)**, **L4 / A100-40 → 30–32B**, **A100-80 / G4 → 70B / 120B**.

## 2 · The hardware Colab actually hands you

Google publishes no guaranteed GPU/RAM numbers; limits fluctuate by demand and account. Treat these as recent measurements, not entitlements — always verify a session with `!nvidia-smi` and `!free -g` before choosing a model.

| Tier / accelerator | Usable VRAM | Host RAM | Session & compute-unit (CU) burn |
|---|---|---|---|
| **Free — T4 (Turing)** | **15 GB** (marketed 16 GB; ~1 GB reserved for ECC) | ~12.7 GB | ≤ 12 h, pre-emptible; idle timeout unpublished |
| **Pro / PAYG — T4, L4 (Ada), A100 40 GB** | 15 / 22.5 / 40 GB | ~25–32 GB (High-RAM slider) | ≤ 24 h; 100 CU/mo (Pro) |
| **Pro+ — A100 40/80 GB, priority GPUs** | 40 / 80 GB | up to ~51–52 GB | ≤ 24 h incl. background execution; 600 CU/mo |
| **G4 — RTX PRO 6000 (Blackwell)** | **~96 GB** | High-RAM | ~8.71 CU/h |
| **H100 (Hopper, 80 GB)** — listed, rarely granted | 80 GB | High-RAM | rate not published |
| **Kaggle (backup host)** | 2× T4 ≈ 30 GB usable, or P100 16 GB | ~29 GB | 30 GPU-h/week, 12 h sessions |
| **CPU session** | 0 | — | not usable for crate audits |

Measured CU burn (2026-03): T4 **1.19 CU/h**, L4 **1.71**, A100-40 **5.40**, A100-80 **7.52**, G4 **8.71**. On a Google AI Pro plan (~200 CU/month) that is ~37 h of A100-40 or ~26 h of A100-80 — the true cost of "just use the big GPU".

Disk is temporary and varies widely (~40 GiB to ~110 GB depending on VM); check `!df -h` before pulling a 60 GB GGUF. Host RAM matters most for `llama.cpp` CPU-offload (`-ngl` spillover), not the file itself (GGUF is mmapped).

> **ToS note.** Colab's FAQ lists "bypassing the notebook UI to interact primarily via a web UI" and "file hosting / web service offerings" as disallowed on managed runtimes **without a positive CU balance**. Running `llama-server` behind a tunnel on a zero-balance account is that pattern; with a paid CU balance (Google AI Pro) it is permitted. Keep the **session-batch** pattern (boot → burn 2–4 h → save → stop), never a 24/7 endpoint.

## 3 · The memory arithmetic

```
total VRAM ≈ weights (quantised file size)
           + ~0.5–1.5 GB runtime overhead (CUDA ctx, buffers, compute)
           + KV cache
           + safety reserve (~1–2 GB)

KV bytes/token = 2 × layers × kv_heads × head_dim × dtype_bytes
```

Worked KV sizes (dense transformers, fp16 KV; halve with `--cache-type-k/v q8_0`):

| Model | KV bytes/token | 16K ctx | 32K ctx | 128K ctx |
|---|---|---|---|---|
| Qwen2.5-Coder-7B (28 L, 4 KV heads) | 28.7 KB | 0.47 GB | 0.94 GB | 3.8 GB |
| Qwen2.5-Coder-14B (48 L, 8 KV) | 98.3 KB | 1.6 GB | 3.2 GB | 12.9 GB |
| Qwen2.5-Coder-32B / Qwen3-30B-A3B (64 L, 8 KV) | 131 KB | 2.1 GB | 4.3 GB | 17.2 GB |
| Llama-3.x 70B (80 L, 8 KV, GQA) | 164 KB | 2.7 GB | 5.4 GB | 21.5 GB |

**Rule of thumb: pick a quant whose file size is 1–2 GB smaller than usable VRAM, then spend the rest on context.** Every GB spent on a bigger model is a GB not spent on context: 32K of usable context on a well-fitting model beats 4K on a model that barely loads.

**Why Bonsai is the outlier.** Ternary-Bonsai-2-27B is a *ternary* ({−1, 0, +1}) packing of a 27B dense model: **PTQ1_0 = 5.95 GB**, **PQ2_0 = 7.21 GB** (vs ~54 GB at FP16), with ~98.2 % benchmark retention and a 262,144-token native context. Its hybrid attention (~75 % linear / ~25 % full) keeps the KV cache per token far smaller than a dense 32B. That is why a 27B model normally sized for an A100 runs on a free T4. It requires the PrismML `llama.cpp` fork — stock `llama.cpp` cannot load PTQ1_0/PQ2_0.

## 4 · Capacity table

"Fits" = weights + runtime overhead + a useful KV budget inside usable VRAM. Approximate sizes only; verify against the actual GGUF.

| Model | Quant | ~Weights | Target hardware | Fits? |
|---|---|---|---|---|
| **Ternary-Bonsai-2-27B** | **PQ2_0** | **7.21 GB** | **free T4 15 GB (default)** | **✅ yes — 262K native ctx** |
| Ternary-Bonsai-2-27B | PTQ1_0 | 5.95 GB | free T4 15 GB | ✅ (more KV headroom) |
| Ternary-Bonsai-2-27B | PQ2_0 | 7.21 GB | L4 22.5 / A100-40 / A100-80 / G4 | ✅ (room to spare) |
| Qwen2.5-Coder-14B-Instruct | Q4_K_M | ~8.4–9.8 GB | free T4 15 GB | ✅ (~32K with q8_0 KV) |
| Qwen2.5-Coder-7B-Instruct | Q4_K_M | ~4.7 GB | free T4 15 GB | ✅ (fast triage/draft) |
| gpt-oss-20b | MXFP4 | 12.8 GB | free T4 15 GB | ⚠️ tight (~14.6 GB all-in → short ctx) |
| Qwen3-Coder-30B-A3B-Instruct | Q4_K_M | 17.3 GB | L4 22.5 GB | ✅ (~22 GB @16K; best at this tier) |
| Qwen2.5-Coder-32B-Instruct | Q4_K_M | ~18.6 GB | L4 22.5 GB | ✅ tight (16–24K ctx) |
| Qwen2.5-Coder-32B / Qwen3-Coder-30B-A3B | Q6_K / Q8_0 | 26–31 GB | A100-40 | ✅ (32K+ ctx) |
| Llama-3.3-70B-Instruct | Q4_K_M | ~40 GB | A100-40 | ⚠️ trap (weights eat the card → ~4K ctx) |
| Llama-3.3-70B-Instruct | Q3_K_S | ~30 GB | A100-40 | ⚠️ ~8–16K ctx, degraded |
| Llama-3.3-70B-Instruct | Q4_K_M | ~40 GB | A100-80 / H100 | ✅ (~100K+ ctx) |
| gpt-oss-120b | MXFP4 | ~61–63 GB | A100-80 / H100 | ✅ tight (~15 GB KV) |
| GLM-4.5-Air 106B (A12B MoE) | Q4_K_M | ~64–69 GB | A100-80 / H100 | ✅ tight (8–32K) |
| Mistral-Large-2 123B | Q4 | ~68 GB | A100-80 / H100 | ✅ tight |
| gpt-oss-120b | MXFP4 | ~61 GB | G4 96 GB | ✅ (~30 GB KV → 32K+ ctx) |
| GLM-4.5-Air 106B | Q4_K_M | ~64–69 GB | G4 96 GB | ✅ comfortable |
| Mistral-Large-2 123B | Q4 | ~68 GB | G4 96 GB | ✅ |
| Llama-3.1-405B | Q4_K_M | ~230–256 GB | any Colab tier | ❌ unloadable |
| 150B+ dense / 405B-class | any usable quant | exceeds card + host RAM | any Colab tier | ❌ unloadable |

Kaggle (backup): Qwen2.5-Coder-32B Q4_K_M splits across 2× T4; Bonsai 27B fits one card, leaving the second free for embeddings. **30 GPU-h/week** is the binding constraint — schedule heavy jobs right after the Friday→Saturday CET reset.

## 5 · The ceiling, stated plainly

| Question | Answer |
|---|---|
| Can Colab serve **Bonsai 27B** for coding? | **Yes — on the free T4.** 7.21 GB (PQ2_0) / 5.95 GB (PTQ1_0); 262K context; ~98 % quality retention. This is the whole reason the Bonsai plan works. |
| Is the ceiling 50B+ or 150B+? | **~120B-class MoE** on A100-80 / G4-96 (gpt-oss-120b, GLM-4.5-Air 106B, 123B Mistral-Large-2). **Not 150B+.** 405B-class is unreachable on every Colab tier. |
| Best *usable* dense ceiling | 70B at Q4_K_M with long context, on A100-80 or better. On A100-40 a 70B loads but has ~4K context — unusable for repo analysis. |
| Hard wall | Weights + KV + overhead ≤ usable VRAM, and CPU offload capped by ~12.7 GB (free) / ~51 GB (Pro+ High-RAM) host RAM. 405B Q4_K_M (~230–256 GB) fails both. |

## 6 · How capacity constrains the fleet

- **Default assignment.** Classes A/B/D/E (cleanup, docs, knowledge, release hygiene) run on **Bonsai 27B PQ2_0 on whatever T4 you get**, at `-c 32768` with `--cache-type-k/v q8_0`, `--parallel 2`, and PTQ1_0 as the OOM fallback — zero marginal cost. This is the tier that keeps the fleet free ([10-fleet-workflows](10-fleet-workflows.md)).
- **When generation quality is the bottleneck** (a rare class-A rewrite, a hard borrow-checker fix), step up to Qwen3-Coder-30B-A3B Q4_K_M on an L4, or Qwen2.5-Coder-32B on an A100-40.
- **Rare justified bursts** (gpt-oss-120b MXFP4 on A100-80/G4) cost ~7.52 CU/h → ~26 h/month out of a 200 CU allowance, plus a 60 GB download per session unless cached in Drive/Kaggle Datasets — so batch them, do not loop on them.
- **Batching.** Because KV cost scales with context and a session is time-boxed, briefs are cache-first and context-budgeted: whole-crate analysis at 16–32K per call, a small number of parallel streams, ~300–800 tokens of reasoning per call. Tasks that do not fit are routed to the API ladder ([08-fleet-providers](08-fleet-providers.md)) rather than forced onto the session.
- **Do not plan around** 70B-dense on A100-40, 150B+, or 405B-class — they are context-starved or unloadable.

## 7 · The picker mapping (verified in code)

`shell/src/bonsai-models.ts` exposes tiers with usable VRAM and the models that fit, and classifies a served model id against the detected tier (`fits` / `trap` / `too-large`). The verified mapping is:

| Tier | Usable VRAM | Recommended |
|---|---|---|
| Colab free · T4 | 15 GB | Bonsai 27B PQ2_0; Qwen2.5-Coder-14B/7B |
| Colab Pro/PAYG · L4 | 22.5 GB | Qwen3-Coder-30B-A3B or Qwen2.5-Coder-32B Q4_K_M |
| Colab Pro/Pro+ · A100-40 | 40 GB | 30–32B at Q6_K/Q8_0 (70B is a trap) |
| Colab Pro+ · A100-80 / H100 | 80 GB | Llama-3.3-70B Q4_K_M; gpt-oss-120b tight |
| Colab G4 · RTX PRO 6000 | 96 GB | gpt-oss-120b; GLM-4.5-Air 106B; Mistral-Large-2 123B |
| Kaggle backup · 2× T4 | 30 GB | Bonsai 27B (one card); 32B split |

The ternary-Bonsai exception is handled explicitly: a `bonsai` model id ≤ 27B is treated as a **15 GB** model, so it fits any tier with ≥ 15 GB — this is the corrected behaviour described in §1.

## Consolidation notes

- **Corrected the pre-correction Bonsai mapping.** Stated up front (§1) that Bonsai 27B PQ2_0 ≈ 7.21 GB **fits a free T4**, and that the old "27B needs an A100" claim is wrong; the ternary pack is why (dense FP16 27B ≈ 54 GB is A100-class).
- **Re-tabulated capacity into one model × quant × ~VRAM × hardware × fit table** (§4), replacing the source's per-tier prose blocks, and kept the tier verdicts as a compact ceiling table (§5).
- **Verified the agent's own picker** against `shell/src/bonsai-models.ts`: the shipped file already maps T4 → 27B; the older doc's "currently maps T4 → 9B" note is stale and dropped.
- **Added §6** to connect capacity to fleet batching and the provider ladder (links 08/10), and **reduced** the source's long source-URL list to the load-bearing facts.
- **Dropped** the `docs/specs/colab-model-capacity.md` deprecation-style "§5.5 picker is wrong" instruction text (the code was already corrected) and the elaborate CU-burn sourcing notes, keeping only the measured CU rates.
