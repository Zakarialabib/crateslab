"""Private Kaggle job: compare dense retrieval with BM25 for CrateLab crates.

Inputs are pinned HF Kaggle Secrets. Outputs are evaluation metrics and
query-level provenance; this job never uploads or publishes its output.
"""
import json
import math
import re
import subprocess
import sys
import time
from collections import Counter
from pathlib import Path

import numpy as np
from huggingface_hub import hf_hub_download

try:
    from kaggle_secrets import UserSecretsClient
except ImportError as exc:
    raise RuntimeError("Run this as a Kaggle Notebook with HF_READ_TOKEN, HF_CRATES_DATASET_REPO, and HF_CRATES_DATASET_REVISION secrets configured.") from exc

secrets = UserSecretsClient()
hf_token = secrets.get_secret("HF_READ_TOKEN")
repo_id = secrets.get_secret("HF_CRATES_DATASET_REPO")
revision = secrets.get_secret("HF_CRATES_DATASET_REVISION")

subprocess.run([sys.executable, "-m", "pip", "install", "--quiet", "sentence-transformers"], check=True)
from sentence_transformers import SentenceTransformer

index_path = Path(hf_hub_download(repo_id=repo_id, repo_type="dataset", filename="index.jsonl", revision=revision, token=hf_token))
rows = [json.loads(line) for line in index_path.open(encoding="utf-8") if line.strip()]
if not rows:
    raise RuntimeError("Pinned crate intelligence index is empty")

queries = [
    {"query": "shared error kinds remedies severity and result type", "expected": ["lab-errors"]},
    {"query": "manifest schema commands arguments limits and authored metadata merge", "expected": ["lab-manifest", "lab-generate"]},
    {"query": "parse typed Rust command enum doc comments into generated manifest skeleton", "expected": ["lab-generate"]},
    {"query": "mock command sink virtual devices deterministic chaos capability refusal rate limits", "expected": ["lab-mocks"]},
    {"query": "scenario TOML steps polling timeouts on fail and Rhai sandbox", "expected": ["lab-scenarios"]},
    {"query": "coverage ledger heatmap command outcomes markdown JSON", "expected": ["lab-coverage"]},
    {"query": "boundary hostile values property cases zero panic invariants cap", "expected": ["lab-conformance"]},
    {"query": "workspace discovery manifest drift CLI orchestrates mock scenario coverage", "expected": ["lab-engine"]},
]

# One known crate per query keeps both retrievers under the same isolation rule.
queries = [{**item, "crate_id": item["expected"][0]} for item in queries]

def tokens(text):
    return re.findall(r"[a-zA-Z0-9_]+", text.lower())

docs = [tokens(row.get("text", "")) for row in rows]
freqs = [Counter(doc) for doc in docs]
lengths = [len(doc) for doc in docs]
avg_len = sum(lengths) / max(1, len(lengths))
doc_freq = Counter(term for freq in freqs for term in freq)

def bm25(query, crate_id, k=5):
    terms = set(tokens(query))
    scored = []
    for i, (row, term_freq, length) in enumerate(zip(rows, freqs, lengths)):
        if row.get("crate") != crate_id:
            continue
        score = 0.0
        for term in terms:
            tf = term_freq.get(term, 0)
            if tf:
                df = doc_freq.get(term, 0)
                idf = math.log(1 + (len(rows) - df + 0.5) / (df + 0.5))
                score += idf * (tf * 2.2) / (tf + 1.2 * (0.25 + 0.75 * length / max(avg_len, 1)))
        scored.append((score, row.get("crate", ""), i))
    return sorted(scored, reverse=True)[:k]

model = SentenceTransformer("sentence-transformers/all-MiniLM-L6-v2", device="cpu")
texts = [row.get("text", "")[:6000] for row in rows]
start = time.perf_counter()
vectors = model.encode(texts, batch_size=32, normalize_embeddings=True, convert_to_numpy=True, show_progress_bar=True)
embedding_seconds = round(time.perf_counter() - start, 3)
query_vectors = model.encode([item["query"] for item in queries], normalize_embeddings=True, convert_to_numpy=True)

results = []
dense_hits = 0
bm25_hits = 0
for query, vector in zip(queries, query_vectors):
    eligible = [i for i, row in enumerate(rows) if row.get("crate") == query["crate_id"]]
    dense_order = sorted(eligible, key=lambda i: float(vectors[i] @ vector), reverse=True)[:5]
    dense_sources = [rows[int(i)].get("source", "") for i in dense_order]
    dense_hit = bool(dense_order and any(rows[int(i)].get("crate") in query["expected"] for i in dense_order))
    bm25_order = bm25(query["query"], query["crate_id"])
    bm25_sources = [rows[i].get("source", "") for score, _, i in bm25_order if score > 0]
    bm25_hit = bool(bm25_sources and any(crate in query["expected"] for score, crate, _ in bm25_order if score > 0))
    dense_hits += int(dense_hit)
    bm25_hits += int(bm25_hit)
    results.append({"query": query["query"], "crate_id": query["crate_id"], "expected_crates": query["expected"],
                    "dense_top5_sources": dense_sources, "bm25_top5_sources": bm25_sources,
                    "dense_hit_at_5": dense_hit, "bm25_hit_at_5": bm25_hit})

out = Path("/kaggle/working")
out.mkdir(parents=True, exist_ok=True)
metrics = {
    "dataset": repo_id,
    "dataset_revision": revision,
    "rows": len(rows),
    "embedding_model": "sentence-transformers/all-MiniLM-L6-v2",
    "embedding_seconds_cpu": embedding_seconds,
    "query_count": len(queries),
    "dense_hit_at_5": dense_hits / len(queries),
    "bm25_hit_at_5": bm25_hits / len(queries),
    "per_query": results,
    "output_notice": "Kaggle outputs include aggregate metrics and source paths only; source text and embeddings are not exported.",
}
(out / "retrieval-evaluation.json").write_text(json.dumps(metrics, indent=2), encoding="utf-8")
print(json.dumps({key: metrics[key] for key in ("rows", "embedding_seconds_cpu", "dense_hit_at_5", "bm25_hit_at_5")}, indent=2))
