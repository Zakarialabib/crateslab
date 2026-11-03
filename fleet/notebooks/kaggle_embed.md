# Kaggle index build (review-first)

1. Attach only the public crate checkout as a Kaggle Dataset input and enable Internet only for package/model download.
2. Run `python /kaggle/input/cratelab/build_rag_index.py --root /kaggle/input/tv-rs --crate tv-rs --out /kaggle/working/index.jsonl`.
3. Inspect the JSONL: every row must have `crate://` provenance; confirm private names, `.env`, lockfiles and CI paths are absent.
4. Optional embedding pass: use `sentence-transformers` with BGE-M3 on the Kaggle GPU and write `index.parquet` containing `id, crate, source, revision, text, vector`.
5. Upload only the reviewed corpus/index to a private Hugging Face dataset repository. Keep the GitHub/HF token in Kaggle Secrets; never write it into notebook output or a dataset.
6. Modal search currently consumes `index.jsonl` from its read-only Volume. Dense vector search/Qdrant is a later optimization once lexical search quality is measured.

Kaggle quotas and GPU availability vary by account and date. Check the current Kaggle settings page before planning a batch; this notebook has no scheduler and makes no automatic uploads.
