//! A deterministic BM25 index over ingested chunks.
//!
//! Okapi BM25 with `k1 = 1.2`, `b = 0.75` — the same weighting the former
//! Modal-hosted Python search used, so results stay comparable to the CSV/JSONL
//! evaluation baseline. Ties break on chunk order, so a query is reproducible.

use crate::ingest::Chunk;
use std::collections::{BTreeSet, HashMap};

/// BM25 term-frequency saturation.
const K1: f64 = 1.2;

/// BM25 length normalisation.
const B: f64 = 0.75;

/// Split text into lower-case alphanumeric/underscore tokens.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

/// A ranked match: which chunk, and its score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// Index into the slice the index was built from.
    pub chunk: usize,
    /// BM25 score (strictly positive for every hit).
    pub score: f64,
}

/// Okapi BM25 over chunked sources.
#[derive(Debug, Clone, Default)]
pub struct Bm25Index {
    term_freqs: Vec<HashMap<String, u32>>,
    doc_freq: HashMap<String, u32>,
    lens: Vec<u32>,
    avg_len: f64,
    total: usize,
}

impl Bm25Index {
    /// Build the index over `chunks`.
    pub fn build(chunks: &[Chunk]) -> Self {
        let mut term_freqs = Vec::with_capacity(chunks.len());
        let mut doc_freq: HashMap<String, u32> = HashMap::new();
        let mut lens = Vec::with_capacity(chunks.len());

        for chunk in chunks {
            let tokens = tokenize(&chunk.text);
            let mut tf: HashMap<String, u32> = HashMap::new();
            for token in &tokens {
                *tf.entry(token.clone()).or_insert(0) += 1;
            }
            for term in tf.keys() {
                *doc_freq.entry(term.clone()).or_insert(0) += 1;
            }
            lens.push(tokens.len() as u32);
            term_freqs.push(tf);
        }

        let total = chunks.len();
        let avg_len = if total == 0 {
            0.0
        } else {
            lens.iter().map(|len| f64::from(*len)).sum::<f64>() / total as f64
        };

        Self {
            term_freqs,
            doc_freq,
            lens,
            avg_len,
            total,
        }
    }

    /// The number of indexed chunks.
    pub fn len(&self) -> usize {
        self.total
    }

    /// Whether the index is empty.
    pub fn is_empty(&self) -> bool {
        self.total == 0
    }

    /// Inverse document frequency for one term.
    pub fn idf(&self, term: &str) -> f64 {
        let df = f64::from(*self.doc_freq.get(term).unwrap_or(&0));
        (1.0 + (self.total as f64 - df + 0.5) / (df + 0.5)).ln()
    }

    /// The top-`k` chunks for `query`, best first, ties broken by chunk order.
    pub fn search(&self, query: &str, k: usize) -> Vec<Hit> {
        let terms: BTreeSet<String> = tokenize(query).into_iter().collect();
        if terms.is_empty() || self.total == 0 || k == 0 {
            return Vec::new();
        }
        let normaliser = self.avg_len.max(1.0);
        let mut hits = Vec::new();
        for (chunk, tf) in self.term_freqs.iter().enumerate() {
            let len = f64::from(self.lens[chunk]);
            let mut score = 0.0;
            for term in &terms {
                let freq = f64::from(*tf.get(term).unwrap_or(&0));
                if freq > 0.0 {
                    score += self.idf(term) * (freq * (K1 + 1.0))
                        / (freq + K1 * (1.0 - B + B * len / normaliser));
                }
            }
            if score > 0.0 {
                hits.push(Hit { chunk, score });
            }
        }
        hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.chunk.cmp(&b.chunk))
        });
        hits.truncate(k);
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{SourceSpec, ingest};

    fn corpus() -> Vec<Chunk> {
        let spec = SourceSpec {
            id: "crates".into(),
            root: "CRATELAB_ROOT".into(),
            extensions: vec![".rs".into()],
            pristine: true,
        };
        let files = vec![
            (
                "coverage.rs".to_string(),
                "pub fn coverage_ledger_update() { /* coverage ledger */ }\n".to_string(),
            ),
            (
                "scenario.rs".to_string(),
                "pub fn scenario_step_assertion() { /* scenario step */ }\n".to_string(),
            ),
        ];
        ingest(&spec, "rev", &files)
    }

    #[test]
    fn tokenizer_is_case_folded_and_symbol_free() {
        assert_eq!(tokenize("Pub fn Coverage_Ledger()"), vec![
            "pub", "fn", "coverage_ledger"
        ]);
        assert!(tokenize("   ").is_empty());
    }

    #[test]
    fn search_ranks_the_matching_chunk_first() {
        let chunks = corpus();
        let index = Bm25Index::build(&chunks);
        assert_eq!(index.len(), 2);
        assert!(!index.is_empty());

        let hits = index.search("coverage ledger", 5);
        assert!(!hits.is_empty());
        assert_eq!(chunks[hits[0].chunk].path, "coverage.rs");
        assert!(hits[0].score > 0.0);
    }

    #[test]
    fn empty_index_and_empty_query_yield_no_hits() {
        let index = Bm25Index::build(&[]);
        assert!(index.is_empty());
        assert!(index.search("anything", 5).is_empty());
        let chunks = corpus();
        let index = Bm25Index::build(&chunks);
        assert!(index.search("   ", 5).is_empty());
        assert!(index.search("coverage", 0).is_empty());
    }
}
