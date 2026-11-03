# Local fleet control plane

The Fleet screen in cratelab is the control plane for a staged free-tier crate-evolution workflow. This tracked `fleet/` module contains implementation scripts and service templates. Its local `.env`, `state/`, `reports/`, and `data/` stay untracked. The pilot catalogs every crate directory in the configured tv-rs workspace and stages three local task items at a time. Runtime config permits only user-confirmed provider calls; target and GitHub writes remain disabled.

## Current executable path

1. Select a crate from the discovered tv-rs workspace catalog and a research/docs task.
2. Optionally stage a local batch of three crate tasks; this never sends data.
3. Select a provider and successfully probe its model catalogue.
4. Preview the selected crate request and confirm the run.
5. The router tries the selected provider, then only configured and previously probed fallbacks. Conservative per-day request caps are enforced from `state/usage.json`; usage stores provider/model/status/token totals, never prompts or response bodies.
6. The source checkout is read-only. `Cargo.toml`, lockfiles, CI files, and other crates are not included in the model context. Results write local reports only.

## Full design pieces

- `scripts/build_rag_index.py`: deterministic provenance-rich JSONL corpus builder. Review output before upload.
- `notebooks/kaggle_embed.md`: Kaggle batch procedure. Embeddings and HF upload are manual; check current account quota first.
- `modal/app.py`: read-only authenticated lexical `/search` endpoint contract. It requires a reviewed index Volume and a Modal secret.
- `circleci/config.yml`: gate template to copy to a target repo after adapting its Rust toolchain/workspace.
- Colab and Kaggle Bonsai remain optional OpenAI-compatible endpoints configured per session with `BONSAI_URL` / `KAGGLE_BONSAI_URL`.

## Credentials

Copy `.env.example` to `.env`. Values are read in the Rust backend and never returned to the UI. Rotate credentials if they were previously exposed. Start with one provider; provider key presence is not a successful probe. No deployment, GitHub write, issue, PR, target edit, or merge is automated.

## Quotas and cost control

The code's daily caps are conservative local request ceilings, not provider-published free allowances. Confirm current limits in each provider dashboard and keep billing disabled if the requirement is zero spend. Fallback requests count against the next provider's local cap. A provider probe is a separate network call and only reports reachability/model IDs.
