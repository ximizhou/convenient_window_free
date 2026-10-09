<script lang="ts">
  import { onDestroy } from "svelte";
  import { zh, en, format } from "./i18n";
  import type { UiKey } from "./i18n";
  import { defaultSettings } from "./settings-store";
  import type { TaskbarAppearanceMode, TaskbarAppearanceSettings, TaskbarAppearanceStatus } from "./types";

  export let english = false;
  export let appearance: TaskbarAppearanceSettings = { ...defaultSettings.taskbarAppearance };
  export let connected = false;
  export let masterEnabled = false;
  export let status: TaskbarAppearanceStatus | null = null;
  export let onChange: (patch: Partial<TaskbarAppearanceSettings>) => void;

  const materials: Array<{ id: TaskbarAppearanceMode; label: UiKey; detail: UiKey }> = [
    { id: "transparent", label: "beautyClear", detail: "beautyClearDetail" },
    { id: "acrylic", label: "beautyAcrylic", detail: "beautyAcrylicDetail" },
    { id: "solid", label: "beautySolid", detail: "beautySolidDetail" }
  ];
  let retryPending = false;
  let retryTimer: ReturnType<typeof setTimeout> | null = null;
  let retryRestoreFailed = false;

  function cancelRetry(): void {
    retryPending = false;
    if (retryTimer) clearTimeout(retryTimer);
    retryTimer = null;
  }

  function applyAppearance(): void {
    retryRestoreFailed = false;
    if (!appearance.enabled) {
      onChange({ enabled: true });
      return;
    }
    // Retry uses the existing config path: restore first, wait for the helper
    // acknowledgement, then enable. No new IPC or unrelated-revision trigger.
    retryPending = true;
    retryTimer = setTimeout(() => {
      cancelRetry();
      retryRestoreFailed = true;
    }, 5000);
    onChange({ enabled: false });
  }

  function restoreAppearance(): void {
    cancelRetry();
    retryRestoreFailed = false;
    onChange({ enabled: false });
  }

  $: if (retryPending && (!connected || !masterEnabled)) cancelRetry();
  $: if (retryPending && !appearance.enabled && status?.state === "inactive") {
    cancelRetry();
    onChange({ enabled: true });
  }
  onDestroy(cancelRetry);

  $: t = english ? en : zh;
  $: supportsMaterials = materials.every((material) => status?.materials?.includes(material.id));
  $: applied = status?.state === "applied" && !status?.terminal && appearance.enabled && supportsMaterials;
  $: canRetry = connected && masterEnabled && status?.retryable !== false
    && (status?.retryable === true || supportsMaterials)
    && (status?.state === "error" || status?.state === "unsupported" || status?.state === "conflict");
  $: canApply = (connected && masterEnabled && status?.available === true && supportsMaterials) || canRetry;
  $: busy = !status?.terminal && (status?.state === "connecting" || status?.state === "recovering" || status?.state === "restoring");
  $: tintEnabled = appearance.mode !== "transparent";
  $: selected = materials.find((material) => material.id === appearance.mode) ?? materials[0];
  $: previewInk = !tintEnabled || appearance.opacity < 45 || tintBrightness(appearance.tint) > 160 ? "#294471" : "#EAF1FC";
  $: previewStyle = `--preview-tint:${appearance.tint};--preview-opacity:${tintEnabled ? appearance.opacity / 100 : 0};--preview-ink:${previewInk}`;
  $: stateKey = retryRestoreFailed ? "beautyRestoreError" : retryPending ? "beautyRestoring"
    : statusKey(connected, masterEnabled, status, supportsMaterials, appearance.enabled);

  function tintBrightness(hex: string): number {
    const channels = [1, 3, 5].map((offset) => parseInt(hex.slice(offset, offset + 2), 16));
    return channels[0] * .299 + channels[1] * .587 + channels[2] * .114;
  }

  function statusKey(isConnected: boolean, masterOn: boolean, current: TaskbarAppearanceStatus | null, supported: boolean, enabled: boolean): UiKey {
    if (!isConnected || !masterOn) return "beautyNeedHelper";
    if (!current) return "beautyPending";
    if (current.state === "restoring") return "beautyRestoring";
    if (current.state === "recovering" && !current.terminal) return enabled ? "beautyRecovering" : "beautyRestoring";
    if (current.state === "conflict") return "beautyConflict";
    if (current.state === "unsupported") return "beautyUnsupported";
    if (current.state === "error" && current.errorCode === "0x80070666") return "beautyVersionMismatch";
    if (current.terminal && enabled) return "beautyRetryExhausted";
    if (current.state === "unavailable") return "beautyModule";
    if (current.state === "error") return current.errorCode === "0x80070666" ? "beautyVersionMismatch" : enabled ? "beautyError" : "beautyRestoreError";
    if (!supported) return "beautyNeedUpdate";
    if (current.state === "connecting") return "beautyConnecting";
    if (current.state === "applied") return enabled ? "beautyApplied" : "beautyRestoring";
    return "beautyInactive";
  }
</script>

<section class="beauty-panel" aria-label={t.beautification}>
  <div class="beauty-intro"><h2>{t.taskbarTitle}</h2><p>{t.taskbarDetail}</p></div>

  <div class="beauty-preview" aria-label={t.beautyPreview}>
    <div class:preview-acrylic={appearance.mode === "acrylic"} class="beauty-wallpaper" style={previewStyle}>
      <span class="beauty-orbit orbit-one"></span><span class="beauty-orbit orbit-two"></span>
      <span class="preview-caption">{t.beautyPreview}</span>
      <div class:preview-border={appearance.showBorder} class="preview-taskbar" aria-hidden="true">
        <div class="preview-apps"><span class="preview-windows"><i></i><i></i><i></i><i></i></span><span class="preview-search"></span><span class="preview-folder"></span><span class="preview-browser"></span></div>
        <div class="preview-tray"><span>⌁</span><span>10:24</span></div>
      </div>
    </div>
  </div>

  <div class="beauty-materials" role="group" aria-label={t.taskbarTitle}>
    {#each materials as material}
      <button class:chosen={appearance.mode === material.id} class={`material-card material-${material.id}`} aria-label={t[material.label]} aria-pressed={appearance.mode === material.id} title={t[material.detail]} on:click={() => onChange({ mode: material.id })} type="button">
        <span class="material-icon" aria-hidden="true"><i></i><b></b></span><strong>{t[material.label]}</strong>
      </button>
    {/each}
  </div>
  <p class="beauty-material-detail" id="taskbar-material-detail">{t[selected.detail]}</p>

  <div class="beauty-controls" class:disabled={!tintEnabled}>
    <label class="beauty-range"><span><strong>{t.beautyOpacity}</strong><output>{tintEnabled ? `${appearance.opacity}%` : "—"}</output></span><input aria-label={t.beautyOpacity} aria-describedby="taskbar-material-detail" max="100" min="0" step="1" type="range" value={appearance.opacity} disabled={!tintEnabled} on:input={(event) => onChange({ opacity: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
    <label class="beauty-color"><span><strong>{t.beautyTint}</strong><small>{tintEnabled ? appearance.tint : t.beautyNoTint}</small></span><input aria-label={t.beautyTint} type="color" value={appearance.tint} disabled={!tintEnabled} on:input={(event) => onChange({ tint: (event.currentTarget as HTMLInputElement).value.toUpperCase() })} /></label>
  </div>
  <label class="beauty-check"><span>{t.beautyBorder}</span><input type="checkbox" checked={appearance.showBorder} on:change={(event) => onChange({ showBorder: (event.currentTarget as HTMLInputElement).checked })} /><i aria-hidden="true"></i></label>

  <div class="beauty-actions"><button class="apply" disabled={!canApply || retryPending || (busy && appearance.enabled) || applied} on:click={applyAppearance} type="button">{applied ? t.enabledState : appearance.enabled ? t.beautyTryAgain : t.beautyActivate}</button><button class="quiet" disabled={!appearance.enabled && !applied && !busy && !retryPending && !status?.terminal && status?.state !== "error"} on:click={restoreAppearance} type="button">{t.beautyRestore}</button></div>
  <div class="beauty-status" class:success={applied} class:problem={status?.state === "error" || status?.state === "conflict" || status?.state === "unsupported"} aria-live="polite"><i></i><div><span>{t[stateKey]}</span>{#if status?.backgrounds && applied}<small>{format(t.beautyMonitors, {count: status.backgrounds})}</small>{/if}</div></div>
  {#if status?.errorCode}<details class="beauty-diagnostics"><summary>{t.beautyDiagnostics}</summary><code>{status.errorCode}</code></details>{/if}
  <details class="beauty-safety"><summary>{t.beautySafety}</summary><p>{t.beautySafetyDetail}</p><p>{t.beautyScope}</p><p>{t.beautyPreviewNote}</p></details>
</section>
