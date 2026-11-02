// fleet_chaos.rhai — a generative step for the lab.
//
// Contract: fn step(vars_json) -> String, returning lab-JSON:
//   { vars: {...}, verdict: "pass" | "fail", message: "..." }
// The runner feeds the returned `vars` forward into later steps, so a
// generative step can *decide* what the declarative steps test next.

let vendors = ["samsung", "lg", "roku"];

fn step(vars_json) {
    // Deterministic "flaky hour": three blips, one device that never pairs.
    let blips = 3;
    let unpairable = "tv-192.168.1.99";

    // Pick the vendor round-robin from a counter carried in vars.
    let idx = 0;
    if vars_json.contains("chaos_round") {
        // vars arrive as JSON; keep the arithmetic simple and side-effect free
        idx = 1;
    }

    #{
        vars: #{
            chaos_vendor: vendors[idx],
            chaos_blips: blips,
            chaos_unpairable: unpairable,
        },
        verdict: "pass",
        message: `synthesized ${blips} blips + 1 unpairable device; next vendor: ${vendors[idx]}`,
    }.to_json()
}
