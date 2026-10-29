<script lang="ts">
  import { diagnosticLabels, type DiagnosticReport } from "./diagnostics";

  export let english = false;
  export let collect: () => DiagnosticReport | Promise<DiagnosticReport>;
  export let saveReport: ((json: string) => Promise<boolean>) | undefined = undefined;

  let report = "";
  let busy = false;
  let feedback: "copied" | "saved" | "downloadStarted" | "failed" | "copyFailed" | "saveFailed" | null = null;
  $: t = english ? diagnosticLabels.en : diagnosticLabels.zh;

  async function generate(): Promise<void> {
    if (busy) return;
    busy = true;
    feedback = null;
    report = "";
    try { report = JSON.stringify(await collect(), null, 2); }
    catch { feedback = "failed"; }
    finally { busy = false; }
  }

  async function copy(): Promise<void> {
    try { await navigator.clipboard.writeText(report); feedback = "copied"; }
    catch { feedback = "copyFailed"; }
  }

  async function save(): Promise<void> {
    if (busy) return;
    feedback = null;
    if (saveReport) {
      busy = true;
      try { if (await saveReport(report)) feedback = "saved"; }
      catch { feedback = "saveFailed"; }
      finally { busy = false; }
      return;
    }
    let url: string | null = null;
    try {
      url = URL.createObjectURL(new Blob([report], { type: "application/json;charset=utf-8" }));
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = "convenient-window-diagnostics.json";
      anchor.click();
      feedback = "downloadStarted";
    } catch { feedback = "saveFailed"; }
    finally { if (url) setTimeout(() => URL.revokeObjectURL(url!), 1000); }
  }
</script>

<section class="diagnostics-panel" aria-label={t.title}>
  <div class="diagnostics-heading">
    <div><h3>{t.title}</h3><p>{t.description}</p></div>
    <button class="quiet" type="button" on:click={generate} disabled={busy}>{busy ? t.loading : report ? t.refresh : t.generate}</button>
  </div>
  {#if report}
    <p class="diagnostics-privacy">{t.privacy}</p>
    <details>
      <summary>{t.preview}</summary>
      <textarea aria-label={t.preview} readonly value={report} rows="9"></textarea>
    </details>
    <div class="diagnostics-actions"><button class="quiet" type="button" disabled={busy} on:click={copy}>{t.copy}</button><button class="quiet" type="button" disabled={busy} on:click={save}>{t.save}</button></div>
  {/if}
  {#if feedback}<p class="diagnostics-feedback" role="status">{t[feedback]}</p>{/if}
</section>

<style>
  .diagnostics-panel { margin-top: 16px; padding-top: 14px; border-top: 1px solid var(--line); min-width: 0; text-align: left; }
  .diagnostics-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  h3 { margin: 0; color: var(--ink); font-size: var(--fs-md); }
  p { margin: 5px 0 0; color: var(--faint-text); font-size: var(--fs-xs); line-height: 1.6; }
  .diagnostics-heading button { flex: none; }
  .diagnostics-privacy { margin: 12px 0; }
  details { min-width: 0; padding: 10px 12px; border: 1px solid var(--line); border-radius: var(--radius-sm); background: var(--surface); }
  summary { cursor: pointer; color: var(--muted); font-size: var(--fs-sm); }
  summary:focus-visible, textarea:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  textarea { box-sizing: border-box; display: block; width: 100%; max-height: 300px; resize: vertical; margin: 10px 0 0; padding: 8px; border: 1px solid var(--line); border-radius: var(--radius-xs); background: var(--panel); color: var(--muted); font: var(--fs-xs)/1.6 Consolas, monospace; overflow-wrap: anywhere; }
  .diagnostics-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 10px; }
  .diagnostics-feedback { color: var(--muted); }
  @media (max-width: 480px) { .diagnostics-heading { flex-direction: column; gap: 8px; } }
</style>
