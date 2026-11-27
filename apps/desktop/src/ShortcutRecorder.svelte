<script lang="ts">
  import { onMount } from "svelte";
  import { translator } from "./i18n";
  import { nextRecordingStep } from "./shortcut-keys";
  import type { HelperClient } from "./helper-client";
  export let helper: HelperClient | undefined = undefined;
  export let value = ""; export let onChange: (value: string) => void; export let label = "录制快捷键"; export let english = false;
  let recording = false; let captured = ""; let root: HTMLButtonElement;
  let sessionId = "";
  let timeout: ReturnType<typeof setTimeout> | undefined;
  let offNative: (() => void) | undefined;
  let notice = "";
  $: ui = translator(english ? "en-US" : "zh-CN");
  function stop() {
    if (sessionId) helper?.cancelShortcutRecording(sessionId);
    sessionId = "";
    recording = false; captured = "";
    clearTimeout(timeout);
    offNative?.(); offNative = undefined;
  }
  function start() {
    stop(); recording = true; captured = ""; notice = ""; root?.focus();
    if (helper?.supportsShortcutRecording) {
      sessionId = crypto.randomUUID();
      offNative = helper.onMessage(message => {
        if (message.type !== "shortcut.recording") return;
        const event = message.data as { sessionId?: string; phase?: string; hint?: string; value?: string; reason?: string };
        if (event.sessionId !== sessionId || !recording) return;
        if (event.phase === "progress") {
          captured = event.hint ?? "";
          notice = event.reason === "unsupported-key" ? ui("shortcutRecordingUnsupported") : "";
        }
        if (event.phase === "committed" && event.value) {
          const value = event.value; stop(); onChange(value);
        } else if (event.phase === "cancelled") {
          if (event.reason === "unavailable") notice = ui("shortcutRecordingUnavailable");
          stop();
        }
      });
      if (!helper.beginShortcutRecording(sessionId)) { stop(); notice = ui("shortcutRecordingReconnect"); return; }
    }
    timeout = setTimeout(stop, 15000);
  }
  function outside(event: PointerEvent | FocusEvent) {
    if (recording && (!(event.target instanceof Node) || !root?.contains(event.target))) stop();
  }
  function visibility() { if (document.hidden) stop(); }
  function clear(e: MouseEvent) { e.stopPropagation(); stop(); onChange(""); }

  // 录制期间在 window 的捕获阶段监听按键，而不是只监听按钮本身。
  // 否则焦点一旦不在按钮上，按键就送不到处理器；而 Ctrl+C、Ctrl+V、Ctrl+X
  // 这类组合键本身是浏览器快捷键，会被优先消费掉，表现成「组合键录不上」。
  // 按钮上也绑同一处理器作为兜底；捕获阶段已 stopPropagation，两者不会重复触发。
  function handleKeydown(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault(); e.stopPropagation();
    if (sessionId) { if (e.key === "Escape") stop(); return; }
    const step = nextRecordingStep(e);
    if (step.action === "cancel") { stop(); return; }
    if (step.action === "wait") { captured = step.hint; return; }
    stop(); onChange(step.value);
  }
  onMount(() => {
    window.addEventListener("keydown", handleKeydown, true);
    window.addEventListener("pointerdown", outside, true);
    window.addEventListener("focusin", outside, true);
    window.addEventListener("blur", stop);
    document.addEventListener("visibilitychange", visibility);
    const offStatus = helper?.onStatus(status => { if (status !== "connected") stop(); });
    return () => {
      stop(); offStatus?.();
      window.removeEventListener("keydown", handleKeydown, true);
      window.removeEventListener("pointerdown", outside, true);
      window.removeEventListener("focusin", outside, true);
      window.removeEventListener("blur", stop);
      document.removeEventListener("visibilitychange", visibility);
    };
  });
</script>
<div class="shortcut-recorder-wrap">{#if notice}<small class="recording-notice" role="status">{notice}</small>{/if}<div class:recording class="shortcut-recorder"><button bind:this={root} aria-label={label} on:click={start} on:keydown={handleKeydown} type="button">{#if recording && captured}<span class="key-row">{#each captured.split("+") as key}<kbd>{key}</kbd>{/each}</span>{:else if value}<span class="key-row">{#each value.split("+") as key}<kbd>{key}</kbd>{/each}</span>{:else}<span class="empty">{recording ? ui("shortcutPressAnyKey") : ui("shortcutRecordHint")}</span>{/if}</button>{#if value}<button class="clear" aria-label={ui("shortcutClear")} on:click={clear} type="button">×</button>{/if}</div></div>
<style>
.shortcut-recorder-wrap{min-width:0}.recording-notice{display:block;margin-bottom:6px;color:var(--muted);font-size: var(--fs-sm);line-height:1.5}
.shortcut-recorder{min-width:0;display:grid;grid-template-columns:minmax(0,1fr) 30px;gap:6px}.shortcut-recorder:not(:has(.clear)){grid-template-columns:1fr}.shortcut-recorder>button:first-child{height:38px;min-width:0;padding:5px 9px;border:1px solid var(--line-strong);border-radius: var(--radius-xs);background:var(--bg);color:var(--ink);display:flex;align-items:center;justify-content:flex-start}.shortcut-recorder>button:first-child:hover,.shortcut-recorder.recording>button:first-child{border-color:var(--accent);box-shadow:0 0 0 3px var(--accent-bg)}.key-row{display:flex;gap:4px;overflow:hidden}.key-row kbd{min-width:28px;height:24px;padding:0 6px;border:1px solid var(--line-strong);border-bottom-width:2px;border-radius: var(--radius-2xs);background:var(--surface);font: 600 var(--fs-xs)/21px "Segoe UI Variable",sans-serif;text-align:center}.empty{color:var(--muted);font-size: var(--fs-sm)}.clear{width:30px;height:30px;border:1px solid var(--line);background:transparent;color:var(--muted);font-size: var(--fs-xl)}.clear:hover{color:var(--danger);border-color:var(--danger)}
</style>
