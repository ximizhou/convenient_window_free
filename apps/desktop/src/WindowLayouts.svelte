<script lang="ts">
  import { tick } from "svelte";
  import type { HelperClient } from "./helper-client";
  import { format, translator } from "./i18n";
  import type { UiKey } from "./i18n";
  import { syncLayoutHotkeys } from "./layout-hotkeys";
  import { layoutShortcutFromKeyboardEvent } from "./layout-hotkey-utils";
  import { previewBounds, previewWindows, type WindowLayoutPreviewBounds } from "./window-layout-preview";
  import { filterLayoutWindows, loadLayoutExclusions, normalizeLayoutExclusions, saveLayoutExclusions } from "./window-layout-exclusions";
  import {
    createWindowLayoutId,
    loadWindowLayouts,
    saveWindowLayouts,
    type WindowLayoutCapture,
    type WindowLayoutSnapshot,
    type WindowLayoutWindow,
    type WindowLayoutRestoreResult
  } from "./window-layout-store";

  export let helper: HelperClient;
  export let english = false;
  export let connected = false;
  export let supported = false;
  export let platformSupportsWindowControl = true;
  export let foregroundApp = "";

  let layouts: WindowLayoutSnapshot[] = loadWindowLayouts();
  let exclusions: string[] = loadLayoutExclusions();
  let busy: "capture" | string = "";
  let editingId = "";
  let editName = "";
  let recordingShortcutId = "";
  let nameInput: HTMLInputElement | undefined = undefined;
  let status: { kind: "success" | "warning" | "error"; text: string } | null = null;

  let ui = translator("zh-CN");
  $: ui = translator(english ? "en-US" : "zh-CN");
  function text(key: UiKey, values: Record<string, string | number> = {}): string { return format(ui(key), values); }

  async function captureLayout(): Promise<void> {
    if (!connected || !supported || busy) return;
    busy = "capture";
    status = null;
    try {
      const result: WindowLayoutCapture = await helper.captureWindowLayout();
      // 排除名单在宿主侧过滤：helper 不参与，记录的窗口数量就是过滤后的数量。
      const windows = filterLayoutWindows(Array.isArray(result.windows) ? result.windows : [], exclusions);
      const number = layouts.length + 1;
      const snapshot: WindowLayoutSnapshot = {
        id: createWindowLayoutId(),
        name: text("windowLayoutsDefaultName", { count: number }),
        capturedAt: result.capturedAt ?? Date.now(),
        windows
      };
      layouts = [snapshot, ...layouts].slice(0, 24);
      saveWindowLayouts(layouts);
      status = {
        kind: windows.length ? "success" : "warning",
        text: windows.length
          ? text("windowLayoutsRecorded", { count: windows.length })
          : text("windowLayoutsNoWindows")
      };
    } catch (error) {
      status = { kind: "error", text: errorMessage(error) };
    } finally {
      busy = "";
    }
  }

  async function restoreLayout(layout: WindowLayoutSnapshot): Promise<void> {
    if (!connected || !supported || busy) return;
    busy = layout.id;
    status = null;
    try {
      // 复位前先过滤排除的应用：被排除的窗口不发送给 helper，数量并入 skipped 提示。
      const windows = filterLayoutWindows(layout.windows, exclusions);
      const excluded = layout.windows.length - windows.length;
      const result: WindowLayoutRestoreResult = await helper.restoreWindowLayout(windows);
      const skipped = (Number.isFinite(result.skipped) ? result.skipped : 0) + excluded;
      const restored = Number.isFinite(result.restored) ? result.restored : 0;
      status = {
        kind: skipped ? "warning" : "success",
        text: skipped
          ? text("windowLayoutsRestoredSkipped", { restored, skipped })
          : text("windowLayoutsRestored", { count: restored })
      };
    } catch (error) {
      status = { kind: "error", text: errorMessage(error) };
    } finally {
      busy = "";
    }
  }

  function addExclusion(): void {
    const app = foregroundApp.trim();
    if (!app) {
      status = { kind: "warning", text: text("statusForegroundMissing") };
      return;
    }
    // 大小写不敏感去重；已达上限时名单不会变化，也不重复写存储。
    const next = normalizeLayoutExclusions([...exclusions, app]);
    if (sameEntries(next, exclusions)) return;
    exclusions = next;
    saveLayoutExclusions(exclusions);
    status = { kind: "success", text: text("statusAppAdded", { app }) };
  }

  function removeExclusion(app: string): void {
    const next = exclusions.filter((item) => item !== app);
    if (next.length === exclusions.length) return;
    exclusions = next;
    saveLayoutExclusions(exclusions);
  }

  function sameEntries(left: string[], right: string[]): boolean {
    return left.length === right.length && left.every((item, index) => item.toLowerCase() === right[index].toLowerCase());
  }

  function beginRename(layout: WindowLayoutSnapshot): void {
    editingId = layout.id;
    editName = layout.name;
    void tick().then(() => { nameInput?.focus(); nameInput?.select(); });
  }

  // No confirm button: the name is saved on Enter and on blur, while Escape
  // cancels. A blank name keeps the previous one instead of clearing it.
  function commitRename(layout: WindowLayoutSnapshot): void {
    if (editingId !== layout.id) return;
    const name = editName.trim().slice(0, 80);
    editingId = "";
    editName = "";
    if (!name || name === layout.name) return;
    layouts = layouts.map((item) => item.id === layout.id ? { ...item, name } : item);
    saveWindowLayouts(layouts);
  }

  function cancelRename(): void {
    editingId = "";
    editName = "";
  }

  function deleteLayout(layout: WindowLayoutSnapshot): void {
    layouts = layouts.filter((item) => item.id !== layout.id);
    saveWindowLayouts(layouts);
    if (editingId === layout.id) editingId = "";
    if (recordingShortcutId === layout.id) recordingShortcutId = "";
    status = { kind: "success", text: text("windowLayoutsDeleted") };
    // Persist the removal in the global shortcut registry as well.
    void syncLayoutHotkeys().catch(() => []);
  }

  function beginShortcutRecording(layout: WindowLayoutSnapshot): void {
    recordingShortcutId = recordingShortcutId === layout.id ? "" : layout.id;
  }

  function cancelShortcutRecording(): void {
    recordingShortcutId = "";
  }

  async function applyShortcut(layout: WindowLayoutSnapshot, shortcut: string): Promise<void> {
    recordingShortcutId = "";
    layouts = layouts.map((item) => {
      if (item.id === layout.id) {
        if (!shortcut) {
          const cleared = { ...item };
          delete cleared.shortcut;
          return cleared;
        }
        return { ...item, shortcut };
      }
      // A shortcut belongs to the layout that set it last: drop the same
      // combination from older layouts so exactly one registration stays live
      // instead of letting snapshot order decide which one wins.
      if (shortcut && item.shortcut === shortcut) {
        const cleared = { ...item };
        delete cleared.shortcut;
        return cleared;
      }
      return item;
    });
    saveWindowLayouts(layouts);
    try {
      const failed = await syncLayoutHotkeys();
      if (shortcut && failed.includes(layout.id)) {
        status = { kind: "warning", text: text("windowLayoutsShortcutFailed") };
      }
    } catch {
      if (shortcut) status = { kind: "warning", text: text("windowLayoutsShortcutFailed") };
    }
  }

  // Global keydown while recording. Escape cancels, Backspace/Delete clears the
  // binding, and only a real shortcut (modifier + main key) is committed.
  function handleShortcutKeydown(event: KeyboardEvent): void {
    if (!recordingShortcutId) return;
    const layout = layouts.find((item) => item.id === recordingShortcutId);
    if (!layout) { recordingShortcutId = ""; return; }
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancelShortcutRecording();
      return;
    }
    if (event.key === "Backspace" || event.key === "Delete") {
      event.preventDefault();
      event.stopPropagation();
      void applyShortcut(layout, "");
      return;
    }
    const shortcut = layoutShortcutFromKeyboardEvent(event);
    if (!shortcut) return;
    event.preventDefault();
    event.stopPropagation();
    void applyShortcut(layout, shortcut);
  }

  function shortcutAria(layout: WindowLayoutSnapshot): string {
    if (recordingShortcutId === layout.id) return text("windowLayoutsShortcutRecording");
    if (layout.shortcut) return text("windowLayoutsShortcutAria", { name: layout.name });
    return text("windowLayoutsShortcutSet");
  }

  // The thumbnail draws the windows that are actually on the desktop, in
  // bottom-to-top order, using opaque fills so covered windows disappear.
  function previewStyle(bounds: WindowLayoutPreviewBounds, item: WindowLayoutWindow): string {
    const width = Math.max(1, bounds.right - bounds.left);
    const height = Math.max(1, bounds.bottom - bounds.top);
    const left = clamp(((item.rect.left - bounds.left) / width) * 100, 0, 96);
    const top = clamp(((item.rect.top - bounds.top) / height) * 100, 0, 94);
    const itemWidth = clamp(((item.rect.right - item.rect.left) / width) * 100, 4, 100 - left);
    const itemHeight = clamp(((item.rect.bottom - item.rect.top) / height) * 100, 5, 100 - top);
    return `left:${round(left)}%;top:${round(top)}%;width:${round(itemWidth)}%;height:${round(itemHeight)}%`;
  }

  function formatDate(value: number): string {
    try { return new Intl.DateTimeFormat(english ? "en-US" : "zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }).format(value); }
    catch { return new Date(value).toLocaleString(); }
  }

  function errorMessage(error: unknown): string {
    const message = error instanceof Error ? error.message.trim() : "";
    // Older helpers report an unsupported request as the raw message name
    // (for example `window.layout.capture`). Keep that protocol detail out
    // of the UI and show the actionable localised recovery hint instead.
    if (message && !message.includes("window.layout.") && !message.includes("unknown_message") && !message.includes("timed out")) return message;
    return text("windowLayoutsError");
  }

  function clamp(value: number, min: number, max: number): number { return Math.min(max, Math.max(min, value)); }

  function round(value: number): number { return Math.round(value * 1000) / 1000; }
</script>

<svelte:window on:keydown={handleShortcutKeydown} />

<section class="window-layouts-panel" aria-label={text("windowLayouts")}>
  <div class="window-layouts-intro">
    <div>
      <p>{text("windowLayoutsDescription")}</p>
    </div>
    <button class="apply layout-capture" disabled={!connected || !supported || Boolean(busy)} on:click={captureLayout} type="button">
      <span aria-hidden="true">＋</span>{busy === "capture" ? text("windowLayoutsRecording") : text("windowLayoutsRecord")}
    </button>
  </div>

  {#if !connected}
    <div class="window-layouts-connection" role="status"><i></i>{text("windowLayoutsConnection")}</div>
  {:else if !platformSupportsWindowControl}
    <div class="window-layouts-connection" role="status"><i></i>{text("windowLayoutsUnsupported")}</div>
  {:else if !supported}
    <div class="window-layouts-connection" role="status"><i></i>{text("windowLayoutsHelperOutdated")}</div>
  {/if}
  {#if status}
    <div class:success={status.kind === "success"} class:warning={status.kind === "warning"} class:error={status.kind === "error"} class="window-layouts-status" role="status" aria-live="polite"><i></i>{status.text}</div>
  {/if}

  {#if layouts.length === 0}
    <div class="window-layouts-empty">
      <div class="layout-empty-mark" aria-hidden="true"><i></i><i></i><i></i></div>
      <strong>{text("windowLayoutsEmpty")}</strong>
      <p>{text("windowLayoutsEmptyDescription")}</p>
    </div>
  {:else}
    <div class="window-layout-list">
      {#each layouts as layout (layout.id)}
        {@const visibleWindows = filterLayoutWindows(layout.windows, exclusions)}
        {@const bounds = previewBounds(visibleWindows)}
        {@const windows = previewWindows(visibleWindows, bounds)}
        <article class="window-layout-row">
          <button class="layout-preview" aria-label={text("windowLayoutsRestoreAria", { name: layout.name })} disabled={!connected || !supported || Boolean(busy)} on:click={() => restoreLayout(layout)} type="button">
            <span class="layout-preview-grid" aria-hidden="true"></span>
            <span class="layout-preview-canvas" aria-hidden="true">
              {#each windows as item}
                <span class="layout-preview-window" style={previewStyle(bounds, item)}></span>
              {/each}
            </span>
          </button>
          <div class="window-layout-copy">
            {#if editingId === layout.id}
              <input bind:this={nameInput} aria-label={text("windowLayoutsLayoutName")} bind:value={editName} on:blur={() => commitRename(layout)} on:keydown={(event) => {
                if (event.key === "Enter") { event.preventDefault(); commitRename(layout); }
                else if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); cancelRename(); }
              }} />
            {:else}
              <button class="layout-name" on:click={() => beginRename(layout)} title={text("windowLayoutsRename")} type="button">{layout.name}</button>
            {/if}
            <p>{text("windowLayoutsWindowCount", { count: visibleWindows.length })} · {formatDate(layout.capturedAt)}</p>
          </div>
          <button class:recording={recordingShortcutId === layout.id} class="layout-shortcut" aria-label={shortcutAria(layout)} title={recordingShortcutId === layout.id ? text("windowLayoutsShortcutHint") : shortcutAria(layout)} on:click={() => beginShortcutRecording(layout)} on:blur={() => { if (recordingShortcutId === layout.id) cancelShortcutRecording(); }} type="button">
            {#if recordingShortcutId === layout.id}
              {text("windowLayoutsShortcutRecording")}
            {:else if layout.shortcut}
              {layout.shortcut}
            {:else}
              <span aria-hidden="true">⌨</span>
            {/if}
          </button>
          <div class="window-layout-actions">
            <button class="apply layout-restore" disabled={!connected || !supported || Boolean(busy)} on:click={() => restoreLayout(layout)} type="button">{busy === layout.id ? text("windowLayoutsRestoring") : text("windowLayoutsRestore")}</button>
            <button class="icon-button danger-icon" aria-label={text("windowLayoutsDeleted")} title={text("windowLayoutsDeleted")} on:click={() => deleteLayout(layout)} type="button">×</button>
          </div>
        </article>
      {/each}
    </div>
  {/if}

  <div class="list-section layout-excluded-apps">
    <div class="subhead">
      <div>
        <h2>{text("layoutExcludedApps")}</h2>
        <p>{text("foreground")}{foregroundApp || text("noForeground")}</p>
      </div>
      <button class="quiet" on:click={addExclusion} type="button">+ {text("addCurrentApp")}</button>
    </div>
    <div class="app-list">
      {#each exclusions as app}
        <div><span>{app}</span><button aria-label={`${text("removeApp")} ${app}`} on:click={() => removeExclusion(app)} type="button">×</button></div>
      {:else}
        <p class="empty">{text("noLayoutExcludedApps")}</p>
      {/each}
    </div>
  </div>
</section>

<style>
  .window-layouts-panel { gap: 14px; }
  .window-layouts-intro { align-items: center; gap: 12px; }
  .window-layouts-intro p { max-width: 32em; margin: 0; line-height: 1.5; }
  .layout-capture, .layout-restore {
    border: 1px solid var(--accent-line);
    border-radius: var(--radius-sm);
    background: var(--accent-bg);
    color: var(--accent-soft);
    box-shadow: none;
    font-weight: 620;
    transition: background var(--dur-fast), border-color var(--dur-fast), transform var(--dur-fast);
  }
  .layout-capture:hover:not(:disabled), .layout-restore:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--hover);
    transform: translateY(-1px);
  }
  .window-layouts-status, .window-layouts-connection { border-radius: var(--radius-sm); }
  .window-layout-list { gap: 9px; }
  .window-layout-row {
    grid-template-columns: 64px minmax(0, 1fr) auto;
    grid-template-rows: minmax(34px, auto) 34px;
    gap: 8px 12px;
    padding: 11px 12px;
    border-radius: var(--radius-sm);
    background: var(--surface);
    box-shadow: none;
    transition: border-color var(--dur-fast), background var(--dur-fast);
  }
  .window-layout-row:hover { border-color: var(--line-strong); background: var(--raised); }
  .layout-preview { grid-column: 1; grid-row: 1 / span 2; width: 64px; height: 50px; border-radius: 7px; }
  .layout-preview-canvas { position: absolute; inset: 4px; }
  .window-layout-copy { grid-column: 2; grid-row: 1; align-self: end; }
  .layout-name { font-size: var(--fs-md); }
  .window-layout-copy p { margin-top: 4px; }
  .layout-shortcut {
    grid-column: 3;
    grid-row: 1;
    align-self: center;
    justify-self: end;
    min-width: 30px;
    max-width: 132px;
    height: 26px;
    padding: 0 8px;
    overflow: hidden;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-xs);
    background: var(--surface);
    color: var(--faint-text);
    font-size: var(--fs-2xs);
    font-weight: 620;
    line-height: 24px;
    text-overflow: ellipsis;
    white-space: nowrap;
    transition: color var(--dur-fast), border-color var(--dur-fast), background var(--dur-fast), box-shadow var(--dur-fast);
  }
  .layout-shortcut:hover { border-color: var(--accent); color: var(--ink); }
  .layout-shortcut:focus-visible { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-bg); outline: none; }
  .layout-shortcut.recording { border-color: var(--accent); background: var(--accent-bg); color: var(--accent-soft); box-shadow: 0 0 0 3px var(--accent-bg); }
  .window-layout-actions {
    grid-column: 1 / -1;
    grid-row: 2;
    display: grid;
    grid-template-columns: 32px minmax(0, 1fr) 32px;
    align-items: center;
    justify-items: stretch;
    gap: 8px;
    padding: 0;
  }
  .window-layout-actions .layout-restore { grid-column: 2; justify-self: center; width: auto; min-width: 96px; padding-inline: 16px; min-height: 31px; }
  .window-layout-actions .danger-icon { grid-column: 3; justify-self: end; }
  .window-layout-actions .icon-button { width: 30px; height: 30px; border-radius: 7px; }
  .layout-name:hover { text-decoration: underline; text-underline-offset: 3px; }
  @media (max-width: 520px) {
    .window-layouts-intro { display: flex; align-items: flex-start; }
    .layout-capture { align-self: center; }
    .window-layout-row { grid-template-columns: 56px minmax(0, 1fr) auto; }
    .layout-preview { width: 56px; height: 46px; }
    .layout-shortcut { max-width: 96px; }
  }
</style>
