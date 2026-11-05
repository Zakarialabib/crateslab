<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../api";
  import type { ForgeSettingField, ForgeSettingsSnapshot, ForgeStatus } from "../types";
  import { Button } from "$lib/components/ui/button";
  import { Badge } from "$lib/components/ui/badge";
  import * as Card from "$lib/components/ui/card";
  import { Input } from "$lib/components/ui/input";
  import { Label } from "$lib/components/ui/label";
  import * as Dialog from "$lib/components/ui/dialog";
  import { cn } from "$lib/utils";

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

<section class="mx-auto max-w-[1240px]">
  <header class="mb-5 flex flex-col items-start gap-5 min-[760px]:flex-row min-[760px]:items-end min-[760px]:justify-between">
    <div><div class="mb-[7px] flex items-center gap-[7px] text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground"><span class="size-[7px] rounded-full bg-success shadow-[0_0_0_3px_#34a87822]"></span> FREE-TIER RUNGS · LOCAL CREDENTIALS</div><h1 class="m-0 mb-1 flex items-center gap-3 text-[25px] font-bold tracking-[-0.4px]">Providers &amp; credentials <Badge variant="secondary" class="border-success px-[9px] py-1 text-[9px] font-extrabold tracking-[0.8px] text-success">SECRETS MASKED</Badge></h1><p class="mt-[7px] text-[13px] text-muted-foreground">Which free AI is usable right now. Every field carries its rung state, a How-to-get link, and a Probe — a cheap GET, never a spend endpoint. Values are saved only to the ignored local environment file.</p></div>
    <Button variant="default" class="h-9 px-[18px] text-sm" onclick={save} disabled={busy}>{busy ? "Saving…" : "Save settings"}</Button>
  </header>

  {#if snapshot}
    <div class="mb-3.5 flex flex-col items-start gap-3 rounded-md border border-border bg-card px-[15px] py-3 text-[11px] text-muted-foreground min-[760px]:flex-row min-[760px]:items-center">
      <strong class="font-bold text-foreground">Local settings file</strong><code class="text-muted-foreground">{snapshot.env_path}</code>
      <span class="text-[10px] font-bold text-muted-foreground min-[760px]:ml-auto">{setCount}/{snapshot.fields.length} set · status only, values never rendered</span>
      <span class="text-[10px] font-bold text-success min-[760px]:ml-auto">Not committed</span>
    </div>
    {#each groups as group (group.title)}
      <Card.Root class="mb-3.5"><Card.Header class="flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">CONFIGURATION</span><h2 class="mt-0.5 mb-0 text-[15px] font-bold text-foreground">{group.title}</h2></div><span class="text-[10.5px] text-muted-foreground">Masked always · status only · probe is a cheap GET</span></Card.Header><Card.Content class="grid grid-cols-1 gap-3 min-[760px]:grid-cols-2">
        {#each group.keys as key (key)}
          {@const item = field(key)}
          {#if item}
            <div class="grid min-w-0 gap-1.5 text-[11px] font-bold text-muted-foreground"><Label class="flex items-center justify-between gap-2">{label(key)} <i class={cn("text-[9px] font-semibold not-italic", item.configured ? "text-success" : "text-warning")}>{item.configured ? (item.secret ? "Secret saved" : "Configured") : "Missing"}</i></Label>
              {#if item.secret}
                <div class="flex items-center gap-2">
                  <span class="whitespace-nowrap rounded-sm border border-dashed border-border bg-muted px-2 py-[3px] font-mono text-[10.5px] tracking-[2px] text-muted-foreground">{masked(key)}</span>
                  <Input type="password" autocomplete="new-password" placeholder={item.configured ? "Saved · blank keeps current value" : "Enter locally; never shown again"} bind:value={drafts[key]} class="h-auto flex-1 rounded-sm bg-card px-2.5 py-[9px] font-mono text-[11px]" />
                </div>
              {:else}
                <Input type="text" spellcheck="false" placeholder="Not set" bind:value={drafts[key]} class="h-auto rounded-sm bg-card px-2.5 py-[9px] font-mono text-[11px]" />
              {/if}
              <div class="flex flex-wrap items-center gap-2">
                <span class={cn("rounded-full border px-2 py-0.5 text-[9.5px] font-bold", probeState(key) === "Probed ✓" ? "border-success bg-success/10 text-success" : "border-border bg-card text-muted-foreground")}>{probeState(key)}</span>
                {#if credentialProbe[key] && item.configured}
                  <Button variant="outline" size="xs" class="min-w-[47px] text-[8.5px]" onclick={() => (probeKey = key)} disabled={busy || probeBusy}>Probe</Button>
                {/if}
                {#if setupUrl(key)}
                  <a class="text-[10px] font-semibold text-primary no-underline hover:underline" href={setupUrl(key)} target="_blank" rel="noreferrer">How to get {label(key)} ↗</a>
                {:else if !item.configured && ["HF_DATASET_REVISION", "MODAL_SEARCH_TOKEN", "HF_CRATES_DATASET_REVISION", "CRATE_INTEL_MODAL_SEARCH_URL", "CRATE_INTEL_MODAL_SEARCH_TOKEN"].includes(key)}
                  <small class="text-[10px] font-medium text-muted-foreground">Set automatically by the {key.includes("CRATE_INTEL") || key === "HF_CRATES_DATASET_REVISION" ? "crate intelligence Modal/HF" : "signage HF/Modal"} workflow on the Ops dashboard.</small>
                {/if}
              </div>
            </div>
          {/if}
        {/each}
      </Card.Content></Card.Root>
    {/each}

    <Card.Root class="mb-3.5"><Card.Header class="flex items-center justify-between gap-3"><div><span class="text-[10px] font-extrabold tracking-[1.45px] text-muted-foreground">LOCAL ACTIONS</span><h2 class="mt-0.5 mb-0 text-[15px] font-bold text-foreground">Corpus and service setup</h2><p class="mt-1 mb-0 text-[11px] text-muted-foreground">Corpus validation and orchestration run in Rust. HF and Modal network actions require separate confirmation.</p></div></Card.Header>
      <Card.Content class="grid grid-cols-1 gap-2.5 min-[760px]:grid-cols-2">
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">Build signage docs index</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">Read the docs folders in Simple-Signage and companion-app and refresh the local ignored corpus.</p><Button variant="outline" size="sm" onclick={() => runTool("build-signage-index", "Building index")} disabled={busy}>{action === "Building index" ? "Building…" : "Build index"}</Button></Card.Root>
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">Validate corpus locally</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">Check provenance, excluded paths, and secret-like strings. No HF request is made.</p><Button variant="outline" size="sm" onclick={() => runTool("validate-signage-index", "Validating index")} disabled={busy}>{action === "Validating index" ? "Validating…" : "Validate index"}</Button></Card.Root>
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">Modal CLI</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">{snapshot.modal_cli_installed ? "Installed" : "Not detected"} · {snapshot.modal_profile_configured ? "Local profile verified" : "No local profile"}. Modal login profile is stored outside the source repository.</p><a class="inline-flex items-center rounded-md border border-border bg-background px-3.5 py-2 text-xs font-medium text-primary no-underline transition-colors hover:bg-muted" href="https://modal.com/docs/cli/latest/token" target="_blank" rel="noreferrer">Token setup guide</a></Card.Root>
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">Hugging Face tokens</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">Read/check: {field("HF_READ_TOKEN")?.configured ? "dedicated token present" : field("HF_TOKEN")?.configured ? "HF_TOKEN present" : "missing"} · Upload: {field("HF_TOKEN_WRITE")?.configured ? "write token present" : field("HF_TOKEN")?.configured ? "HF_TOKEN present" : "missing"}. Values never appear here.</p><a class="inline-flex items-center rounded-md border border-border bg-background px-3.5 py-2 text-xs font-medium text-primary no-underline transition-colors hover:bg-muted" href="https://huggingface.co/settings/tokens" target="_blank" rel="noreferrer">Manage tokens</a></Card.Root>
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">Validate configured HF token</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">Make a read-only HF identity and permissions check using the token saved in the local environment file.</p><Button variant="outline" size="sm" onclick={() => runTool("check-hf-token", "Checking Hugging Face token")} disabled={busy}>{action === "Checking Hugging Face token" ? "Checking…" : "Check HF access"}</Button></Card.Root>
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">Hugging Face CLI</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">{snapshot.hf_cli_installed ? "Installed · upload is orchestrated by the Rust desktop backend." : "Missing · install the official CLI to enable dataset uploads."}</p><a class="inline-flex items-center rounded-md border border-border bg-background px-3.5 py-2 text-xs font-medium text-primary no-underline transition-colors hover:bg-muted" href="https://huggingface.co/docs/huggingface_hub/guides/cli" target="_blank" rel="noreferrer">CLI setup guide</a></Card.Root>
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">Crate intelligence dataset</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">Private internal source index: {field("HF_CRATES_DATASET_REPO")?.value || "set the repository name above"}. Local source scan and prompt/context pack run from the Ops dashboard.</p><a class="inline-flex items-center rounded-md border border-border bg-background px-3.5 py-2 text-xs font-medium text-primary no-underline transition-colors hover:bg-muted" href="https://huggingface.co/new-dataset" target="_blank" rel="noreferrer">Create private HF dataset</a></Card.Root>
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">Kaggle evaluation job</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">{snapshot.kaggle_cli_installed ? "Kaggle CLI installed in CrateLab's isolated local environment." : "Kaggle CLI missing."} · Kernel: {field("KAGGLE_KERNEL_ID")?.value || "owner/kernel-slug needed"}. Job is private, CPU-first, and requires HF secrets configured in Kaggle.</p><a class="inline-flex items-center rounded-md border border-border bg-background px-3.5 py-2 text-xs font-medium text-primary no-underline transition-colors hover:bg-muted" href="https://www.kaggle.com/settings/api" target="_blank" rel="noreferrer">Kaggle API settings</a> <a class="inline-flex items-center rounded-md border border-border bg-background px-3.5 py-2 text-xs font-medium text-primary no-underline transition-colors hover:bg-muted" href="https://github.com/Kaggle/kaggle-cli/blob/main/docs/kernels.md" target="_blank" rel="noreferrer">Kernel job CLI guide</a></Card.Root>
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">CircleCI research/docs gates</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">Forge reads account, project, pipeline, and workflow status. The project slug list is comma-separated, for example <code class="text-muted-foreground">gh/Zakarialabib/tv-rs</code>. Connect the repository in CircleCI and add the reviewed gate config to its <code class="text-muted-foreground">.circleci/config.yml</code>.</p><a class="inline-flex items-center rounded-md border border-border bg-background px-3.5 py-2 text-xs font-medium text-primary no-underline transition-colors hover:bg-muted" href="https://app.circleci.com/projects" target="_blank" rel="noreferrer">Connect CircleCI project</a> <a class="inline-flex items-center rounded-md border border-border bg-background px-3.5 py-2 text-xs font-medium text-primary no-underline transition-colors hover:bg-muted" href="https://circleci.com/docs/api/v2/" target="_blank" rel="noreferrer">API reference</a></Card.Root>
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">Upload private signage dataset</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">Create or update {field("HF_DATASET_REPO")?.value || "a dataset repo set above"} with the local docs corpus. Upload includes current uncommitted docs from both source projects.</p><Button variant="outline" size="sm" onclick={() => (confirmExternal = "upload")} disabled={busy || !snapshot.hf_cli_installed || !(field("HF_TOKEN_WRITE")?.configured || field("HF_TOKEN")?.configured) || !field("HF_DATASET_REPO")?.value}>Review upload</Button></Card.Root>
        <Card.Root class="grid content-start justify-items-start gap-2 rounded-[7px] bg-card p-[13px] text-[10px]"><strong class="text-xs font-bold text-foreground">Deploy Modal search endpoint</strong><p class="m-0 min-h-8 leading-[1.5] text-muted-foreground">Create or replace the two Modal secrets and deploy the authenticated read-only search service using the pinned HF revision.</p><Button variant="outline" size="sm" onclick={() => (confirmExternal = "deploy")} disabled={busy || !(field("HF_READ_TOKEN")?.configured || field("HF_TOKEN")?.configured) || !field("HF_DATASET_REPO")?.value || !field("HF_DATASET_REVISION")?.value || !snapshot.modal_cli_installed || !snapshot.modal_profile_configured}>Review deployment</Button></Card.Root>
      </Card.Content></Card.Root>
  {/if}

  {#if message}<div class="my-3 rounded-[7px] border border-success/40 bg-success/10 px-3.5 py-3 text-[11px] whitespace-pre-wrap break-words text-success" role="status">{message}</div>{/if}
  {#if error}<div class="mb-3.5 flex items-start justify-between gap-2.5 rounded-[9px] border border-destructive/30 bg-destructive/10 px-[13px] py-2.5 text-xs text-destructive" role="alert">{error}</div>{/if}
  {#if confirmExternal}
    <Dialog.Root open={confirmExternal !== null} onOpenChange={(next) => { if (!next) confirmExternal = null; }}>
      <Dialog.Content showCloseButton={false} class="w-full max-w-[520px] gap-0 rounded-[13px] border border-border bg-card p-[22px] shadow-[0_20px_60px_#0d2a4335]">
        <span class="text-[9px] font-extrabold tracking-[1.2px] text-muted-foreground">EXTERNAL SERVICE ACTION</span>
        {#if confirmExternal === "upload"}
          <Dialog.Title class="mt-1.5 mb-[7px] text-[18px] font-bold text-foreground">Upload signage docs to Hugging Face?</Dialog.Title>
          <Dialog.Description class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">This creates or updates the private dataset <strong class="font-bold text-foreground">{field("HF_DATASET_REPO")?.value}</strong> using your configured HF token.</Dialog.Description>
          <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Data sent</strong><span>The full local signage docs index (about 4.2 MB): indexed docs from Simple-Signage and companion-app, including their current uncommitted documentation edits. The builder filtered secret-like content but human review is still recommended.</span></div>
          <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Access</strong><span>The HF dataset is set private. HF receives and stores the corpus as a new dataset revision. Upload uses HF_TOKEN_WRITE when set, otherwise HF_TOKEN; the selected token needs dataset repo.write permission.</span></div>
        {:else}
          <Dialog.Title class="mt-1.5 mb-[7px] text-[18px] font-bold text-foreground">Deploy Modal search?</Dialog.Title>
          <Dialog.Description class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">This creates or replaces Modal secrets and deploys the authenticated search service for <strong class="font-bold text-foreground">{field("HF_DATASET_REPO")?.value}</strong> at revision <strong class="font-bold text-foreground">{field("HF_DATASET_REVISION")?.value}</strong>.</Dialog.Description>
          <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Credentials sent to Modal</strong><span>{field("HF_READ_TOKEN")?.configured ? "The dedicated HF_READ_TOKEN" : "The configured HF_TOKEN"}, dataset repo and pinned revision are stored in the Modal secret so the service can download the private corpus. {field("HF_READ_TOKEN")?.configured ? "This uses the dedicated read token." : "Consider adding a least-privilege read-only HF_READ_TOKEN in Settings."} A generated search bearer token is also stored in a Modal secret and locally in forge/.env.</span></div>
          <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Service behavior</strong><span>Deploy publishes an HTTPS endpoint that anyone can reach, but search requests require the bearer token. Modal compute may use account quota or incur charges depending on your plan and usage.</span></div>
        {/if}
        <div class="mt-[18px] flex justify-end gap-2"><Button variant="outline" size="sm" class="text-[10.5px]" onclick={() => (confirmExternal = null)} disabled={busy}>Cancel</Button><Button variant="default" size="sm" class="text-[10.5px]" onclick={runConfirmedExternal} disabled={busy}>{confirmExternal === "upload" ? "Upload corpus" : "Create secrets and deploy"}</Button></div>
      </Dialog.Content>
    </Dialog.Root>
  {/if}
  {#if probeKey}
    <Dialog.Root open={probeKey !== null} onOpenChange={(next) => { if (!next) probeKey = null; }}>
      <Dialog.Content showCloseButton={false} class="w-full max-w-[520px] gap-0 rounded-[13px] border border-border bg-card p-[22px] shadow-[0_20px_60px_#0d2a4335]">
        <span class="text-[9px] font-extrabold tracking-[1.2px] text-muted-foreground">READ-ONLY CREDENTIAL CHECK</span>
        <Dialog.Title class="mt-1.5 mb-[7px] text-[18px] font-bold text-foreground">Probe {label(probeKey)}?</Dialog.Title>
        <Dialog.Description class="mb-3 text-[11px] leading-[1.55] text-muted-foreground">The saved credential is sent in one cheap GET to this rung's identity or health endpoint. No crate source, prompt, or spend endpoint is involved.</Dialog.Description>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Rung</strong><span>{credentialProbe[probeKey]}</span></div>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Credential</strong><span>{probeKey} · value stays masked, never rendered</span></div>
        <div class="mt-[11px] flex flex-col gap-0.5 text-[10px] text-muted-foreground"><strong class="text-[10.5px] text-foreground">Stored locally</strong><span>Success/failure, timestamp, and served model IDs only. The response body is discarded.</span></div>
        {#if probeMessage}<div class="mt-3 rounded-[7px] bg-muted p-[9px] text-[10px] text-muted-foreground" role="status">{probeMessage}</div>{/if}
        <div class="mt-[18px] flex justify-end gap-2"><Button variant="outline" size="sm" class="text-[10.5px]" onclick={() => (probeKey = null)} disabled={probeBusy}>Cancel</Button><Button variant="default" size="sm" class="text-[10.5px]" onclick={confirmProbe} disabled={probeBusy}>{probeBusy ? "Probing…" : "Send credential and probe"}</Button></div>
      </Dialog.Content>
    </Dialog.Root>
  {/if}
  <footer class="mt-3 flex flex-col gap-3 text-[9px] text-muted-foreground min-[760px]:flex-row min-[760px]:justify-between"><span>Secrets stay local in forge/.env</span><span>Probe is a cheap GET — never a spend endpoint. Free tiers only: spend stays €0.00.</span></footer>
</section>
