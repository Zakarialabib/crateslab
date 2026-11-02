//! # lab-generate — the compiler does the typing, the lab does the rest
//!
//! Workspaces already declare their command surface **in Rust**, as typed
//! enums (`tv-rs-core::CommandId`, the cctv equivalent) with doc comments.
//! This crate is the bridge: it parses that source with `syn` and emits the
//! **generated manifest layer** — ids, variants, docs, argument shapes —
//! so the authored TOML only carries what a compiler cannot know
//! (labels, danger, timeouts, limits, mock bindings).
//!
//! Mapping rules:
//!
//! * `PowerOff` → snake_case → id `tv.power_off` (prefix + variant).
//! * `/// doc comments` → the command's `help` text.
//! * Struct variants' named fields → typed [`ArgSpec`]s via a type
//!   heuristic (`Ipv4Addr` → `ipv4`, `String` → `str`, `bool` → `bool`, …).
//! * Authored manifests that prefer prettier ids (`tv.power.off`) pin the
//!   correspondence with `variant = "PowerOff"`; [`lab_manifest::LabManifest::apply_generated`]
//!   matches on id **or** variant.

use lab_errors::{LabError, LabResult};
use lab_manifest::{ArgSpec, ArgType, GeneratedCommand, GeneratedManifest};
use syn::Item;

/// Parse `source`, find `enum_name`, and produce the generated layer.
pub fn generate_from_source(
    source: &str,
    enum_name: &str,
    prefix: &str,
    source_path: &str,
) -> LabResult<GeneratedManifest> {
    let file = syn::parse_file(source)
        .map_err(|e| LabError::Manifest { path: source_path.into(), problem: format!("not parseable Rust: {e}") })?;

    let item_enum = file
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Enum(e) => Some(e),
            _ => None,
        })
        .find(|e| e.ident == enum_name)
        .ok_or_else(|| LabError::Manifest {
            path: source_path.into(),
            problem: format!("enum '{enum_name}' not found"),
        })?;

    let mut commands = Vec::new();
    for variant in &item_enum.variants {
        let variant_name = variant.ident.to_string();
        let doc = extract_doc(&variant.attrs);
        let id = format!("{prefix}.{}", to_snake(&variant_name));
        let args = extract_args(variant);
        commands.push(GeneratedCommand {
            id,
            variant: variant_name,
            doc,
            crate_path: Some(source_path.rsplit("::").next().unwrap_or(source_path).to_string()),
            args,
        });
    }

    if commands.is_empty() {
        return Err(LabError::Manifest {
            path: source_path.into(),
            problem: format!("enum '{enum_name}' has no variants"),
        });
    }

    Ok(GeneratedManifest {
        source: source_path.to_string(),
        generated_at: iso_now(),
        commands,
    })
}

/// Render the authoring-layer TOML skeleton for the generated commands.
/// Comments mark everything the human should fill in.
pub fn authoring_skeleton(gen: &GeneratedManifest, workspace: &str, repo: &str) -> String {
    let mut out = String::new();
    out.push_str("# lab authoring layer — generated skeleton, fill in the TODOs\n");
    out.push_str(&format!("# workspace: {workspace}\n\n"));
    out.push_str("schema_version = 1\n\n");
    out.push_str("[workspace]\n");
    out.push_str(&format!("name = \"{workspace}\"\n"));
    out.push_str("prefix = \"TODO\"\n");
    out.push_str(&format!("repo = \"{repo}\"\n"));
    out.push_str("facade = \"TODO\"\n");
    out.push_str("core = \"TODO\"\n");
    out.push_str("version = \"TODO\"\n\n");
    for c in &gen.commands {
        out.push_str("[[commands]]\n");
        out.push_str(&format!("id = \"{}\"\n", c.id));
        out.push_str(&format!("label = \"{}\"  # TODO: human label\n", c.id));
        out.push_str("# kind = \"action\" | \"query\" | \"config\"\n");
        out.push_str("# danger = \"safe\" | \"attention\" | \"destructive\"\n");
        if let Some(d) = &c.doc {
            out.push_str(&format!("# from doc comment: {d}\n"));
        }
        out.push_str("# timeout_ms = 8000\n\n");
    }
    out
}

fn extract_doc(attrs: &[syn::Attribute]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for a in attrs {
        if a.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &a.meta {
                if let syn::Expr::Lit(lit) = &nv.value {
                    if let syn::Lit::Str(s) = &lit.lit {
                        let line = s.value();
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            lines.push(trimmed.to_string());
                        }
                    }
                }
            }
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join(" "))
    }
}

fn to_snake(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        if ch.is_uppercase() {
            let prev_lower = i > 0 && chars[i - 1].is_lowercase();
            let next_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();
            if i > 0 && (prev_lower || next_lower) {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(*ch);
        }
    }
    out
}

fn extract_args(variant: &syn::Variant) -> Vec<ArgSpec> {
    let syn::Fields::Named(named) = &variant.fields else {
        return Vec::new();
    };
    named
        .named
        .iter()
        .map(|f| {
            let name = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default();
            let ty = type_of(&f.ty);
            ArgSpec {
                name,
                r#type: ty,
                required: !is_option(&f.ty),
                options: Vec::new(),
                default: None,
                help: extract_doc(&f.attrs),
                min: None,
                max: None,
                min_len: None,
                max_len: None,
            }
        })
        .collect()
}

fn is_option(ty: &syn::Type) -> bool {
    quote_type(ty).replace(' ', "").starts_with("Option<")
}

fn type_of(ty: &syn::Type) -> ArgType {
    let s = quote_type(ty).replace(' ', "");
    // Unwrap Option<T> — the inner type drives the widget.
    let inner: &str = s
        .strip_prefix("Option<")
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap_or(&s);
    match inner {
        "bool" => ArgType::Bool,
        "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize" => ArgType::Int,
        "f32" | "f64" => ArgType::Float,
        "Ipv4Addr" | "IpAddr" => ArgType::Ipv4,
        "PathBuf" => ArgType::Path,
        "Url" => ArgType::Url,
        "Value" | "Json" => ArgType::Json,
        "MacAddr" => ArgType::Mac,
        _ => ArgType::Str,
    }
}

/// Render a type without hand-rolled recursion: `Type::to_token_stream`
/// (via `quote::ToTokens`) gives the exact tokens; we squeeze whitespace
/// to a canonical form.
fn quote_type(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream()
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn iso_now() -> String {
    // Deterministic, dependency-free timestamp (std only): unix seconds.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("testdata/tv_command_id.rs");

    #[test]
    fn parses_variants_docs_and_fields() {
        let gen = generate_from_source(SAMPLE, "CommandId", "tv", "crates/core/src/command.rs::CommandId")
            .expect("parses");
        assert_eq!(gen.commands.len(), 6);

        let wake = gen.commands.iter().find(|c| c.variant == "Wake").unwrap();
        assert_eq!(wake.id, "tv.wake");
        assert_eq!(wake.doc.as_deref(), Some("Wake-on-LAN magic packet, no control protocol needed."));

        let off = gen.commands.iter().find(|c| c.variant == "PowerOff").unwrap();
        assert_eq!(off.id, "tv.power_off");

        let press = gen.commands.iter().find(|c| c.variant == "KeyPress").unwrap();
        assert_eq!(press.args.len(), 2);
        let key = press.args.iter().find(|a| a.name == "key").unwrap();
        assert_eq!(key.r#type, ArgType::Str);
        assert!(key.required);
        let repeat = press.args.iter().find(|a| a.name == "repeat").unwrap();
        assert_eq!(repeat.r#type, ArgType::Int);
        assert!(!repeat.required, "Option<u16> becomes optional");

        let launch = gen.commands.iter().find(|c| c.variant == "Launch").unwrap();
        let app = launch.args.iter().find(|a| a.name == "app_id").unwrap();
        assert_eq!(app.r#type, ArgType::Str);
        assert_eq!(app.help.as_deref(), Some("Vendor app id, e.g. netflix."));
    }

    #[test]
    fn missing_enum_is_manifest_error() {
        let err = generate_from_source(SAMPLE, "NotAnEnum", "tv", "x.rs").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn snake_case_multi_word() {
        assert_eq!(to_snake("PowerOff"), "power_off");
        assert_eq!(to_snake("Wake"), "wake");
        assert_eq!(to_snake("EnsureDesiredState"), "ensure_desired_state");
    }
}
