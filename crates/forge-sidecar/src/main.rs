//! `forge-sidecar` — newline-delimited JSON-RPC over stdio.
//!
//! Reads one JSON [`Request`](forge_sidecar::rpc::Request) per line from stdin,
//! dispatches it, and writes one JSON response per line to stdout. All logic
//! lives in the [`forge_sidecar`] library; this binary is only the transport.
//!
//! The one thing it owns is the production wiring: the process environment
//! (layered over `FORGE_ENV_FILE` when set) and a single shared HTTP client.
//! That is what keeps credentials out of every adapter and out of the wire
//! format — a request names a provider, never a key.

use std::io::{BufRead, Write};

use forge_sidecar::rpc::{dispatch, Response, INVALID_REQUEST};
use forge_sidecar::services::LiveServices;

fn main() {
    let services = LiveServices::from_process();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<forge_sidecar::rpc::Request>(&line) {
            Ok(request) => dispatch(&services, &request),
            Err(error) => Response::err(0, INVALID_REQUEST, error.to_string()),
        };

        if serde_json::to_writer(&mut out, &response).is_err() || out.write_all(b"\n").is_err() {
            break;
        }
        let _ = out.flush();
    }
}
