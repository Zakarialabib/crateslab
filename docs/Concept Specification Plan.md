# cratelab — Concept Specification Plan

## Framing

cratelab already has a spine worth protecting: **one manifest drives everything** — the UI, the runner, the coverage ledger, the fuzzer. The generated layer proposes the surface from the crate's typed enums; the authored layer disposes the human judgement. Everything below is built *on* that spine, never beside it. The goal of this plan is to move cratelab from a **passive bench** (a human clicks, observes) into an **active conformance engine** (the bench proposes, heals, races, and gates releases by itself).

### Invariants we do not touch
These carry over from the workspaces and must survive every concept below:

- **Refusals are data.** A capability refusal is a first-class outcome (severity info, exit 0), never a red error.
- **Mock-first, hardware behind a toggle.** The same `CommandSink` port serves the mock fleet and a real facade; scenarios never know the difference.
- **Deterministic chaos.** Failure injection is counter-based, no RNG, so a failing run replays identically.
- **Zero panic** on any path a manifest or script can influence.
- **One error vocabulary.** Everything branches on `LabError` + `Remedy`, nothing else.
- **The UI is a renderer, not an author.** Adding a crate adds zero frontend code.

---

## The Ten Concept Specs

Each concept is a **merge** of at least two existing ideas, plus the one new thing it introduces. Fields are consistent so they can be compared and prioritized.

---

### 1. Semantic Contract Evolution
- **Thesis:** When the crate's surface changes, cratelab doesn't just warn about drift — it drafts the fix.
- **Merges:** drift detection + generated/override split + LLM-assisted override authoring.
- **Problem:** Today `manifest_behind` is a banner a human must act on. Overrides are hand-written, so they rot as fast as the prose they replace.
- **Mechanism:** When the generator detects a new variant or changed signature, cratelab produces a *proposed override* — inferred risk level, suggested mock strategy, and a drafted `why` — drawn from the crate's doc comments. A human reviews and commits. The machine proposes; the artifact discipline disposes.
- **Consumes → Produces:** generated-layer diff + crate docs → a reviewable override patch with provenance.
- **Success criterion:** a new `CommandId` variant goes from "drift banner" to "merged, covered override" with a human touching only the approve button.
- **Depends on:** the generator binary, the drift vocabulary, the override `why` convention.
- **Risk:** an auto-drafted `why` that sounds right but is wrong. Mitigation: drafts are advisory, never written to `generated/`, and lint warns on any override lacking a human-edited `why`.

---

### 2. Property-Based Conformance
- **Thesis:** The input schema is already a generator; pair it with invariants and every command gets a free proptest suite, forever.
- **Merges:** the small `input` schema dialect + conformance tiers + type-directed input generation.
- **Problem:** Argument handling is where panics hide, and hand-written tests only cover the values someone happened to think of.
- **Mechanism:** Each command's declared input fields drive type-directed generation (half in-range, half deliberately hostile). Declared invariants — e.g. *idempotent means two calls with the same key give the same result* — become assertions. The zero-panic rule becomes an executable property rather than a README promise.
- **Consumes → Produces:** `input` schema + invariant flags → a standing per-command property suite and its violations.
- **Success criterion:** any command marked `must`-tier has its invariants exercised on every run, and a panic on hostile input fails the ledger rather than crashing.
- **Depends on:** the fuzzing type-strategy, conformance tier metadata.
- **Risk:** invariant noise (flaky "failures" that are really correct refusals). Mitigation: refusals are classified before assertion, so a typed refusal passes the property.

---

### 3. Adversarial Generative UI (Red-Team Mode)
- **Thesis:** The same widget table that renders a happy-path form can render an attacker.
- **Merges:** the generative widget table + type-directed fuzzing + the `FieldSpec` bounds.
- **Problem:** The UI is currently a happy-path surface; fuzzing lives in a separate panel, so the two never meet.
- **Mechanism:** A toggle turns each generated form adversarial: it reads each field's min/max/pattern/enum and emits boundary values, one-past-bound, and hostile strings on invoke. The panel draws a live resilience readout — which inputs produced a graceful refusal (good) versus an internal error (a bug).
- **Consumes → Produces:** rendered `FieldSpec` → adversarial invocations + a per-widget resilience status.
- **Success criterion:** an operator can click through a command's hostile envelope and see, at a glance, that every refusal is typed and no input panics.
- **Depends on:** the generative form, the error inspector, fuzzing's generation rules.
- **Risk:** hostile input against armed hardware. Mitigation: Red-Team Mode is mock/replay-only; the hardware guard simply refuses it.

---

### 4. Mock Fidelity Shadow Engine
- **Thesis:** The mock fleet should be measured against reality, and the gap should close itself.
- **Merges:** golden cassettes + replay adapter + the coverage ledger + mock strategies.
- **Problem:** Mocks are hand-authored and silently drift from real hardware. A mock that always succeeds hides exactly the error paths the lab exists to exercise.
- **Mechanism:** Run a recorded hardware cassette and the current mock in parallel; compare outcomes per command. Where they disagree — mock says success, hardware says `PtzDisabled` — cratelab flags a **fidelity gap** and drafts a mock patch (a new when/then rule) that brings the mock in line with the recording.
- **Consumes → Produces:** a cassette + the live mock → a fidelity report + proposed mock override.
- **Success criterion:** a measurable "mock fidelity" score per workspace, and gaps convertible to patches with one review.
- **Depends on:** record/replay cassettes, the override merge, the ledger.
- **Risk:** over-fitting the mock to one specific device's recording. Mitigation: patches are per-device-class, and the gap report names the single device it came from.

---

### 5. Golden Cassette Regression
- **Thesis:** Real-hardware behavior becomes a checked-in artifact that fails loudly on change.
- **Merges:** the recording adapter + replay adapter + CI runner.
- **Problem:** "It worked on the TV in the office" is not reproducible, and hardware-verified results live in a blank markdown table today.
- **Mechanism:** Record a hardware session once into a cassette. Every CI run replays it — no hardware, no network, byte-identical. A behavior change in the crate under test shows up as a replay mismatch, and the previously-empty hardware-results table fills itself from the cassette.
- **Consumes → Produces:** one recorded session → a durable regression artifact + a populated hardware-verified column.
- **Success criterion:** a crate change that alters a recorded interaction fails CI on replay, and capability rows gain real `hw_verified` entries without transcription.
- **Depends on:** the cassette format, manifest version pinning (replay must refuse a changed manifest).
- **Risk:** cassettes going stale relative to the manifest. Mitigation: the cassette stores manifest versions and refuses to replay across a mismatch.

---

### 6. Quirk Round-Trip Discovery
- **Thesis:** The lab can find the quirks the workspaces currently hand-maintain.
- **Merges:** the quirks vocabulary + fuzz campaigns + the coverage ledger.
- **Problem:** Quirk tables (a command failing only on a specific model/firmware pair) are written by hand after someone gets burned.
- **Mechanism:** During fuzz and soak runs, cratelab watches for a command that fails only under a specific `(device, vendor, firmware)` combination. That signature becomes a candidate quirk row, emitted as a ready-to-paste override. The round-trip is complete: quirks inform the mock, and the mock's failures propose new quirks.
- **Consumes → Produces:** fuzz/soak outcomes keyed by device identity → candidate quirk rows.
- **Success criterion:** a reproducible, device-specific failure surfaces as a proposed quirk with its reproducing seed attached.
- **Depends on:** deterministic chaos, device fixtures carrying model/firmware metadata.
- **Risk:** mistaking a transient for a quirk. Mitigation: a candidate must reproduce across replays before it's proposed.

---

### 7. Remedy Autopilot (Self-Healing Playlists)
- **Thesis:** Errors already carry a routing table; the runner should be able to follow it.
- **Merges:** the `Remedy` vocabulary + the playlist runner + the error taxonomy.
- **Problem:** Today a `Remedy` is displayed to a human. The lab proves the crate fails correctly but never proves the *system* recovers correctly.
- **Mechanism:** When a step fails, the runner intercepts the `Remedy`. If a healing playbook is declared for that remedy, the runner executes it (re-auth, back-off, re-provision) and retries the step. The coverage ledger gains a new cell state — **healed** — alongside pass/refused/failed.
- **Consumes → Produces:** a failed step + declared remedy playbooks → a healed or escalated outcome.
- **Success criterion:** a playlist seeded with deliberate `AuthFailed` and `Offline` faults passes because the runner heals them, and coverage can report healing as a distinct, countable state.
- **Depends on:** the remedy vocabulary being data, the playlist runner, healing playbooks as a new manifest-declared block.
- **Risk:** an autopilot that retries a genuinely destructive step. Mitigation: healing never re-arms a destructive command; it can heal `safe`/`mutating` only, and escalation stops at the hardware guard.

---

### 8. Cross-Workspace Confluence Engine
- **Thesis:** The interesting bugs live between workspaces, and no single-crate suite can express them.
- **Merges:** confluence scenarios + the workspace-agnostic event bus + the digital twin + deterministic chaos.
- **Problem:** A sensor event that ducks audio, which must *not* wake a panel a camera already quarantined — that invariant spans three manifests and dies in any per-crate test.
- **Mechanism:** Because the bus is workspace-agnostic, confluence is a playlist, not an architecture change. The engine extends this into **event-timing fuzzing**: it slides the millisecond offset between cross-workspace events under a seeded generator, hunting the race where a reconciler misreads a delayed quarantine as a panel fault. A recorded-cassette-driven mock fleet becomes a standing, scrubbable digital twin of the whole building.
- **Consumes → Produces:** multiple manifests + seeded event offsets → cross-workspace timelines and the exact offset that breaks an invariant.
- **Success criterion:** a reproducible cross-crate race is found and replayed from a printed seed, and a named invariant (e.g. the one-action-per-hour rule) holds across a simulated 24-hour multi-device run.
- **Depends on:** the shared bus, deterministic clock/chaos, at least two loaded manifests.
- **Risk:** combinatorial blow-up of event orderings. Mitigation: seeded, shrinkable campaigns that report a minimal reproducing offset.

---

### 9. Federated Manifest Registry
- **Thesis:** cratelab stops being a bench for three workspaces and becomes a bench for any workspace.
- **Merges:** the manifest registry idea + sparse-index discovery + the WASM adapter seam.
- **Problem:** Adoption today means wiring a new workspace into the lab by hand.
- **Mechanism:** Manifests publish to a small index so any Rust crate can be adopted by dropping a manifest in; the lab becomes generic and the current three workspaces are its first tenants. A WASM adapter seam lets a vendor ship a simulator without cratelab depending on their crate — the seam stays clean, the dependency graph stays acyclic.
- **Consumes → Produces:** a published manifest (+ optional WASM simulator) → a newly testable workspace with generated UI and coverage.
- **Success criterion:** a crate cratelab has never seen becomes invocable, coverable, and fuzzable with no lab source changes.
- **Depends on:** the manifest schema being stable, discovery, the adapter trait.
- **Risk:** third-party manifests that don't hold the lab's invariants. Mitigation: registry manifests pass the same validation and refusal discipline as first-party ones.

---

### 10. Provisioning Bundle Validator
- **Thesis:** The installer's worst moment — a bundle that decrypts but the backend refuses — becomes a panel.
- **Merges:** the provisioning bundle format + encrypted auth envelopes + the error taxonomy.
- **Problem:** Both workspaces ship provisioning bundles and encrypted auth envelopes, and a mismatch between what a bundle claims and what a backend accepts is discovered in the field.
- **Mechanism:** A dedicated surface loads a bundle, decrypts in memory, and reports the gap: "this bundle claims a capability the target backend refuses." Secrets stay redacted end-to-end, matching the zeroized-secrets discipline of the crates.
- **Consumes → Produces:** a provisioning bundle + a target backend's capability set → a claim-versus-reality report.
- **Success criterion:** a bundle/backend mismatch is reported as a typed, remediable error before deployment, never as a field failure.
- **Depends on:** capability sets, the error vocabulary, secret redaction.
- **Risk:** handling real credentials in a bench. Mitigation: decrypt-in-memory only, redaction enforced, and the panel is mock-backed until explicitly armed.

---

## Sequencing — how they compose into a plan

The concepts are ordered by how much of the existing spine they reuse, cheapest first. They converge on a single **integration capstone**: a **Continuous Conformance Release Gate**, where drift + coverage + conformance + cassette-replay all feed one go/no-go decision on publish.

| Phase | Concepts | Why this order | Exit signal |
|---|---|---|---|
| **A — Extend the contract** | 1 Semantic Evolution · 2 Property Conformance · 3 Red-Team UI | Build directly on manifest + fuzz + UI already present; low risk, immediate leverage | drift becomes a reviewable patch; every `must` command has live properties |
| **B — Close the fidelity gap** | 5 Golden Cassette · 4 Shadow Engine · 6 Quirk Round-Trip | Cassette must exist before the shadow engine can measure against it | a mock-fidelity score per workspace; hardware results auto-populated |
| **C — Resilience & emergence** | 7 Remedy Autopilot · 8 Confluence Engine | Depend on the remedy vocabulary and the shared bus being data | a seeded cross-workspace race reproduced; healing counted in coverage |
| **D — Ecosystem & gate** | 9 Federated Registry · 10 Provisioning Validator · Release Gate capstone | Depend on a stable schema and everything upstream feeding CI | an unknown crate adopted with zero lab changes; publish gated on conformance |

**Dependency notes worth flagging:**
- Concepts 4 and 5 both rest on record/replay; build the cassette format once, correctly, before either.
- Concepts 2, 3, and 6 all consume the same type-directed generation — it's one engine reused three ways.
- Concept 7 introduces the only genuinely new manifest block (healing playbooks); keep it data, keep it behind the same refusal discipline.
- Concept 8 is the highest-leverage and the hardest to bound; give it seeded, shrinkable campaigns from day one.

---

## Open questions to decide before committing

1. **How much autonomy do we trust?** Concepts 1, 4, and 7 all propose or act automatically. The line is: *propose freely, act only on `safe`/`mutating`, never on destructive, and never past the hardware guard.*
2. **Is the ledger the single source of truth for all new states?** Healing, fidelity gaps, and quirk candidates all want to be first-class ledger citizens. Say yes and they stay diffable; say no and they fragment.
3. **Where does the LLM live?** Concept 1 needs a model. The safe answer: advisory only, never writes `generated/`, every output carries a human-edited `why`.
4. **Which workspace proves the confluence story first?** The sensor→audio→panel chain is the natural demo; picking it now shapes the fixtures we build.
5. **Do we gate releases from day one, or report-only first?** The capstone can start as a PR comment (report) and graduate to a hard block (gate) once the signals are trusted.

---

The through-line of the whole plan: **cratelab's manifest is not a map of the API — it is a living behavioral contract.** Every concept above makes that contract self-maintaining (1, 4), self-proving (2, 3, 6), self-healing (7), cross-cutting (8), and open to the ecosystem (9, 10) — while the refusal-as-data, mock-first, deterministic spine stays exactly as it is.