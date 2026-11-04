<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { ForgeSettingField, ForgeSettingsSnapshot, ForgeStatus } from "../types";

  let snapshot = $state<ForgeSettingsSnapshot | null>(null);
  let forgeStatus = $state<ForgeStatus | null>(null);
  let drafts = $state<Record<string, string>>({});
  let busy = $state(false);
  let action = $state("");
  let confirmExternal = $state<"upload" | "deploy" | null>(null);
  let message = $state("");
  let error = $state("");
  let probeKey = $state<string | null>(null);
  let probeBusy = $state(false);
  let probeMessage = $state("");

  // Which rung a credential field belongs to — a Probe is always a cheap GET
  // against that rung, never a spend endpoint.
  const credentialProbe: Record<string, string> = {
    GEMINI_API_KEY: "gemini",
    GROQ_API_KEY: "groq",
    CF_ACCOUNT_ID: "cloudflare",
    CF_API_TOKEN: "cloudflare",
    NVIDIA_API_KEY: "nvidia",
    OPENROUTER_API_KEY: "openrouter",
    ZHIPU_API_KEY: "zhipu",
    MODELSCOPE_TOKEN: "modelscope",
    BONSAI_URL: "colab-bonsai",
    KAGGLE_BONSAI_URL: "kaggle-bonsai",
    HF_TOKEN: "huggingface",
    HF_READ_TOKEN: "huggingface",
    HF_TOKEN_WRITE: "huggingface",
    MODAL_SEARCH_URL: "modal",
    MODAL_SEARCH_TOKEN: "modal",
    MODAL_TOKEN_ID: "modal",
    MODAL_TOKEN_SECRET: "modal",
    QDRANT_URL: "qdrant",
    QDRANT_API_KEY: "qdrant",
    KAGGLE_API_KEY: "kaggle",
    CIRCLECI_TOKEN: "circleci",
    GH_TOKEN: "github-read",
  };

  const groups = [
    { title: "Inference providers", keys: ["GEMINI_API_KEY", "GROQ_API_KEY", "CF_ACCOUNT_ID", "CF_API_TOKEN", "NVIDIA_API_KEY", "OPENROUTER_API_KEY", "ZHIPU_API_KEY", "MODELSCOPE_TOKEN", "BONSAI_URL", "KAGGLE_BONSAI_URL"] },
    { title: "Knowledge and search", keys: ["HF_TOKEN", "HF_TOKEN_WRITE", "HF_READ_TOKEN", "HF_DATASET_REPO", "HF_DATASET_REVISION", "HF_CRATES_DATASET_REPO", "HF_CRATES_DATASET_REVISION", "MODAL_SEARCH_URL", "MODAL_SEARCH_TOKEN", "CRATE_INTEL_MODAL_SEARCH_URL", "CRATE_INTEL_MODAL_SEARCH_TOKEN", "MODAL_TOKEN_ID", "MODAL_TOKEN_SECRET", "QDRANT_URL", "QDRANT_API_KEY"] },
    { title: "Automation and source", keys: ["KAGGLE_API_KEY", "KAGGLE_KERNEL_ID", "CIRCLECI_TOKEN", "CIRCLECI_PROJECT_SLUGS", "GH_TOKEN"] },
    { title: "Local repositories and knowledge sources", keys: ["TV_RS_ROOT", "CCTV_RS_ROOT", "SIMPLE_SIGNAGE_ROOT", "COMPANION_APP_ROOT"] },
  ];

  onMount(() => { void load(); });

  async function load() {
    error = "";
    try {
      snapshot = await api.forgeGetSettings();
      drafts = Object.fromEntries(snapshot.fields.filter((field) => !field.secret).map((field) => [field.key, field.value ?? ""]));
    } catch (e) { error = String(e); }
    try { forgeStatus = await api.forgeStatus(); } catch { /* readiness chip stays empty */ }
  }

  function field(key: string): ForgeSettingField | undefined { return snapshot?.fields.find((item) => item.key === key); }
  function label(key: string): string { return key.replaceAll("_", " ").toLowerCase().replace(/\b\w/g, (c) => c.toUpperCase()); }
  // A secret is only ever rendered as dots — never as its value, never echoed.
  function masked(key: string): string { return field(key)?.configured ? "••••••••" : "not set"; }
  function setupUrl(key: string): string | null {
    return field(key)?.setup_url ?? forgeStatus?.providers.find((p) => p.id === credentialProbe[key])?.setup_url ?? null;
  }
  function probeState(key: string): string {
    const provider = forgeStatus?.providers.find((p) => p.id === credentialProbe[key]);
    if (!provider) return "—";
    if (provider.probe_ok === true) return "Probed ✓";
    if (provider.probe_ok === false) return "Probe failed";
    return provider.credential_ready ? "Configured · unprobed" : "Missing";
  }

  const setCount = $derived(snapshot?.fields.filter((item) => item.configured).length ?? 0);

  async function confirmProbe() {
    const key = probeKey;
    if (!key) return;
    const providerId = credentialProbe[key];
    if (!providerId) return;
    probeBusy = true;
    probeMessage = "";
    try {
      const result = await api.forgeProbeProvider(providerId);
      probeMessage = `${result.provider}: ${result.summary} (HTTP ${result.status})`;
      forgeStatus = await api.forgeStatus();
      if (result.ok) probeKey = null;
    } catch (e) {
      probeMessage = String(e);
    } finally {
      probeBusy = false;
    }
  }

  async function save() {
    if (!snapshot) return;
    busy = true; message = ""; error = "";
    try {
      const updates: Record<string, string> = { ...drafts };
      for (const item of snapshot.fields) if (item.secret && drafts[item.key]) updates[item.key] = drafts[item.key];
      snapshot = await api.forgeSaveSettings(updates);
      drafts = Object.fromEntries(snapshot.fields.filter((item) => !item.secret).map((item) => [item.key, item.value ?? ""]));
      for (const item of snapshot.fields) if (item.secret) drafts[item.key] = "";
      message = "Saved locally. Secret values were not returned to the UI.";
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }

  async function runTool(name: string, title: string) {
    busy = true; action = title; message = ""; error = "";
    try {
      const result = await api.forgeRunTool(name);
      const combined = [result.output, result.error].filter(Boolean).join("\n").trim();
      if (!result.ok) throw new Error(combined || `${title} failed`);
      message = combined || `${title} completed.`;
      if (["build-signage-index", "upload-signage-index", "deploy-modal-search"].includes(name)) await load();
    } catch (e) { error = String(e); }
    finally { busy = false; action = ""; }
  }

  async function runConfirmedExternal() {
    const step = confirmExternal;
    confirmExternal = null;
    if (!step) return;
    await runTool(step === "upload" ? "upload-signage-index" : "deploy-modal-search", step === "upload" ? "Uploading HF dataset" : "Deploying Modal search");
  }
</script>

<section class="forge-settings-page">
  <header class="forge-header">
    <div><div class="eyebrow"><span class="eyebrow-dot"></span> FREE-TIER RUNGS · LOCAL CREDENTIALS</div><h1>Providers &amp; credentials <span class="safe-pill">SECRETS MASKED</span></h1><p class="forge-lede">Which free AI is usable right now. Every field carries its rung state, a How-to-get link, and a Probe — a cheap GET, never a spend endpoint. Values are saved only to the ignored local environment file.</p></div>
    <button class="primary" onclick={save} disabled={busy}>{busy ? "Saving…" : "Save settings"}</button>
  </header>

  {#if snapshot}
    <div class="settings-location">
      <strong>Local settings file</strong><code>{snapshot.env_path}</code>
      <span class="env-status">{setCount}/{snapshot.fields.length} set · status only, values never rendered</span>
      <span>Not committed</span>
    </div>
    {#each groups as group (group.title)}
      <section class="panel settings-group"><div class="panel-heading"><div><span class="section-kicker">CONFIGURATION</span><h2>{group.title}</h2></div><span class="flow-caption">Masked always · status only · probe is a cheap GET</span></div><div class="settings-grid">
        {#each group.keys as key (key)}
          {@const item = field(key)}
          {#if item}
            <div class="settings-field"><span>{label(key)} <i class:configured={item.configured}>{item.configured ? (item.secret ? "Secret saved" : "Configured") : "Missing"}</i></span>
              {#if item.secret}
                <div class="secret-row">
                  <span class="secret-mask">{masked(key)}</span>
                  <input type="password" autocomplete="new-password" placeholder={item.configured ? "Saved · blank keeps current value" : "Enter locally; never shown again"} bind:value={drafts[key]} />
                </div>
              {:else}
                <input type="text" spellcheck="false" placeholder="Not set" bind:value={drafts[key]} />
              {/if}
              <div class="settings-field-actions">
                <span class="rung-state" class:rung-probed={probeState(key) === "Probed ✓"}>{probeState(key)}</span>
                {#if credentialProbe[key] && item.configured}
                  <button class="probe-button" onclick={() => (probeKey = key)} disabled={busy || probeBusy}>Probe</button>
                {/if}
                {#if setupUrl(key)}
                  <a class="settings-help-link" href={setupUrl(key)} target="_blank" rel="noreferrer">How to get {label(key)} ↗</a>
                {:else if !item.configured && ["HF_DATASET_REVISION", "MODAL_SEARCH_TOKEN", "HF_CRATES_DATASET_REVISION", "CRATE_INTEL_MODAL_SEARCH_URL", "CRATE_INTEL_MODAL_SEARCH_TOKEN"].includes(key)}
                  <small class="settings-help-note">Set automatically by the {key.includes("CRATE_INTEL") || key === "HF_CRATES_DATASET_REVISION" ? "crate intelligence Modal/HF" : "signage HF/Modal"} workflow on the Ops dashboard.</small>
                {/if}
              </div>
            </div>
          {/if}
        {/each}
      </div></section>
    {/each}

    <section class="panel settings-group"><div class="panel-heading"><div><span class="section-kicker">LOCAL ACTIONS</span><h2>Corpus and service setup</h2><p>Corpus validation and orchestration run in Rust. HF and Modal network actions require separate confirmation.</p></div></div>
      <div class="settings-actions">
        <article><strong>Build signage docs index</strong><p>Read the docs folders in Simple-Signage and companion-app and refresh the local ignored corpus.</p><button class="ghost" onclick={() => runTool("build-signage-index", "Building index")} disabled={busy}>{action === "Building index" ? "Building…" : "Build index"}</button></article>
        <article><strong>Validate corpus locally</strong><p>Check provenance, excluded paths, and secret-like strings. No HF request is made.</p><button class="ghost" onclick={() => runTool("validate-signage-index", "Validating index")} disabled={busy}>{action === "Validating index" ? "Validating…" : "Validate index"}</button></article>
        <article><strong>Modal CLI</strong><p>{snapshot.modal_cli_installed ? "Installed" : "Not detected"} · {snapshot.modal_profile_configured ? "Local profile verified" : "No local profile"}. Modal login profile is stored outside the source repository.</p><a class="ghost settings-link" href="https://modal.com/docs/cli/latest/token" target="_blank" rel="noreferrer">Token setup guide</a></article>
        <article><strong>Hugging Face tokens</strong><p>Read/check: {field("HF_READ_TOKEN")?.configured ? "dedicated token present" : field("HF_TOKEN")?.configured ? "HF_TOKEN present" : "missing"} · Upload: {field("HF_TOKEN_WRITE")?.configured ? "write token present" : field("HF_TOKEN")?.configured ? "HF_TOKEN present" : "missing"}. Values never appear here.</p><a class="ghost settings-link" href="https://huggingface.co/settings/tokens" target="_blank" rel="noreferrer">Manage tokens</a></article>
        <article><strong>Validate configured HF token</strong><p>Make a read-only HF identity and permissions check using the token saved in the local environment file.</p><button class="ghost" onclick={() => runTool("check-hf-token", "Checking Hugging Face token")} disabled={busy}>{action === "Checking Hugging Face token" ? "Checking…" : "Check HF access"}</button></article>
        <article><strong>Hugging Face CLI</strong><p>{snapshot.hf_cli_installed ? "Installed · upload is orchestrated by the Rust desktop backend." : "Missing · install the official CLI to enable dataset uploads."}</p><a class="ghost settings-link" href="https://huggingface.co/docs/huggingface_hub/guides/cli" target="_blank" rel="noreferrer">CLI setup guide</a></article>
        <article><strong>Crate intelligence dataset</strong><p>Private internal source index: {field("HF_CRATES_DATASET_REPO")?.value || "set the repository name above"}. Local source scan and prompt/context pack run from the Ops dashboard.</p><a class="ghost settings-link" href="https://huggingface.co/new-dataset" target="_blank" rel="noreferrer">Create private HF dataset</a></article>
        <article><strong>Kaggle evaluation job</strong><p>{snapshot.kaggle_cli_installed ? "Kaggle CLI installed in CrateLab's isolated local environment." : "Kaggle CLI missing."} · Kernel: {field("KAGGLE_KERNEL_ID")?.value || "owner/kernel-slug needed"}. Job is private, CPU-first, and requires HF secrets configured in Kaggle.</p><a class="ghost settings-link" href="https://www.kaggle.com/settings/api" target="_blank" rel="noreferrer">Kaggle API settings</a> <a class="ghost settings-link" href="https://github.com/Kaggle/kaggle-cli/blob/main/docs/kernels.md" target="_blank" rel="noreferrer">Kernel job CLI guide</a></article>
        <article><strong>CircleCI research/docs gates</strong><p>Forge reads account, project, pipeline, and workflow status. The project slug list is comma-separated, for example <code>gh/Zakarialabib/tv-rs</code>. Connect the repository in CircleCI and add the reviewed gate config to its <code>.circleci/config.yml</code>.</p><a class="ghost settings-link" href="https://app.circleci.com/projects" target="_blank" rel="noreferrer">Connect CircleCI project</a> <a class="ghost settings-link" href="https://circleci.com/docs/api/v2/" target="_blank" rel="noreferrer">API reference</a></article>
        <article><strong>Upload private signage dataset</strong><p>Create or update {field("HF_DATASET_REPO")?.value || "a dataset repo set above"} with the local docs corpus. Upload includes current uncommitted docs from both source projects.</p><button class="ghost" onclick={() => (confirmExternal = "upload")} disabled={busy || !snapshot.hf_cli_installed || !(field("HF_TOKEN_WRITE")?.configured || field("HF_TOKEN")?.configured) || !field("HF_DATASET_REPO")?.value}>Review upload</button></article>
        <article><strong>Deploy Modal search endpoint</strong><p>Create or replace the two Modal secrets and deploy the authenticated read-only search service using the pinned HF revision.</p><button class="ghost" onclick={() => (confirmExternal = "deploy")} disabled={busy || !(field("HF_READ_TOKEN")?.configured || field("HF_TOKEN")?.configured) || !field("HF_DATASET_REPO")?.value || !field("HF_DATASET_REVISION")?.value || !snapshot.modal_cli_installed || !snapshot.modal_profile_configured}>Review deployment</button></article>
      </div>
    </section>
  {/if}

  {#if message}<div class="settings-notice" role="status">{message}</div>{/if}
  {#if error}<div class="forge-alert" role="alert">{error}</div>{/if}
  {#if confirmExternal}
    <div class="modal-backdrop" role="presentation">
      <dialog class="confirm-modal" open aria-labelledby="external-action-title">
        <span class="modal-kicker">EXTERNAL SERVICE ACTION</span>
        {#if confirmExternal === "upload"}
          <h2 id="external-action-title">Upload signage docs to Hugging Face?</h2>
          <p>This creates or updates the private dataset <strong>{field("HF_DATASET_REPO")?.value}</strong> using your configured HF token.</p>
          <div class="modal-detail"><strong>Data sent</strong><span>The full local signage docs index (about 4.2 MB): indexed docs from Simple-Signage and companion-app, including their current uncommitted documentation edits. The builder filtered secret-like content but human review is still recommended.</span></div>
          <div class="modal-detail"><strong>Access</strong><span>The HF dataset is set private. HF receives and stores the corpus as a new dataset revision. Upload uses HF_TOKEN_WRITE when set, otherwise HF_TOKEN; the selected token needs dataset repo.write permission.</span></div>
        {:else}
          <h2 id="external-action-title">Deploy Modal search?</h2>
          <p>This creates or replaces Modal secrets and deploys the authenticated search service for <strong>{field("HF_DATASET_REPO")?.value}</strong> at revision <strong>{field("HF_DATASET_REVISION")?.value}</strong>.</p>
          <div class="modal-detail"><strong>Credentials sent to Modal</strong><span>{field("HF_READ_TOKEN")?.configured ? "The dedicated HF_READ_TOKEN" : "The configured HF_TOKEN"}, dataset repo and pinned revision are stored in the Modal secret so the service can download the private corpus. {field("HF_READ_TOKEN")?.configured ? "This uses the dedicated read token." : "Consider adding a least-privilege read-only HF_READ_TOKEN in Settings."} A generated search bearer token is also stored in a Modal secret and locally in forge/.env.</span></div>
          <div class="modal-detail"><strong>Service behavior</strong><span>Deploy publishes an HTTPS endpoint that anyone can reach, but search requests require the bearer token. Modal compute may use account quota or incur charges depending on your plan and usage.</span></div>
        {/if}
        <div class="modal-actions"><button class="ghost" onclick={() => (confirmExternal = null)} disabled={busy}>Cancel</button><button class="primary" onclick={runConfirmedExternal} disabled={busy}>{confirmExternal === "upload" ? "Upload corpus" : "Create secrets and deploy"}</button></div>
      </dialog>
    </div>
  {/if}
  {#if probeKey}
    <div class="modal-backdrop" role="presentation">
      <dialog class="confirm-modal" open aria-labelledby="credential-probe-title">
        <span class="modal-kicker">READ-ONLY CREDENTIAL CHECK</span>
        <h2 id="credential-probe-title">Probe {label(probeKey)}?</h2>
        <p>The saved credential is sent in one cheap GET to this rung's identity or health endpoint. No crate source, prompt, or spend endpoint is involved.</p>
        <div class="modal-detail"><strong>Rung</strong><span>{credentialProbe[probeKey]}</span></div>
        <div class="modal-detail"><strong>Credential</strong><span>{probeKey} · value stays masked, never rendered</span></div>
        <div class="modal-detail"><strong>Stored locally</strong><span>Success/failure, timestamp, and served model IDs only. The response body is discarded.</span></div>
        {#if probeMessage}<div class="modal-message" role="status">{probeMessage}</div>{/if}
        <div class="modal-actions"><button class="ghost" onclick={() => (probeKey = null)} disabled={probeBusy}>Cancel</button><button class="primary" onclick={confirmProbe} disabled={probeBusy}>{probeBusy ? "Probing…" : "Send credential and probe"}</button></div>
      </dialog>
    </div>
  {/if}
  <footer class="forge-footer"><span>Secrets stay local in forge/.env</span><span>Probe is a cheap GET — never a spend endpoint. Free tiers only: spend stays €0.00.</span></footer>
</section>
