# Kaggle embedding notes (optional future batch path)

The active first-phase index contains selected private signage documentation. CrateLab's Rust/Tauri backend builds and validates that corpus locally, and Fleet Settings owns any confirmed Hugging Face upload. Do not use the legacy Python uploader steps below.

1. Kaggle is optional. Use it only for a reviewed embedding experiment after BM25 search has been evaluated.
2. Attach only the selected documentation corpus and the minimum model inputs needed; do not attach source trees or secrets.
3. Preserve `id`, `project`, `source`, `revision`, and text alongside each vector so retrieved results remain traceable.
4. Compare embedding retrieval against the existing Modal BM25 endpoint before adopting it. Keep HF upload and deployment in the separate confirmed Fleet Settings workflows.
5. Modal currently loads the pinned `index.jsonl` from the private HF dataset into read-only container memory. Dense vector search/Qdrant is a later optimization.

Kaggle quotas and GPU availability vary by account and date. Check the current Kaggle settings page before planning a batch; this notebook has no scheduler and makes no automatic uploads.
