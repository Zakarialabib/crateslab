//! # forge-protocol — the forge's strict IPC contract
//!
//! The forge is a generator/validator loop: an agent proposes, the validator
//! compiles and lints, and refusals flow back as data. That loop only stays
//! auditable if both halves agree on one wire shape, so the shape lives here
//! as **pure data** — [`ProposalArtifact`] and everything it contains.
//!
//! Nothing in this crate performs I/O or holds behaviour beyond a few accessor
//! helpers, so it is the natural leaf dependency of both `forge-rag` (which
//! chunks the code a proposal is about) and `forge-core` (which drives the
//! state machine that produces and validates one).
//!
//! ## Invariant
//!
//! A generator's reply is only ever parsed into a [`ProposalArtifact`]. There
//! is no field for prose, markdown or conversational text: a proposal that
//! cannot be expressed here is not a valid proposal.

pub mod artifact;

pub use artifact::{CargoTomlPatch, Dependency, FileAction, FileProposal, ProposalArtifact};
