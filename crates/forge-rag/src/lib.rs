//! # forge-rag — the retrieval plane
//!
//! Every forge domain (a public workspace, a cratelab crate, ops, and the
//! retrieval plane itself) is grounded the same way:
//!
//! 1. [`ingest`] turns source trees into provenance-rich [`Chunk`]s.
//! 2. [`index`] builds a deterministic BM25 [`Bm25Index`] over them.
//! 3. [`retrieve`] returns a ranked [`EvidencePacket`] with full provenance.
//! 4. [`prompt`] renders a scripted [`prompt::PromptTemplate`] over that packet.
//!
//! The plane is deliberately I/O-free at the core: ingestion takes in-memory
//! files, so the whole pipeline is unit-testable and reproducible. Hitting the
//! network or a remote (Modal / Kaggle) is the sidecar's job, not this crate's.
//!
//! Invariant: **a missing retrieval result is no evidence, never proof that a
//! behaviour is absent.** An empty packet must be surfaced as such.

pub mod index;
pub mod ingest;
pub mod prompt;
pub mod retrieve;

pub use index::{tokenize, Bm25Index, Hit};
pub use ingest::{chunk_lines, ingest, Chunk, SourceSpec};
pub use prompt::{crate_evolution_template, render, PromptTemplate, CRATE_EVOLUTION_PROMPT};
pub use retrieve::{retrieve, EvidencePacket, Snippet};
