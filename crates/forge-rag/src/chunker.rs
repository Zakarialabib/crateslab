//! AST-aware chunking — Rust source into semantic units.
//!
//! The line-window chunker in [`crate::ingest`] is language-agnostic and fast,
//! but it slices a function in half and never knows that two lines belong to
//! the same `impl`. This module is the typed alternative: it parses a file with
//! `syn` and emits one [`CodeChunk`] per *semantic* item — a function, a struct,
//! a trait, an impl block, or a module — each carrying its own signature, its
//! doc comment and the module path it lives at.
//!
//! Chunks are produced deterministically (source order, depth-first through
//! inline modules), so the same file always yields the same ids. Parsing a file
//! that is not valid Rust yields **no** chunks rather than a partial guess: the
//! caller falls back to the line window instead.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use syn::{File, Item, ItemFn, ItemImpl, ItemMod, ItemStruct, ItemTrait, Type};

/// The semantic kind of a code chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeItemKind {
    /// A `mod` declaration (inline body or `mod x;`).
    Module,
    /// A `struct` definition.
    Struct,
    /// A `trait` definition.
    Trait,
    /// A free function.
    Function,
    /// An `impl` block.
    ImplBlock,
    /// Anything else the chunker does not model.
    Unknown,
}

/// A semantically meaningful chunk of Rust code, ready for indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeChunk {
    /// Stable id: `{file}::{module_path}::{name}`.
    pub id: String,
    /// The file the chunk was parsed from.
    pub file_path: PathBuf,
    /// What kind of item this is.
    pub item_kind: CodeItemKind,
    /// The item's name (`execute`, `ForgeEngine`, `Foo` for `impl Foo`).
    pub name: String,
    /// The item's signature only, e.g. `pub fn execute(&self) -> Output`.
    pub signature: String,
    /// The joined `///` lines above the item, when present.
    pub doc_comment: Option<String>,
    /// The full, valid Rust source for this item.
    pub source_code: String,
    /// Where the item lives, e.g. `crate::forge_core::orchestrator`.
    pub module_path: String,
    /// Populated later by the embedding engine; never serialised.
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
}

/// The module path every file's items are rooted at.
const CRATE_ROOT: &str = "crate";

/// Parse `source` and return one chunk per semantic item, in source order.
///
/// Returns an empty vector when `source` is not valid Rust — a syntax error is
/// not a partial parse, and the caller should fall back to line chunking.
pub fn parse_rust_file(file_path: PathBuf, source: &str) -> Vec<CodeChunk> {
    let ast: File = match syn::parse_file(source) {
        Ok(ast) => ast,
        Err(_) => return Vec::new(),
    };

    let mut chunks = Vec::new();
    collect_items(&file_path, &ast.items, CRATE_ROOT, &mut chunks);
    chunks
}

/// Walk `items` at `module_path`, depth-first, appending one chunk per item.
fn collect_items(file_path: &Path, items: &[Item], module_path: &str, out: &mut Vec<CodeChunk>) {
    for item in items {
        match item {
            Item::Fn(item_fn) => out.push(extract_function(file_path, item_fn, module_path)),
            Item::Struct(item_struct) => {
                out.push(extract_struct(file_path, item_struct, module_path));
            }
            Item::Trait(item_trait) => out.push(extract_trait(file_path, item_trait, module_path)),
            Item::Impl(item_impl) => out.push(extract_impl(file_path, item_impl, module_path)),
            Item::Mod(item_mod) => extract_module(file_path, item_mod, module_path, out),
            _ => continue,
        }
    }
}

/// A free function chunk.
fn extract_function(file_path: &Path, item: &ItemFn, module_path: &str) -> CodeChunk {
    let name = item.sig.ident.to_string();
    CodeChunk {
        id: chunk_id(file_path, module_path, &name),
        file_path: file_path.to_path_buf(),
        item_kind: CodeItemKind::Function,
        signature: quote::quote!(#item.sig).to_string(),
        doc_comment: extract_docs(&item.attrs),
        source_code: quote::quote!(#item).to_string(),
        name,
        module_path: module_path.to_string(),
        embedding: None,
    }
}

/// A struct definition chunk.
fn extract_struct(file_path: &Path, item: &ItemStruct, module_path: &str) -> CodeChunk {
    let ItemStruct {
        attrs,
        vis,
        ident,
        generics,
        ..
    } = item;
    let name = ident.to_string();
    CodeChunk {
        id: chunk_id(file_path, module_path, &name),
        file_path: file_path.to_path_buf(),
        item_kind: CodeItemKind::Struct,
        signature: quote::quote!(#vis struct #ident #generics).to_string(),
        doc_comment: extract_docs(attrs),
        source_code: quote::quote!(#item).to_string(),
        name,
        module_path: module_path.to_string(),
        embedding: None,
    }
}

/// A trait definition chunk.
fn extract_trait(file_path: &Path, item: &ItemTrait, module_path: &str) -> CodeChunk {
    let ItemTrait {
        attrs,
        vis,
        ident,
        generics,
        ..
    } = item;
    let name = ident.to_string();
    CodeChunk {
        id: chunk_id(file_path, module_path, &name),
        file_path: file_path.to_path_buf(),
        item_kind: CodeItemKind::Trait,
        signature: quote::quote!(#vis trait #ident #generics).to_string(),
        doc_comment: extract_docs(attrs),
        source_code: quote::quote!(#item).to_string(),
        name,
        module_path: module_path.to_string(),
        embedding: None,
    }
}

/// An `impl` block chunk, named after the type it implements for.
fn extract_impl(file_path: &Path, item: &ItemImpl, module_path: &str) -> CodeChunk {
    let ItemImpl {
        attrs,
        generics,
        trait_,
        self_ty,
        ..
    } = item;
    let name = type_name(self_ty);
    // `impl Foo`, `impl Trait for Foo`, or the negative `impl !Trait for Foo`.
    let signature = match trait_ {
        Some((Some(_), path, _)) => quote::quote!(impl #generics ! #path for #self_ty).to_string(),
        Some((None, path, _)) => quote::quote!(impl #generics #path for #self_ty).to_string(),
        None => quote::quote!(impl #generics #self_ty).to_string(),
    };
    CodeChunk {
        id: chunk_id(file_path, module_path, &name),
        file_path: file_path.to_path_buf(),
        item_kind: CodeItemKind::ImplBlock,
        signature,
        doc_comment: extract_docs(attrs),
        source_code: quote::quote!(#item).to_string(),
        name,
        module_path: module_path.to_string(),
        embedding: None,
    }
}

/// A module chunk, plus every item declared inside an inline body.
fn extract_module(file_path: &Path, item: &ItemMod, module_path: &str, out: &mut Vec<CodeChunk>) {
    let ItemMod {
        attrs, vis, ident, ..
    } = item;
    let name = ident.to_string();
    let child_path = format!("{module_path}::{name}");

    out.push(CodeChunk {
        id: chunk_id(file_path, module_path, &name),
        file_path: file_path.to_path_buf(),
        item_kind: CodeItemKind::Module,
        signature: quote::quote!(#vis mod #ident).to_string(),
        doc_comment: extract_docs(attrs),
        source_code: quote::quote!(#item).to_string(),
        name,
        module_path: module_path.to_string(),
        embedding: None,
    });

    if let Some((_, items)) = &item.content {
        collect_items(file_path, items, &child_path, out);
    }
}

/// Extract `///` doc comments from an item's attributes.
pub fn extract_docs(attrs: &[syn::Attribute]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        return Some(lit_str.value().trim().to_string());
                    }
                }
            }
            None
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

/// `{file}::{module_path}::{name}` — stable across runs, unique per item.
fn chunk_id(file_path: &Path, module_path: &str, name: &str) -> String {
    format!("{}::{module_path}::{name}", file_path.display())
}

/// The last path segment of a type, so `impl crate::a::Foo<T>` is named `Foo`.
fn type_name(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| quote::quote!(#ty).to_string()),
        other => quote::quote!(#other).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
/// A covered cell.
#[derive(Debug)]
pub struct Cell {
    pub passed: bool,
}

/// What a rule set decides.
pub trait RuleSet {
    fn evaluate(&self) -> bool;
}

mod inner {
    /// A nested helper.
    pub fn helper() -> u32 { 1 }
}

impl Cell {
    /// Whether the cell is green.
    pub fn is_green(&self) -> bool { self.passed }
}

/// Entry point.
pub fn update_ledger(cells: &[Cell]) -> usize {
    cells.iter().filter(|cell| cell.passed).count()
}
"#;

    fn chunks() -> Vec<CodeChunk> {
        parse_rust_file(PathBuf::from("src/lib.rs"), SAMPLE)
    }

    #[test]
    fn every_semantic_item_becomes_a_chunk() {
        let kinds: Vec<CodeItemKind> = chunks().iter().map(|chunk| chunk.item_kind).collect();
        assert!(kinds.contains(&CodeItemKind::Struct));
        assert!(kinds.contains(&CodeItemKind::Trait));
        assert!(kinds.contains(&CodeItemKind::Module));
        assert!(kinds.contains(&CodeItemKind::ImplBlock));
        assert!(kinds.contains(&CodeItemKind::Function));
    }

    #[test]
    fn items_carry_their_signature_and_doc_comment() {
        let chunks = chunks();
        let cell = chunks
            .iter()
            .find(|chunk| chunk.name == "Cell" && chunk.item_kind == CodeItemKind::Struct)
            .expect("the Cell struct chunk");
        assert_eq!(cell.doc_comment.as_deref(), Some("A covered cell."));
        assert!(cell.signature.contains("pub struct Cell"));
        assert!(cell.source_code.contains("struct Cell"));
        assert!(cell.source_code.contains("passed"));

        let ledger = chunks
            .iter()
            .find(|chunk| chunk.name == "update_ledger")
            .expect("the update_ledger chunk");
        assert_eq!(ledger.doc_comment.as_deref(), Some("Entry point."));
        assert!(ledger.signature.contains("pub fn update_ledger"));
    }

    #[test]
    fn impl_blocks_are_named_after_their_type() {
        let chunks = chunks();
        let impl_block = chunks
            .iter()
            .find(|chunk| chunk.item_kind == CodeItemKind::ImplBlock)
            .expect("the impl block chunk");
        assert_eq!(impl_block.name, "Cell");
        assert!(impl_block.signature.starts_with("impl Cell"));
        assert_eq!(impl_block.module_path, "crate");
    }

    #[test]
    fn inline_modules_nest_their_children_paths() {
        let chunks = chunks();
        let helper = chunks
            .iter()
            .find(|chunk| chunk.name == "helper")
            .expect("the nested helper chunk");
        assert_eq!(helper.module_path, "crate::inner");
        assert_eq!(helper.id, "src/lib.rs::crate::inner::helper");
    }

    #[test]
    fn invalid_rust_yields_no_chunks() {
        assert!(parse_rust_file(PathBuf::from("src/lib.rs"), "pub fn broken( {").is_empty());
    }
}
