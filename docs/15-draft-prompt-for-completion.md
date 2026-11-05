# SYSTEM CONTEXT
You are an expert Rust systems architect and AI engineer. We are building the foundational layer for `cratesLab`, an agentic forge that studies, generates, tests, and evolves Rust crates. 

Your task is to implement and wire together three foundational crates in our Rust workspace: `forge-protocol`, `forge-rag`, and `forge-core`. The files exist but are currently empty. You must write the complete, compilable Rust code, set up the `Cargo.toml` dependencies, and wire them together.

# WORKSPACE STRUCTURE & DEPENDENCIES
The workspace contains the following crates. Pay strict attention to their dependencies:

1. `forge-protocol`: Pure data structures. No complex logic. 
   - Dependencies: `serde`, `serde_json`, `thiserror`.
2. `forge-rag`: AST-aware code chunker.
   - Dependencies: `forge-protocol` (path = "../forge-protocol"), `syn` (version "2.0", features = ["full", "parsing", "extra-traits"]), `quote`, `proc-macro2`, `serde`, `serde_json`.
3. `forge-core`: The orchestrator state machine.
   - Dependencies: `forge-protocol` (path = "../forge-protocol"), `forge-rag` (path = "../forge-rag"), `serde`, `serde_json`, `thiserror`.

# CRATE SPECIFICATIONS

## 1. `forge-protocol` (src/lib.rs)
This crate defines the strict JSON-RPC/IPC contract. The AI Generator MUST output data matching these schemas. No markdown, no conversational text.
Implement the following structs and enums with `#[derive(Debug, Clone, Serialize, Deserialize)]`:
- `ProposalArtifact`: Contains `crate_name` (String), `description` (String), `files` (Vec<FileProposal>), `cargo_toml_patch` (CargoTomlPatch), and `reasoning` (String).
- `FileProposal`: Contains `path` (String, relative to crate root), `action` (FileAction), and `content` (String, complete valid Rust code).
- `FileAction`: Enum with `Create`, `Update`, `Delete`.
- `CargoTomlPatch`: Contains `add_dependencies` (Vec<Dependency>), `add_dev_dependencies` (Vec<Dependency>), `add_features` (Vec<String>).
- `Dependency`: Contains `name` (String), `version` (Option<String>), `path` (Option<String>), `features` (Vec<String>).

## 2. `forge-rag` (src/chunker.rs & src/lib.rs)
This crate parses Rust code into semantic chunks using `syn`.
Implement the following in `src/chunker.rs`:
- `CodeItemKind`: Enum with `Module`, `Struct`, `Trait`, `Function`, `ImplBlock`, `Unknown`.
- `CodeChunk`: Struct with `id` (String), `file_path` (PathBuf), `item_kind` (CodeItemKind), `name` (String), `signature` (String), `doc_comment` (Option<String>), `source_code` (String), `module_path` (String).
- `parse_rust_file(file_path: PathBuf, source: &str) -> Vec<CodeChunk>`: Parses the source using `syn::parse_file`. Iterates over `ast.items` and extracts Functions, Structs, Traits, and Modules.
- `extract_function`, `extract_struct`, `extract_trait`, `extract_module`: Helper functions that use `quote::quote!` to generate the `signature` and `source_code` strings, and extract doc comments.
- `extract_docs(attrs: &[syn::Attribute]) -> Option<String>`: Extracts `///` doc comments from attributes.
Expose these via `src/lib.rs`.

## 3. `forge-core` (src/orchestrator.rs & src/lib.rs)
This crate manages the agentic state machine and self-healing loop.
Implement the following in `src/orchestrator.rs`:
- `ValidationError`: Enum using `thiserror::Error` with variants: `CompilationFailed { stderr: String }`, `ClippyViolation { message: String }`, `SchemaViolation { field: String }`, `ScenarioFailed { output: String }`.
- `ForgeStateName`: Enum with `Planning`, `Retrieving`, `Drafting`, `Validating`, `Healing`, `Completed`, `Failed`.
- `ForgeStep`: Struct with `state` (ForgeStateName), `timestamp` (u64), `input_context_hash` (String), `output_artifact_hash` (Option<String>), `error` (Option<ValidationError>).
- `ForgeContext`: Struct with `goal` (String), `history` (Vec<ForgeStep>), `retrieved_evidence` (Vec<String> - placeholder for EvidencePackets), `current_proposal` (Option<ProposalArtifact>), `retry_count` (u8), `max_retries` (u8).
- Implement methods on `ForgeContext`: `record_step(&mut self, state: ForgeStateName, error: Option<ValidationError>)` and `can_retry(&self) -> bool`.
Expose these via `src/lib.rs`.

# STRICT RULES & CONSTRAINTS
1. DO NOT use placeholder comments like `// TODO: implement`. Write the full, working logic.
2. Ensure all structs that cross crate boundaries derive `Serialize` and `Deserialize`.
3. Ensure all error types derive `Debug` and implement `std::error::Error` (via `thiserror`).
4. Use `syn` 2.0 syntax (e.g., `attr.path().is_ident("doc")` instead of `attr.path.is_ident("doc")`).
5. Make sure `lib.rs` in each crate properly declares and exposes the modules.

# EXECUTION STEPS
Please execute this task in the following order. Stop and wait for my confirmation after each step if you are in an interactive environment, otherwise output all code clearly separated by file paths.

Step 1: Update the `Cargo.toml` for all three crates with the exact dependencies specified.
Step 2: Implement `forge-protocol` completely.
Step 3: Implement `forge-rag` (chunker.rs and lib.rs).
Step 4: Implement `forge-core` (orchestrator.rs and lib.rs).
Step 5: Run a mental `cargo check` to ensure all types, imports, and crate dependencies resolve correctly. Fix any issues before outputting the final code.