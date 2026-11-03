"""Minimal Modal-hosted read-only lexical retrieval for a JSONL corpus.

The endpoint accepts {query, k}, returns [{score, source, text}], and never
accepts writes. Mount the approved corpus through a Modal Volume after review.
"""
import json
import os
import re

import modal

app = modal.App("cratelab-rust-search")
image = modal.Image.debian_slim().pip_install("fastapi")
corpus = modal.Volume.from_name("cratelab-rust-corpus", create_if_missing=False)


@app.function(image=image, volumes={"/data": corpus}, secrets=[modal.Secret.from_name("cratelab-search-token")])
@modal.asgi_app()
def search_app():
    from fastapi import FastAPI, Header, HTTPException
    from pydantic import BaseModel, Field

    api = FastAPI()

    class Query(BaseModel):
        query: str = Field(min_length=2, max_length=2000)
        k: int = Field(default=6, ge=1, le=20)

    @api.post("/search")
    def search(body: Query, authorization: str = Header(default="")):
        expected = os.environ.get("SEARCH_BEARER_TOKEN", "")
        if not expected or authorization != f"Bearer {expected}":
            raise HTTPException(status_code=401, detail="unauthorized")
        terms = set(re.findall(r"[a-zA-Z0-9_]+", body.query.lower()))
        hits = []
        path = "/data/index.jsonl"
        with open(path, encoding="utf-8") as stream:
            for line in stream:
                row = json.loads(line)
                text = row.get("text", "")
                doc_terms = set(re.findall(r"[a-zA-Z0-9_]+", text.lower()))
                score = len(terms & doc_terms) / max(1, len(terms))
                if score:
                    hits.append({"score": score, "source": row.get("source"), "text": text[:6000]})
        hits.sort(key=lambda hit: hit["score"], reverse=True)
        return hits[:body.k]

    return api
