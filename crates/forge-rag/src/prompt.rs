//! Scripted prompt rendering.
//!
//! A [`PromptTemplate`] is the authored half of a prompt; the [`EvidencePacket`]
//! is the retrieved half. [`render`] joins them into one deterministic string.
//! The crate-evolution template is shipped with the crate
//! ([`CRATE_EVOLUTION_PROMPT`]) so the research contract travels with the code
//! that renders it instead of living in a loose markdown file.

use crate::retrieve::EvidencePacket;
use serde::{Deserialize, Serialize};

/// The crate-evolution research prompt, embedded at build time.
pub const CRATE_EVOLUTION_PROMPT: &str = include_str!("../prompts/crate-evolution-research.md");

/// A scripted prompt template: a role, ordered context sections, and an output
/// contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Stable template id.
    pub id: String,
    /// The system role block (may be a full authored contract).
    pub system_role: String,
    /// Ordered context sections the prompt must fill.
    #[serde(default)]
    pub sections: Vec<String>,
    /// The headings the model must return, in order.
    #[serde(default)]
    pub output_contract: Vec<String>,
}

/// The crate-evolution research template.
pub fn crate_evolution_template() -> PromptTemplate {
    PromptTemplate {
        id: "crate-evolution-research".into(),
        system_role: CRATE_EVOLUTION_PROMPT.into(),
        sections: vec![
            "Question, target crate(s), task class, and intended deliverable".into(),
            "System boundaries and dependency direction".into(),
            "Target manifest, crate-level module docs, and relevant public declarations".into(),
            "Direct dependency/dependent evidence where a contract crosses a boundary".into(),
            "Relevant manifests, scenarios, coverage and conformance outputs".into(),
            "Retrieved snippets, with source, crate, revision and line metadata".into(),
        ],
        output_contract: vec![
            "Finding — one-paragraph answer and confidence (high/medium/low)".into(),
            "Evidence — claim, source path/line, and what it proves".into(),
            "Cross-crate effects — direct dependencies/consumers only, or 'none found'".into(),
            "Coverage and CI implications — existing evidence and missing checks".into(),
            "Recommended next actions — at most five ordered, reviewable steps".into(),
            "Prompt/context gaps — facts needed for a stronger answer".into(),
        ],
    }
}

/// Render a prompt from a template and a grounded evidence packet.
///
/// A missing result is rendered as *no evidence*, never as proof of absence.
pub fn render(template: &PromptTemplate, packet: &EvidencePacket, question: &str) -> String {
    let mut out = String::new();
    out.push_str(template.system_role.trim());
    out.push_str("\n\n## Question\n\n");
    out.push_str(question.trim());

    out.push_str("\n\n## Required context order\n\n");
    for (index, section) in template.sections.iter().enumerate() {
        out.push_str(&format!("{}. {}\n", index + 1, section));
    }

    out.push_str("\n## Retrieved evidence\n\n");
    if packet.is_empty() {
        out.push_str(
            "(none retrieved — treat this as no evidence, not as proof that the behaviour is absent)\n",
        );
    } else {
        for (index, snippet) in packet.snippets.iter().enumerate() {
            out.push_str(&format!(
                "{}. {}:{} @ {} (score {:.3})\n{}\n\n",
                index + 1,
                snippet.path,
                snippet.start_line,
                snippet.revision,
                snippet.score,
                snippet.text,
            ));
        }
    }

    out.push_str("## Required output\n\n");
    for heading in &template.output_contract {
        out.push_str(&format!("- {}\n", heading));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Bm25Index;
    use crate::ingest::{ingest, SourceSpec};
    use crate::retrieve::retrieve;

    #[test]
    fn embedded_template_has_a_contract() {
        let template = crate_evolution_template();
        assert_eq!(template.id, "crate-evolution-research");
        assert!(template.system_role.contains("System role"));
        assert_eq!(template.output_contract.len(), 6);
    }

    #[test]
    fn render_includes_the_question_and_evidence() {
        let spec = SourceSpec {
            id: "crates".into(),
            root: "CRATELAB_ROOT".into(),
            extensions: vec![".rs".into()],
            pristine: true,
        };
        let chunks = ingest(
            &spec,
            "rev-1",
            &[(
                "coverage.rs".to_string(),
                "pub fn coverage_ledger_update() { /* coverage ledger */ }\n".to_string(),
            )],
        );
        let index = Bm25Index::build(&chunks);
        let packet = retrieve(&index, &chunks, "coverage ledger", 3);

        let prompt = render(&crate_evolution_template(), &packet, "Audit the coverage ledger.");
        assert!(prompt.contains("Audit the coverage ledger."));
        assert!(prompt.contains("coverage.rs:1 @ rev-1"));
        assert!(prompt.contains("## Required output"));
    }

    #[test]
    fn render_marks_an_empty_packet_as_no_evidence() {
        let packet = EvidencePacket::empty("nothing");
        let prompt = render(&crate_evolution_template(), &packet, "Anything?");
        assert!(prompt.contains("no evidence"));
    }
}
