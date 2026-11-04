//! Corpus ingestion — source trees into provenance-rich chunks.
//!
//! A port of the former `fleet/scripts/build_rag_index.py` into the workspace,
//! so the corpus builder is typed, tested and shipped with the crate instead of
//! living in an untracked Python script. The chunker is line-based and
//! deterministic; the skip rules keep lockfiles, credentials and CI config out
//! of the index.

use serde::{Deserialize, Serialize};

/// Lines per chunk. Mirrors the historical Python default.
pub const CHUNK_LINES: usize = 80;

/// Lines of overlap between consecutive chunks.
pub const CHUNK_OVERLAP: usize = 12;

/// File names that must never be indexed (credentials and lockfiles).
const DENIED_FILES: [&str; 5] = [
    "cargo.toml",
    "cargo.lock",
    ".env",
    "secrets.toml",
    "credentials.json",
];

/// A source root to ingest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpec {
    /// Stable source id, e.g. `tv-rs` or a docs-tree id.
    pub id: String,
    /// Root path. Prefer environment-resolved or repo-relative; never commit an
    /// absolute user path.
    pub root: String,
    /// Lower-case extensions to include, with the leading dot (`[".rs", ".md"]`).
    pub extensions: Vec<String>,
    /// True when the tree is a clean checkout, so the recorded revision is
    /// trustworthy and uncommitted files were not included.
    #[serde(default)]
    pub pristine: bool,
}

/// One indexed chunk with full provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    /// Globally unique id: `{source}:{path}:{start_line}`.
    pub id: String,
    /// A citation URI, e.g. `crate://lab-manifest/src/lib.rs#L12`.
    pub source: String,
    /// Path within the source, slash-separated.
    pub path: String,
    /// First line of the chunk (1-based).
    pub start_line: u32,
    /// The exact source revision the chunk was read from.
    pub revision: String,
    /// The chunk text.
    pub text: String,
}

/// Deterministic line-window chunker.
///
/// Emits `(start_line, text)` for every non-empty window of `size` lines,
/// stepping `size - overlap` lines at a time.
pub fn chunk_lines(text: &str, size: usize, overlap: usize) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    if size == 0 {
        return out;
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut start = 0usize;
    while start < lines.len() {
        let end = (start + size).min(lines.len());
        let body = lines[start..end].join("\n");
        let body = body.trim();
        if !body.is_empty() {
            out.push(((start + 1) as u32, body.to_string()));
        }
        if end == lines.len() {
            break;
        }
        start = end.saturating_sub(overlap);
    }
    out
}

/// Whether a path is indexable under `extensions`.
///
/// Rejects denied file names (lockfiles, credentials) and CI config trees
/// (`.github/`, `.circleci/`) regardless of extension.
pub fn is_indexable(path: &str, extensions: &[String]) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(lower.as_str());
    if DENIED_FILES.contains(&name) {
        return false;
    }
    if lower.starts_with(".github/")
        || lower.starts_with(".circleci/")
        || lower.contains("/.github/")
        || lower.contains("/.circleci/")
    {
        return false;
    }
    extensions
        .iter()
        .any(|extension| lower.ends_with(extension.as_str()))
}

/// Ingest a spec from in-memory `(relative_path, contents)` files.
pub fn ingest(spec: &SourceSpec, revision: &str, files: &[(String, String)]) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    for (path, text) in files {
        if !is_indexable(path, &spec.extensions) {
            continue;
        }
        for (start_line, body) in chunk_lines(text, CHUNK_LINES, CHUNK_OVERLAP) {
            chunks.push(Chunk {
                id: format!("{}:{}:{}", spec.id, path, start_line),
                source: format!("crate://{}/{}#L{}", spec.id, path, start_line),
                path: path.clone(),
                start_line,
                revision: revision.to_string(),
                text: body,
            });
        }
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> SourceSpec {
        SourceSpec {
            id: "lab-manifest".into(),
            root: "CRATELAB_ROOT".into(),
            extensions: vec![".rs".into(), ".md".into()],
            pristine: true,
        }
    }

    #[test]
    fn chunker_windows_with_overlap_and_line_numbers() {
        let text = (1..=10)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let chunks = chunk_lines(&text, 4, 2);
        assert_eq!(chunks[0].0, 1);
        assert!(chunks[0].1.starts_with("line 1"));
        // Second window steps back by the overlap: start = 4 - 2 + 1 = 3.
        assert_eq!(chunks[1].0, 3);
        assert_eq!(chunks.last().expect("last").0, 7);
    }

    #[test]
    fn ingest_skips_lockfiles_credentials_and_ci_config() {
        let files = vec![
            ("src/lib.rs".to_string(), "pub fn a() {}\n".to_string()),
            ("Cargo.toml".to_string(), "[package]\nname='x'\n".to_string()),
            (".env".to_string(), "SECRET=1\n".to_string()),
            (
                ".github/workflows/ci.yml".to_string(),
                "on: push\n".to_string(),
            ),
            ("README.md".to_string(), "# Title\n".to_string()),
        ];
        let chunks = ingest(&spec(), "abc123", &files);
        let paths: Vec<&str> = chunks.iter().map(|c| c.path.as_str()).collect();
        assert!(paths.contains(&"src/lib.rs"));
        assert!(paths.contains(&"README.md"));
        assert!(!paths.contains(&"Cargo.toml"));
        assert!(!paths.contains(&".env"));
        assert!(!paths.contains(&".github/workflows/ci.yml"));
    }

    #[test]
    fn chunks_carry_a_citable_source_and_revision() {
        let files = vec![("src/lib.rs".to_string(), "pub fn a() {}\n".to_string())];
        let chunks = ingest(&spec(), "deadbeef", &files);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].source, "crate://lab-manifest/src/lib.rs#L1");
        assert_eq!(chunks[0].revision, "deadbeef");
    }
}
