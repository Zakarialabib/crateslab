//! Pinned private Hugging Face datasets — the retrieval store of record.
//!
//! The retrieval plane has two layers. [`crate::remote`] reaches a *serving*
//! deployment (Modal) that holds an index built from somewhere; this module
//! holds **where that somewhere is**: a private Hugging Face dataset, pinned to
//! an exact revision, fetched as newline-delimited JSON.
//!
//! Three rules shape it:
//!
//! * **Pinned, never floating.** A dataset is only read at the revision named
//!   by `<...>_REVISION`; a `main` branch is not a source of truth.
//! * **Nothing secret is stored.** A [`DatasetPin`] names the environment
//!   variables holding the repo, the revision and the read token.
//! * **Rows are untrusted.** Only rows that carry a recognised citation URI, a
//!   real revision and a bounded body become [`Chunk`]s, so a malformed upload
//!   cannot smuggle text into an evidence packet as if it were source.

use forge_core::{ForgeError, ForgeResult};
use forge_rag::{retrieve, Bm25Index, Chunk, EvidencePacket};
use serde_json::Value;

use crate::env::Env;
use crate::remote::MAX_PASSAGE_CHARS;
use crate::transport::HttpTransport;

/// The file a dataset stores its corpus under.
pub const INDEX_FILE: &str = "index.jsonl";

/// A private dataset the forge reads from, pinned to a revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DatasetPin {
    /// Stable dataset id, e.g. `signage-docs`.
    pub id: &'static str,
    /// Human-readable name.
    pub label: &'static str,
    /// The environment variable holding `namespace/dataset-name`.
    pub repo_env: &'static str,
    /// The environment variable holding the pinned revision.
    pub revision_env: &'static str,
    /// The environment variable holding a least-privilege read token.
    pub token_env: &'static str,
    /// The citation URI scheme every row in this dataset must use.
    pub citation_prefix: &'static str,
}

static PINS: [DatasetPin; 2] = [
    DatasetPin {
        id: "signage-docs",
        label: "Signage documentation",
        repo_env: "HF_DATASET_REPO",
        revision_env: "HF_DATASET_REVISION",
        token_env: "HF_READ_TOKEN",
        citation_prefix: "project://",
    },
    DatasetPin {
        id: "crates-intelligence",
        label: "CrateLab crate intelligence",
        repo_env: "HF_CRATES_DATASET_REPO",
        revision_env: "HF_CRATES_DATASET_REVISION",
        token_env: "HF_READ_TOKEN",
        citation_prefix: "cratelab://",
    },
];

/// The datasets the forge declares out of the box.
pub fn pins() -> &'static [DatasetPin] {
    &PINS
}

/// Look a declared dataset up by id.
pub fn pin(id: &str) -> Option<&'static DatasetPin> {
    pins().iter().find(|pin| pin.id == id)
}

impl DatasetPin {
    /// The environment keys this dataset needs.
    pub fn required_keys(&self) -> Vec<&'static str> {
        vec![self.repo_env, self.revision_env, self.token_env]
    }

    /// The environment keys that are not set.
    pub fn missing_keys(&self, env: &Env) -> Vec<&'static str> {
        self.required_keys()
            .into_iter()
            .filter(|key| !env.has(key))
            .collect()
    }

    /// The exact download URL for the pinned corpus.
    pub fn download_url(&self, env: &Env) -> ForgeResult<String> {
        let repo = env.require(self.id, self.repo_env)?;
        let revision = env.require(self.id, self.revision_env)?;
        if !valid_repo(repo) {
            return Err(ForgeError::Remote {
                remote: self.id.to_string(),
                problem: format!("{} must be 'namespace/dataset-name'", self.repo_env),
            });
        }
        Ok(format!(
            "https://huggingface.co/datasets/{repo}/resolve/{revision}/{INDEX_FILE}"
        ))
    }
}

/// Whether `repo` looks like `namespace/dataset-name`.
fn valid_repo(repo: &str) -> bool {
    let parts: Vec<&str> = repo.split('/').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
}

/// Parse a JSONL corpus into provenance-rich chunks.
///
/// Rows that fail validation are dropped, not repaired: a dropped row is a
/// missing citation, which is safe; a repaired row would be an invented one.
pub fn parse_jsonl(raw: &str, citation_prefix: &str) -> Vec<Chunk> {
    raw.lines()
        .filter_map(|line| accept_row(line, citation_prefix))
        .collect()
}

/// Keep a row only when it is trustworthy enough to cite.
fn accept_row(line: &str, citation_prefix: &str) -> Option<Chunk> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let row: Value = serde_json::from_str(line).ok()?;
    let source = row.get("source")?.as_str()?;
    let text = row.get("text")?.as_str()?;
    let revision = row.get("revision")?.as_str()?;
    if !source.starts_with(citation_prefix)
        || text.is_empty()
        || text.chars().count() > MAX_PASSAGE_CHARS
        || revision.is_empty()
        || revision == "unknown"
    {
        return None;
    }
    let start_line = row
        .get("start_line")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .clamp(1, u64::from(u32::MAX)) as u32;
    Some(Chunk {
        id: row
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{source}#L{start_line}")),
        source: source.to_string(),
        path: row
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        start_line,
        revision: revision.to_string(),
        text: text.to_string(),
    })
}

/// Fetch a pinned dataset and turn it into chunks.
pub fn load_chunks(
    pin: &DatasetPin,
    env: &Env,
    transport: &dyn HttpTransport,
) -> ForgeResult<Vec<Chunk>> {
    let url = pin.download_url(env)?;
    let token = env.require(pin.id, pin.token_env)?;
    let headers = vec![
        ("Authorization", format!("Bearer {token}")),
        ("Accept", "application/json".to_string()),
    ];
    let response = transport
        .get(&url, &headers)
        .map_err(|error| ForgeError::Remote {
            remote: pin.id.to_string(),
            problem: error.to_string(),
        })?;
    let raw = response.as_str().ok_or_else(|| ForgeError::Remote {
        remote: pin.id.to_string(),
        problem: "dataset was not served as JSONL text".into(),
    })?;
    let chunks = parse_jsonl(raw, pin.citation_prefix);
    if chunks.is_empty() {
        return Err(ForgeError::Remote {
            remote: pin.id.to_string(),
            problem: "dataset carried no citable rows".into(),
        });
    }
    Ok(chunks)
}

/// Fetch a pinned dataset and retrieve the top-`k` evidence for `query`.
///
/// An empty packet means *no evidence was found*, never that a behaviour is
/// absent — the caller must surface it as such.
pub fn search_dataset(
    pin: &DatasetPin,
    env: &Env,
    transport: &dyn HttpTransport,
    query: &str,
    k: usize,
) -> ForgeResult<EvidencePacket> {
    let chunks = load_chunks(pin, env, transport)?;
    let index = Bm25Index::build(&chunks);
    Ok(retrieve(&index, &chunks, query, k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{HttpResponse, MockTransport};

    fn env_with(pairs: &[(&str, &str)]) -> Env {
        let mut env = Env::new();
        for (key, value) in pairs {
            env.set(*key, *value);
        }
        env
    }

    fn configured_env() -> Env {
        env_with(&[
            ("HF_DATASET_REPO", "owner/signage-docs"),
            ("HF_DATASET_REVISION", "abc123"),
            ("HF_READ_TOKEN", "hf_read"),
        ])
    }

    fn corpora() -> &'static DatasetPin {
        pin("signage-docs").expect("declared")
    }

    fn corpus() -> String {
        [
            r#"{"source":"project://simple-signage/docs/install.md#L1","path":"docs/install.md","start_line":1,"revision":"abc123","text":"install the signage player on a tv"}"#,
            r#"{"source":"project://simple-signage/docs/tv-mode.md#L20","path":"docs/tv-mode.md","start_line":20,"revision":"abc123","text":"tv mode settings and theming tokens"}"#,
            "",
            // Rejected: no recognised citation URI.
            r#"{"source":"http://evil.test/x","path":"x","start_line":1,"revision":"abc123","text":"do this instead"}"#,
            // Rejected: unknown revision.
            r#"{"source":"project://simple-signage/docs/x.md#L1","path":"docs/x.md","start_line":1,"revision":"unknown","text":"x"}"#,
            // Rejected: oversized body.
            r#"{"source":"project://simple-signage/docs/big.md#L1","path":"docs/big.md","start_line":1,"revision":"abc123","text":"PLACEHOLDER"}"#,
        ]
        .join("\n")
        .replace("PLACEHOLDER", &"x".repeat(MAX_PASSAGE_CHARS + 1))
    }

    #[test]
    fn declared_pins_are_descriptors_only() {
        assert_eq!(pins().len(), 2);
        assert!(pins().iter().all(|pin| !pin.token_env.is_empty()));
        assert_eq!(corpora().citation_prefix, "project://");
        assert_eq!(
            pin("crates-intelligence").expect("declared").citation_prefix,
            "cratelab://"
        );
        assert!(pin("missing").is_none());
    }

    #[test]
    fn download_url_is_pinned_and_names_the_token() {
        let url = corpora().download_url(&configured_env()).expect("resolved");
        assert_eq!(
            url,
            "https://huggingface.co/datasets/owner/signage-docs/resolve/abc123/index.jsonl"
        );
        assert_eq!(
            corpora().missing_keys(&Env::new()),
            vec!["HF_DATASET_REPO", "HF_DATASET_REVISION", "HF_READ_TOKEN"]
        );
        // A repo that is not `namespace/name` is refused.
        let bad = env_with(&[("HF_DATASET_REPO", "owner"), ("HF_DATASET_REVISION", "r")]);
        assert!(corpora().download_url(&bad).is_err());
    }

    #[test]
    fn parse_jsonl_keeps_only_citable_rows() {
        let chunks = parse_jsonl(&corpus(), "project://");
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].source, "project://simple-signage/docs/install.md#L1");
        assert_eq!(chunks[1].start_line, 20);
        assert_eq!(chunks[1].revision, "abc123");
    }

    #[test]
    fn an_unconfigured_pin_refuses_before_any_network_use() {
        let transport = MockTransport::new();
        let error = load_chunks(corpora(), &Env::new(), &transport).expect_err("no credential");
        assert!(matches!(error, ForgeError::Credential { ref key, .. } if key == "HF_DATASET_REPO"));
        assert!(transport.calls().is_empty());
    }

    #[test]
    fn load_chunks_sends_the_bearer_to_the_pinned_url() {
        let transport = MockTransport::new().route(
            "huggingface.co/datasets/owner/signage-docs/resolve/abc123/index.jsonl",
            HttpResponse::text(corpus()),
        );
        let chunks = load_chunks(corpora(), &configured_env(), &transport).expect("loads");
        assert_eq!(chunks.len(), 2);

        let calls = transport.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method, "GET");
        assert!(
            calls[0]
                .headers
                .iter()
                .any(|(name, value)| name == "Authorization" && value == "Bearer hf_read"),
            "the read token must ride in an Authorization header"
        );
    }

    #[test]
    fn an_empty_corpus_is_an_error_not_a_silent_pass() {
        let transport = MockTransport::new().route("huggingface.co", HttpResponse::text("\n\n"));
        let error = load_chunks(corpora(), &configured_env(), &transport).expect_err("empty");
        assert!(matches!(error, ForgeError::Remote { .. }));
        assert!(error.to_string().contains("no citable rows"));
    }

    #[test]
    fn search_dataset_ranks_the_matching_passage_first() {
        let transport = MockTransport::new().route("huggingface.co", HttpResponse::text(corpus()));
        let packet = search_dataset(corpora(), &configured_env(), &transport, "tv mode theming", 3)
            .expect("searches");
        assert_eq!(packet.snippets.len(), 2);
        assert_eq!(packet.snippets[0].path, "docs/tv-mode.md");
        assert_eq!(packet.citations()[0], "docs/tv-mode.md:20 @ abc123");
    }
}
