//! Registry discovery against the crates.io **sparse index**
//! (`index.crates.io`). Not the crates.io REST API on purpose: the index
//! serves plain JSON lines, needs no auth, and is the same transport cargo
//! itself uses, so it is stable under load.
//!
//! Enabled with `--features registry` (pulls `ureq`). Never enabled in the
//! default test profile so `cargo test --workspace` runs fully offline.

use lab_errors::{LabError, LabResult};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read as _;

#[derive(Deserialize)]
struct IndexEntry {
    vers: String,
    /// `yanked` / `deleted` entries are skipped.
    #[serde(default)]
    yanked: bool,
}

/// Minimal version ordering key (major, minor, patch) — enough for "latest".
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
struct Key(u64, u64, u64);

fn parse_key(v: &str) -> Key {
    let core = v.split(['-', '+']).next().unwrap_or("0.0.0");
    let mut it = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    Key(it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

fn index_path(crate_name: &str) -> String {
    // crates.io sparse index layout: 2 chars / 2 chars / full name.
    // Names shorter than 4 chars live under 3/<name>.
    let n = crate_name.len();
    if n < 4 {
        format!("3/{crate_name}")
    } else {
        format!("{}/{}/{}", &crate_name[..2], &crate_name[2..4], crate_name)
    }
}

/// Latest non-yanked version per crate name.
pub fn latest_versions(names: &[&str]) -> LabResult<BTreeMap<String, String>> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("cratelab/0.1 (lab discovery; +https://github.com/Zakarialabib)")
        .build();
    let mut out = BTreeMap::new();
    for name in names {
        let url = format!("https://index.crates.io/{}", index_path(name));
        let res = agent.get(&url).call();
        match res {
            Ok(r) => {
                let mut text = String::new();
                r.into_reader()
                    .read_to_string(&mut text)
                    .map_err(|e| LabError::Discovery { phase: "registry".into(), message: e.to_string() })?;
                let mut best: Option<(Key, String)> = None;
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    if let Ok(entry) = serde_json::from_str::<IndexEntry>(line) {
                        if entry.yanked {
                            continue;
                        }
                        let k = parse_key(&entry.vers);
                        if best.as_ref().map(|(bk, _)| k > *bk).unwrap_or(true) {
                            best = Some((k, entry.vers));
                        }
                    }
                }
                if let Some((_, v)) = best {
                    out.insert(name.to_string(), v);
                }
            }
            Err(ureq::Error::Status(404, _)) => {
                // Unknown crate on the registry: not fatal for matching.
                continue;
            }
            Err(e) => {
                return Err(LabError::Discovery {
                    phase: "registry".into(),
                    message: format!("{name}: {e}"),
                })
            }
        }
    }
    Ok(out)
}
