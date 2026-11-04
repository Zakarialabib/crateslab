export const fleetWorkflows = [
  { id: "research-summary", label: "Research", group: "Understand", local: "Inventory source and public API", live: "Map purpose, boundaries, related crates, and open research questions.", steps: ["Scope one crate", "Collect source evidence", "Research context", "Review claims", "Capture learning"] },
  { id: "rustdoc-audit", label: "Documentation", group: "Improve", local: "Find public API documentation candidates", live: "Identify missing or unclear rustdoc with exact source evidence.", steps: ["Scan public surface", "Check existing docs", "Draft doc recommendations", "Review evidence", "Capture learning"] },
  { id: "feature-study", label: "Feature study", group: "Plan", local: "Map API and implementation files", live: "Trace a feature across crates, contracts, tests, and user-facing behavior.", steps: ["State feature question", "Map code path", "Trace dependencies", "Identify acceptance evidence", "Capture learning"] },
  { id: "bug-investigation", label: "Bug investigation", group: "Diagnose", local: "Collect risk markers and nearby API evidence", live: "Build ranked hypotheses from supplied symptoms and source evidence; separate facts from guesses.", steps: ["Record symptom", "Map affected API", "Inspect error paths", "Rank hypotheses", "Define reproduction"] },
  { id: "debug-plan", label: "Debug plan", group: "Diagnose", local: "Collect panic, unwrap, TODO, and FIXME candidates", live: "Propose a minimal diagnostic sequence, expected observations, and safe stop conditions.", steps: ["Record failure", "Find call path", "Select observations", "Reproduce locally", "Capture outcome"] },
  { id: "issue-triage", label: "Issue triage", group: "Plan", local: "Prepare crate context; issue content is not fetched", live: "Classify supplied issue text into impact, evidence, missing data, and a next action.", steps: ["Paste issue context", "Identify affected crate", "Check source evidence", "Classify impact", "Review recommendation"] },
  { id: "knowledge-synthesis", label: "Knowledge synthesis", group: "Learn", local: "Build source and dependency inventory", live: "Synthesize source-grounded knowledge with provenance, confidence, and unresolved questions.", steps: ["Select evidence scope", "Retrieve source passages", "Synthesize contracts", "Check provenance", "Record reusable note"] },
  {
    id: "cross-language-research",
    label: "Cross-language research",
    group: "Research",
    local: "Inventory the crate's public API, docs, tests and feature surface",
    live: "Find comparable SDKs in other languages, extract what they excel at, diff against the crate, and write adoption notes (docs missing, features, issues).",
    steps: [
      "Pick a capability",
      "Find comparable SDKs",
      "Extract the excellent parts",
      "Diff against the crate",
      "Write adoption notes",
      "Capture learning",
    ],
    refs: ["https://github.com/google/artemis", "https://github.com/morluto/rea"],
  },
] as const;

export type FleetWorkflowId = (typeof fleetWorkflows)[number]["id"];
