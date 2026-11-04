/// Bonsai (Colab / Kaggle session) model selection.
///
/// No model is downloaded locally: the session serves it, so the picker is
/// driven by what `/models` reports plus a hardware-capability hint.
///
/// Numbers come from docs/11-providers-and-capacity.md (Colab tiers + GGUF
/// weight sizes). The practical ceiling on Colab is the ~120B MoE class on an
/// A100-80 / G4-96 GB box; 150B+ and 405B-class are not loadable (weights alone
/// exceed the card, and host RAM caps CPU offload).

export interface BonsaiTier {
  id: string;
  label: string;
  /** Usable VRAM in GB. */
  vramGb: number;
  hint: string;
  /** Models that fit this tier with room for context. */
  fits: string[];
  /** Models that load but leave too little VRAM for useful context. */
  traps?: string[];
  /** Models that cannot be loaded at all on this tier. */
  tooLarge?: string[];
  codingVerdict: string;
}

export const bonsaiTiers: BonsaiTier[] = [
  {
    id: "t4-15",
    label: "Colab free · T4 15 GB",
    vramGb: 15,
    hint: "Ternary Bonsai 27B is a T4 model — 7.2 GB of weights leaves 32K+ context.",
    fits: [
      "Ternary-Bonsai-2-27B PQ2_0 (7.21 GB)",
      "Ternary-Bonsai-2-9B",
      "Qwen2.5-Coder-14B Q4_K_M (~8.4-9.8 GB)",
      "Qwen2.5-Coder-7B Q4_K_M (~4.7 GB)",
    ],
    traps: ["gpt-oss-20b MXFP4 (12.8 GB — short context only)"],
    tooLarge: ["Qwen2.5-Coder-32B Q4_K_M", "Llama-3.3-70B", "gpt-oss-120b"],
    codingVerdict: "Yes for 27B: repo/doc analysis at 32K ctx (262K native).",
  },
  {
    id: "l4-22",
    label: "Colab Pro / PAYG · L4 22.5 GB",
    vramGb: 22.5,
    hint: "Best code-generation tier: 30B MoE / dense 32B at Q4_K_M with 16-32K ctx.",
    fits: ["Qwen3-Coder-30B-A3B Q4_K_M (17.3 GB)", "Qwen2.5-Coder-32B Q4_K_M (~19 GB)"],
    tooLarge: ["Llama-3.3-70B", "gpt-oss-120b"],
    codingVerdict: "Step up only when generation quality is the bottleneck.",
  },
  {
    id: "a100-40",
    label: "Colab Pro / Pro+ · A100 40 GB",
    vramGb: 40,
    hint: "Spend it on a high-quality 30-32B quant with long context, not on 70B.",
    fits: ["Qwen2.5-Coder-32B Q6_K-Q8_0 (32K+ ctx)", "Qwen3-Coder-30B-A3B Q6_K-Q8_0"],
    traps: ["Llama-3.3-70B Q4_K_M (~40 GB — loads, ~4K ctx only)"],
    tooLarge: ["gpt-oss-120b (~61 GB)"],
    codingVerdict: "70B-dense is a trap here: weights eat the card, context starves.",
  },
  {
    id: "a100-80",
    label: "Colab Pro+ · A100 80 GB / H100 80 GB",
    vramGb: 80,
    hint: "Where 70B dense becomes usable and 120B MoE fits tight; ~7.5 CU/h.",
    fits: ["Llama-3.3-70B Q4_K_M (~40 GB, 100K+ ctx)", "gpt-oss-120b MXFP4 (~61 GB, tight)"],
    traps: ["GLM-4.5-Air 106B Q4_K_M (~64-69 GB)", "Mistral-Large-2 123B Q4 (~68 GB)"],
    tooLarge: ["Llama-3.1-405B Q4_K_M (~230-256 GB)", "150B+ dense"],
    codingVerdict: "Reserve for the few tasks that justify the compute units.",
  },
  {
    id: "g4-96",
    label: "Colab G4 · RTX PRO 6000 ~96 GB",
    vramGb: 96,
    hint: "Top of Colab: 120B-class MoE with real context (~30 GB KV).",
    fits: ["gpt-oss-120b MXFP4 (32K+ ctx)", "GLM-4.5-Air 106B Q4_K_M", "Mistral-Large-2 123B Q4"],
    tooLarge: ["Llama-3.1-405B", "Qwen3-Coder-480B", "Kimi-K2"],
    codingVerdict: "Ceiling of the platform — still not 405B-class.",
  },
  {
    id: "kaggle-30",
    label: "Kaggle backup · 2xT4 ~30 GB",
    vramGb: 30,
    hint: "Same class as the L4 but slower and capped at 30 GPU-h/week.",
    fits: ["Ternary-Bonsai-2-27B PQ2_0 (one card)", "Qwen2.5-Coder-32B Q4_K_M (split)"],
    tooLarge: ["gpt-oss-120b"],
    codingVerdict: "Schedule after the Friday→Saturday CET reset.",
  },
  {
    id: "cpu",
    label: "CPU session",
    vramGb: 0,
    hint: "Too slow for audits — switch to a GPU session.",
    fits: [],
    tooLarge: ["anything above ~7B"],
    codingVerdict: "Not usable for crate audits.",
  },
  {
    id: "unknown",
    label: "Unknown hardware",
    vramGb: 0,
    hint: "Probe the session first; the served model list decides.",
    fits: [],
    codingVerdict: "Probe, then match the served models against the tiers above.",
  },
];

/** Coarse model-size class parsed from a served model id. */
function modelSizeB(modelId: string): number {
  const m = modelId.match(/-(\d+(?:\.\d+)?)b(?:$|[-_])/i);
  if (m) return parseFloat(m[1]);
  if (/-405b/i.test(modelId)) return 405;
  if (/-480b/i.test(modelId)) return 480;
  return 0;
}

/** Classify a served model against the detected tier. */
export function bonsaiModelFit(modelId: string, tierId: string): "fits" | "trap" | "too-large" | "unknown" {
  const tier = bonsaiTiers.find((t) => t.id === tierId);
  if (!tier || tier.id === "unknown") return "unknown";
  const size = modelSizeB(modelId);
  if (!size) return "unknown";
  // Ternary Bonsai 27B is the exception: ternary weights make it a 15 GB model.
  const isTernaryBonsai = /bonsai/i.test(modelId) && size <= 27;
  if (isTernaryBonsai) return tier.vramGb >= 15 ? "fits" : "too-large";
  if (size <= 32) return tier.vramGb >= 22.5 ? "fits" : tier.vramGb >= 15 ? "trap" : "too-large";
  if (size <= 70) return tier.vramGb >= 80 ? "fits" : tier.vramGb >= 40 ? "trap" : "too-large";
  if (size <= 123) return tier.vramGb >= 96 ? "fits" : tier.vramGb >= 80 ? "trap" : "too-large";
  return "too-large"; // 150B+ / 405B-class: not loadable on Colab
}

/** Pick the recommended model for a tier out of the models the session serves. */
export function recommendedBonsaiModel(models: string[], tierId: string): string | null {
  if (!models.length) return null;
  const tier = bonsaiTiers.find((t) => t.id === tierId);
  if (!tier) return models[0];
  const fitting = models.filter((m) => bonsaiModelFit(m, tier.id) === "fits");
  if (!fitting.length) return models[0];
  // Prefer the largest model that still fits: more capacity, still in budget.
  return fitting.sort((a, b) => modelSizeB(b) - modelSizeB(a))[0];
}
