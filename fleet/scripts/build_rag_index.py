#!/usr/bin/env python3
"""Create a provenance-rich JSONL corpus for public Rust sources.

This portable first stage intentionally emits text chunks only. Embeddings can
be added in a Kaggle notebook after the source corpus is reviewed.
"""
import argparse
import json
import pathlib
import re
import subprocess
import time

EXTENSIONS = {".rs", ".md", ".toml"}
SKIP = {"target", ".git", "node_modules", "vendor", ".env", "secrets", "credentials", ".agents", ".circleci", ".github", "e2e_suite"}


def revision(root: pathlib.Path) -> str:
    try:
        return subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True, stderr=subprocess.DEVNULL).strip()
    except Exception:
        return "unknown"


def chunks(text: str, size: int, overlap: int):
    lines = text.splitlines()
    start = 0
    while start < len(lines):
        end = min(start + size, len(lines))
        body = "\n".join(lines[start:end]).strip()
        if body:
            yield start + 1, body
        if end == len(lines):
            break
        start = end - overlap


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True, type=pathlib.Path, help="public crate/workspace checkout")
    ap.add_argument("--out", required=True, type=pathlib.Path)
    ap.add_argument("--crate", required=True)
    args = ap.parse_args()
    root = args.root.resolve()
    rev = revision(root)
    count = 0
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with args.out.open("w", encoding="utf-8") as out:
        for path in sorted(root.rglob("*")):
            if not path.is_file() or path.suffix.lower() not in EXTENSIONS or any(part in SKIP for part in path.parts):
                continue
            rel = path.relative_to(root).as_posix()
            # Exclude lockfiles and credential/config artifacts from retrieval.
            if path.name in {"Cargo.toml", "Cargo.lock", ".env", "secrets.toml", "credentials.json"} or re.search(r"(^|/)(\.github|\.circleci)/", rel):
                continue
            try:
                raw = path.read_text(encoding="utf-8")
            except (UnicodeDecodeError, OSError):
                continue
            for line, body in chunks(raw, 80, 12):
                record = {"id": f"{args.crate}:{rel}:{line}", "crate": args.crate,
                          "source": f"crate://{args.crate}/{rel}#L{line}", "path": rel,
                          "start_line": line, "revision": rev, "indexed_at": int(time.time()), "text": body}
                out.write(json.dumps(record, ensure_ascii=False) + "\n")
                count += 1
    print(f"wrote {count} chunks to {args.out} (revision {rev})")


if __name__ == "__main__":
    main()
