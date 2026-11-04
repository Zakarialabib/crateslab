//! Evidence-packet retrieval.
//!
//! Turns a ranked index into an [`EvidencePacket`] with full provenance, ready
//! for [`crate::prompt::render`]. The packet is the *only* thing a provider
//! call is allowed to see beyond the question, so every snippet keeps its
//! source, path, line range and pinned revision.

use crate::index::Bm25Index;
use crate::ingest::Chunk;
use serde::{Deserialize, Serialize};

/// One provenance-preserving evidence passage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    /// The BM25 score that ranked this snippet.
    pub score: f64,
    /// The citation URI of the source chunk.
    pub source: String,
    /// Path within the source.
    pub path: String,
    /// First line of the cited range (1-based).
    pub start_line: u32,
    /// The pinned revision the passage was read from.
    pub revision: String,
    /// The passage text.
    pub text: String,
    /// The index of the originating chunk, for round-tripping.
    pub chunk: usize,
}

/// A grounded evidence packet handed to prompt rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidencePacket {
    /// The query that produced this packet.
    pub query: String,
    /// The ranked snippets, best first.
    pub snippets: Vec<Snippet>,
}

impl EvidencePacket {
    /// An empty packet for `query`.
    ///
    /// An empty packet means *no evidence was found* — it is never proof that
    /// a behaviour is absent. Callers must surface it as such.
    pub fn empty(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            snippets: Vec::new(),
        }
    }

    /// Whether the packet carries no evidence.
    pub fn is_empty(&self) -> bool {
        self.snippets.is_empty()
    }

    /// The evidence lines to attach to a provider request's audit trail.
    pub fn citations(&self) -> Vec<String> {
        self.snippets
            .iter()
            .map(|snippet| format!("{}:{} @ {}", snippet.path, snippet.start_line, snippet.revision))
            .collect()
    }
}

/// Retrieve the top-`k` snippets for `query`.
pub fn retrieve(index: &Bm25Index, chunks: &[Chunk], query: &str, k: usize) -> EvidencePacket {
    let hits = index.search(query, k);
    let snippets = hits
        .into_iter()
        .filter_map(|hit| {
            chunks.get(hit.chunk).map(|chunk| Snippet {
                score: hit.score,
                source: chunk.source.clone(),
                path: chunk.path.clone(),
                start_line: chunk.start_line,
                revision: chunk.revision.clone(),
                text: chunk.text.clone(),
                chunk: hit.chunk,
            })
        })
        .collect();
    EvidencePacket {
        query: query.to_string(),
        snippets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{ingest, SourceSpec};

    fn chunks() -> Vec<Chunk> {
        let spec = SourceSpec {
            id: "crates".into(),
            root: "CRATELAB_ROOT".into(),
            extensions: vec![".rs".into()],
            pristine: true,
        };
        ingest(
            &spec,
            "rev-1",
            &[
                (
                    "coverage.rs".to_string(),
                    "pub fn coverage_ledger_update() { /* coverage ledger */ }\n".to_string(),
                ),
                (
                    "scenario.rs".to_string(),
                    "pub fn scenario_step() { /* scenario step */ }\n".to_string(),
                ),
            ],
        )
    }

    #[test]
    fn retrieval_preserves_provenance() {
        let chunks = chunks();
        let index = Bm25Index::build(&chunks);
        let packet = retrieve(&index, &chunks, "coverage ledger", 5);
        assert!(!packet.is_empty());
        let snippet = &packet.snippets[0];
        assert_eq!(snippet.path, "coverage.rs");
        assert_eq!(snippet.revision, "rev-1");
        assert_eq!(snippet.start_line, 1);
        assert_eq!(snippet.source, "crate://crates/coverage.rs#L1");
    }

    #[test]
    fn a_miss_is_an_empty_packet_not_an_error() {
        let chunks = chunks();
        let index = Bm25Index::build(&chunks);
        let packet = retrieve(&index, &chunks, "quantum flux", 5);
        assert!(packet.is_empty());
        assert!(packet.citations().is_empty());
    }
}
