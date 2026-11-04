"""Read-only BM25 search over a private Hugging Face dataset artifact."""
import json
import math
import os
import re
from collections import Counter
from pathlib import Path

import modal

app = modal.App("cratelab-signage-docs-search")
image = modal.Image.debian_slim().pip_install("fastapi", "huggingface_hub")
secrets = [modal.Secret.from_name("cratelab-hf-read"), modal.Secret.from_name("cratelab-search-token")]


def tokens(text):
    return re.findall(r"[a-zA-Z0-9_]+", text.lower())


@app.cls(image=image, secrets=secrets, scaledown_window=300)
class CorpusSearch:
    @modal.enter()
    def load_corpus(self):
        from huggingface_hub import snapshot_download

        repo_id = os.environ["HF_DATASET_REPO"]
        revision = os.environ["HF_DATASET_REVISION"]
        local_dir = snapshot_download(
            repo_id=repo_id,
            repo_type="dataset",
            revision=revision,
            token=os.environ["HF_TOKEN"],
            allow_patterns=["index.jsonl"],
            local_dir="/tmp/cratelab-corpus",
        )
        index_path = Path(local_dir) / "index.jsonl"
        self.rows = [json.loads(line) for line in index_path.open(encoding="utf-8") if line.strip()]
        self.docs = [tokens(row.get("text", "")) for row in self.rows]
        self.term_freqs = [Counter(doc) for doc in self.docs]
        self.doc_len = [len(doc) for doc in self.docs]
        self.avg_len = sum(self.doc_len) / max(1, len(self.doc_len))
        self.doc_freq = Counter(term for freq in self.term_freqs for term in freq)

    @modal.asgi_app()
    def web(self):
        from fastapi import FastAPI, Header, HTTPException
        from pydantic import BaseModel, Field
        from secrets import compare_digest

        api = FastAPI()

        class Query(BaseModel):
            query: str = Field(min_length=2, max_length=2000)
            k: int = Field(default=6, ge=1, le=20)
            project_id: str = Field(default="", max_length=80)

        def authorized(value: str) -> bool:
            expected = os.environ.get("SEARCH_BEARER_TOKEN", "")
            return bool(expected) and compare_digest(value, f"Bearer {expected}")

        @api.get("/health")
        def health(authorization: str = Header(default="")):
            if not authorized(authorization):
                raise HTTPException(status_code=401, detail="unauthorized")
            return {"ok": True, "chunks": len(self.rows), "dataset_revision": os.environ["HF_DATASET_REVISION"]}

        @api.post("/search")
        def search(body: Query, authorization: str = Header(default="")):
            if not authorized(authorization):
                raise HTTPException(status_code=401, detail="unauthorized")
            terms = set(tokens(body.query))
            if not terms:
                return []
            if body.project_id and body.project_id not in {"simple-signage", "companion-app"}:
                raise HTTPException(status_code=400, detail="unknown project")
            hits = []
            total_docs = len(self.rows)
            for row, frequencies, length in zip(self.rows, self.term_freqs, self.doc_len):
                if body.project_id and row.get("project") != body.project_id:
                    continue
                score = 0.0
                for term in terms:
                    tf = frequencies.get(term, 0)
                    if not tf:
                        continue
                    df = self.doc_freq.get(term, 0)
                    idf = math.log(1 + (total_docs - df + 0.5) / (df + 0.5))
                    score += idf * (tf * 2.2) / (tf + 1.2 * (0.25 + 0.75 * length / max(self.avg_len, 1)))
                if score > 0:
                    hits.append({"score": score, "project": row.get("project"), "source": row.get("source"), "path": row.get("path"),
                                 "start_line": row.get("start_line"), "revision": row.get("revision"),
                                 "text": row.get("text", "")[:6000]})
            hits.sort(key=lambda hit: hit["score"], reverse=True)
            return hits[:body.k]

        return api
