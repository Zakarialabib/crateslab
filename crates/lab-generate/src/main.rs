//! `lab-generate` CLI — turn a typed command enum into the generated
//! manifest layer:
//!
//! ```text
//! lab-generate crates/core/src/command.rs CommandId \
//!     --prefix tv --out manifests/generated/tv-rs.commands.json \
//!     --skeleton manifests/generated/tv-rs.skeleton.toml
//! ```

use lab_generate::generate_from_source;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!(
            "usage: lab-generate <input.rs> <EnumName> --prefix <p> [--crate <path>] \
             [--out <generated.json>] [--skeleton <skeleton.toml>]"
        );
        std::process::exit(2);
    }
    let input = &args[0];
    let enum_name = &args[1];

    let mut prefix = String::from("x");
    let mut out: Option<PathBuf> = None;
    let mut skeleton: Option<PathBuf> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--prefix" => prefix = args.get(i + 1).cloned().unwrap_or(prefix),
            "--out" => out = args.get(i + 1).map(PathBuf::from),
            "--skeleton" => skeleton = args.get(i + 1).map(PathBuf::from),
            other => {
                eprintln!("unknown flag '{other}'");
                std::process::exit(2);
            }
        }
        i += 2;
    }

    let source = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {input}: {e}");
            std::process::exit(3);
        }
    };

    match generate_from_source(&source, enum_name, &prefix, input) {
        Ok(gen) => {
            let json = serde_json::to_string_pretty(&gen).expect("serializable");
            match out {
                Some(p) => {
                    std::fs::write(&p, json).expect("write generated json");
                    eprintln!("wrote {}", p.display());
                }
                None => println!("{json}"),
            }
            if let Some(sp) = skeleton {
                let ws = prefix.clone();
                std::fs::write(
                    &sp,
                    lab_generate::authoring_skeleton(&gen, &format!("{ws}-rs"), "https://github.com/Zakarialabib"),
                )
                .expect("write skeleton");
                eprintln!("wrote {}", sp.display());
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(3);
        }
    }
}
