# CrateLab Fleet

The Fleet view is the staged control plane for research and documentation work on the public `tv-rs` workspace. Every configured provider request requires a preview and explicit confirmation. Automatic failover is restricted to configured providers that have previously passed a probe and have remaining local request budget. Provider quotas and billing terms are account-specific and may change; local caps do not prove a provider's free allowance.

## Rust-owned desktop path

The Tauri Rust backend owns local settings, provider routing, workspace previews, documentation corpus construction and validation, HF identity/upload actions, and Modal CLI secret/deploy orchestration. The Svelte UI presents settings and explicit review dialogs. Python remains only in hosted Modal ASGI search runtime and provider-hosted notebook cells.

- `shell/src-tauri/src/fleet_knowledge.rs`: local corpus build/validation and confirmed HF/Modal operations.
- `fleet/modal/app.py`: authenticated, read-only BM25 search over a pinned private HF dataset revision.
- `fleet/notebooks/kaggle_embed.md`: optional GPU embedding procedure; dense embedding is not used in the active lexical pilot.
- `fleet/config.json`: workspace roots, dataset metadata, credential setup links, and safety defaults.
- `fleet/circleci/config.yml`: Rust 1.89 fmt/clippy/test/rustdoc gate template for the tv-rs workspace.

## CircleCI operations dashboard

Fleet Settings accepts a CircleCI personal API token and `CIRCLECI_PROJECT_SLUGS`, a comma-separated list such as `gh/Zakarialabib/tv-rs`. The dashboard sends the token to CircleCI API v2 only after the user reviews and confirms the read-only status request. It reads account and collaboration metadata, project visibility, up to five recent pipeline summaries per project, and workflow statuses for the newest pipeline. Requests are bounded to twelve configured projects.

An unavailable project is reported as not connected or not visible; this does not prove which cause applies. Connect the repository in CircleCI, confirm the exact VCS project slug, and check token access. The dashboard also checks whether the local Rust gate template exists and whether the selected tv-rs workspace already has `.circleci/config.yml`. Applying that template to tv-rs is a manual, reviewable repository change. Fleet does not trigger, cancel, retry, or edit pipelines and does not change CircleCI or GitHub settings.

The template pins Rust 1.89 to match tv-rs `rust-version`. Its workflow runs formatting, Clippy, workspace tests, and warning-denying documentation generation. The template is not installed in tv-rs automatically.

## Operations dashboard

The desktop landing view is `Ops dashboard`. It brings together two in-memory mock-device scenarios (TV morning loop and CCTV zone watch), local tv-rs research/rustdoc report actions, local signage-index build/validation, provider readiness, and CircleCI collaboration status. The GitHub identity check, provider probes, and CircleCI status load each require a review click before credentials are sent. The scenario cards target only the Rust mock backend; real hardware support is not enabled by this screen.

## Private docs RAG boundary

The private Simple-Signage and companion-app repositories provide an explicitly selected, docs-only context for research/docs on `tv-rs`. Only each repository's `docs/` tree and top-level README/CHANGELOG/CONTRIBUTING are indexed. Application source is excluded. The private repositories are never task targets and are never edited by Fleet. The corpus builder records source revisions and whether documentation files were dirty; the upload review discloses that uncommitted documentation is included.

## HF → Modal

1. Set the workspace roots and HF dataset repo in Fleet Settings.
2. Build and locally validate the corpus. No provider call or upload occurs.
3. Review the upload dialog, then explicitly confirm to create/update a private dataset and pin its revision. `HF_TOKEN_WRITE` is preferred; `HF_TOKEN` is the fallback. The selected token needs dataset write scope.
4. For deploy, set a dedicated read-only `HF_READ_TOKEN` if possible. Review the Modal dialog and explicitly confirm. Rust creates/updates named Modal secrets and deploys the Modal search app. Modal receives the HF read credential, repo, revision, and generated search bearer token. The bearer token is saved to ignored local `.env`.
5. Probe the Modal endpoint. During a confirmed tv-rs provider request, select a signage project to include a limited number of retrieved snippets. Reports retain source paths and pinned revisions.

HF upload and Modal deployment are separate externally visible operations. Neither is invoked by building or validating the local index. Current provider pricing and quotas must be checked in account dashboards. Keep spend controls enabled if the goal is zero out-of-pocket cost.

## Credentials

Fleet settings use the ignored `fleet/.env`. Secret values are never returned to the UI. Existing values are preserved when a secret input is left blank. Provider fields include official setup links when missing. HF CLI and Modal CLI profiles remain local; secret transfer to Modal happens only in the confirmed deploy flow. GitHub writes, target edits, and automatic merges remain disabled.


## Internal crate intelligence (HF → Modal → Kaggle)

`docs/crate-intelligence.md` is the source-traced role/dependency map for all eight crates. Fleet builds a separate local index from `crates/*/src/**/*.rs` plus package name, description, and direct internal dependencies; it validates provenance and scans indexed text for credential-like strings before an upload can be reviewed. `fleet/prompts/crate-evolution-research.md` defines the context order, evidence labeling, and output contract. Prompt generation adds the selected crate's manifest/package facts and public declarations; use it before any third-party research request.

HF uses `HF_CRATES_DATASET_REPO` and stores a pinned revision in `HF_CRATES_DATASET_REVISION`. Upload is a separate confirmed action and requires private visibility. Modal runs a distinct authenticated BM25 search app filtered by crate. Kaggle's private CPU-first evaluation reads only the pinned HF revision via Kaggle Secrets and compares dense retrieval with BM25 on crate-filtered questions. Results contain aggregate scores and source paths only; they are not downloaded or published automatically. Kaggle submit uploads the kernel and starts compute, so Fleet shows a review dialog first. `KAGGLE_KERNEL_ID` must be set to your `owner/kernel-slug`, and the private Kaggle kernel must be allowed to use `HF_READ_TOKEN`, `HF_CRATES_DATASET_REPO`, and `HF_CRATES_DATASET_REVISION` Secrets.
