<script lang="ts">
  import { onMount } from "svelte";
  import { loadHostValue, saveHostValue } from "./host-storage";

  export let workspace: HTMLElement | null = null;
  export let mode: string | null = null;
  export let label: string;
  export let resetLabel: string;
  export let controls: string | undefined = undefined;

  const STORAGE_KEY = "convenient-window-pane-widths";
  const WIDTH_PROPERTY = "--user-drawer-width";
  const MODES = ["power", "hotzones", "edge-hide", "gestures", "beautification", "window-layouts", "startup", "more"];
  const PREVIEW_MIN = 180;
  const STEP = 16;

  let preferences: Record<string, number> = {};
  let mounted = false;
  let handle: HTMLDivElement;
  let activeWorkspace: HTMLElement | null = null;
  let activeMode: string | null = null;
  let observer: ResizeObserver | null = null;
  let frame = 0;
  let available = false;
  let workspaceWidth = 0;
  let minimum = 310;
  let maximum = 310;
  let currentWidth = 0;
  let drag: { id: number; startX: number; startWidth: number; x: number; moved: boolean } | null = null;

  $: if (mounted) configure(workspace, mode);

  function normalizePreferences(value: unknown): Record<string, number> {
    if (!value || typeof value !== "object" || Array.isArray(value)) return {};
    const result: Record<string, number> = {};
    for (const key of MODES) {
      const width = (value as Record<string, unknown>)[key];
      if (typeof width === "number" && Number.isFinite(width) && width > 0) result[key] = Math.round(width);
    }
    return result;
  }

  function clamp(width: number): number {
    return Math.round(Math.min(maximum, Math.max(minimum, width)));
  }

  function applyWidth(width: number): void {
    currentWidth = clamp(width);
    activeWorkspace?.style.setProperty(WIDTH_PROPERTY, `${currentWidth}px`);
  }

  function refresh(): void {
    if (!activeWorkspace || !activeMode) return;
    workspaceWidth = Math.floor(activeWorkspace.getBoundingClientRect().width);
    minimum = activeMode === "gestures" ? 560 : activeMode === "startup" ? 400 : 310;
    maximum = workspaceWidth - PREVIEW_MIN;
    available = window.innerWidth > 700 && maximum >= minimum;
    if (!available) {
      finishDrag(false);
      activeWorkspace.style.removeProperty(WIDTH_PROPERTY);
      return;
    }
    if (drag) {
      applyWidth(drag.startWidth + drag.startX - drag.x);
    } else if (preferences[activeMode] !== undefined) {
      // Clamp for the current viewport without replacing the user's stored width.
      applyWidth(preferences[activeMode]);
    } else {
      activeWorkspace.style.removeProperty(WIDTH_PROPERTY);
      const drawer = activeWorkspace.querySelector<HTMLElement>(".drawer");
      currentWidth = drawer?.getBoundingClientRect().width ?? minimum;
    }
  }

  function scheduleRefresh(): void {
    if (!mounted || !activeWorkspace || !activeMode) return;
    if (!frame) frame = requestAnimationFrame(() => { frame = 0; refresh(); });
  }

  function configure(element: HTMLElement | null, nextMode: string | null): void {
    finishDrag(false);
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    observer?.disconnect();
    observer = null;
    activeWorkspace?.style.removeProperty(WIDTH_PROPERTY);
    activeWorkspace = element;
    activeMode = nextMode && MODES.includes(nextMode) ? nextMode : null;
    available = false;
    if (!activeWorkspace || !activeMode) return;
    observer = new window.ResizeObserver(scheduleRefresh);
    observer.observe(activeWorkspace);
    scheduleRefresh();
  }

  function beginDrag(event: PointerEvent): void {
    if (event.button !== 0 || !event.isPrimary || drag) return;
    refresh();
    if (!available || !activeWorkspace) return;
    event.preventDefault();
    handle.focus({ preventScroll: true });
    drag = { id: event.pointerId, startX: event.clientX, startWidth: currentWidth, x: event.clientX, moved: false };
    activeWorkspace.classList.add("resizing");
    window.addEventListener("pointermove", moveDrag);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", cancelDrag);
    try { handle.setPointerCapture(event.pointerId); } catch { /* Window listeners still finish the gesture if capture is unavailable. */ }
  }

  function moveDrag(event: PointerEvent): void {
    if (!drag || drag.id !== event.pointerId) return;
    event.preventDefault();
    drag.x = event.clientX;
    drag.moved ||= Math.abs(drag.x - drag.startX) >= 1;
    scheduleRefresh();
  }

  function endDrag(event: PointerEvent): void {
    if (!drag || drag.id !== event.pointerId) return;
    drag.x = event.clientX;
    drag.moved ||= Math.abs(drag.x - drag.startX) >= 1;
    refresh();
    finishDrag(true);
  }

  function cancelDrag(event: PointerEvent): void {
    if (drag?.id !== event.pointerId) return;
    finishDrag(false);
    scheduleRefresh();
  }

  function finishDrag(commit: boolean): void {
    const previous = drag;
    drag = null;
    window.removeEventListener("pointermove", moveDrag);
    window.removeEventListener("pointerup", endDrag);
    window.removeEventListener("pointercancel", cancelDrag);
    activeWorkspace?.classList.remove("resizing");
    if (!previous) return;
    if (handle?.hasPointerCapture?.(previous.id)) handle.releasePointerCapture(previous.id);
    if (commit && previous.moved && available && activeMode) {
      preferences[activeMode] = currentWidth;
      saveHostValue(STORAGE_KEY, preferences);
    }
    scheduleRefresh();
  }

  function reset(): void {
    if (!activeMode || !available) return;
    finishDrag(false);
    if (preferences[activeMode] !== undefined) {
      delete preferences[activeMode];
      saveHostValue(STORAGE_KEY, preferences);
    }
    refresh();
  }

  function keydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && drag) {
      event.preventDefault();
      finishDrag(false);
      return;
    }
    if (!["ArrowLeft", "ArrowRight", "Home"].includes(event.key) || !available || !activeMode) return;
    event.preventDefault();
    if (event.key === "Home") { reset(); return; }
    finishDrag(false);
    applyWidth(currentWidth + (event.key === "ArrowLeft" ? STEP : -STEP));
    preferences[activeMode] = currentWidth;
    saveHostValue(STORAGE_KEY, preferences);
  }

  function stopForViewportChange(): void {
    finishDrag(false);
    scheduleRefresh();
  }

  onMount(() => {
    preferences = normalizePreferences(loadHostValue<unknown>(STORAGE_KEY));
    mounted = true;
    window.addEventListener("resize", stopForViewportChange);
    window.addEventListener("blur", stopForViewportChange);
    return () => {
      mounted = false;
      finishDrag(false);
      if (frame) cancelAnimationFrame(frame);
      frame = 0;
      observer?.disconnect();
      activeWorkspace?.style.removeProperty(WIDTH_PROPERTY);
      activeWorkspace = null;
      activeMode = null;
      window.removeEventListener("resize", stopForViewportChange);
      window.removeEventListener("blur", stopForViewportChange);
    };
  });
</script>

<!-- A focusable ARIA separator is the keyboard-operated window splitter widget. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
<div
  bind:this={handle}
  class="workspace-splitter"
  role="separator"
  aria-label={label}
  aria-controls={controls}
  aria-orientation="vertical"
  aria-valuemin={PREVIEW_MIN}
  aria-valuemax={Math.max(PREVIEW_MIN, workspaceWidth - minimum)}
  aria-valuenow={Math.max(PREVIEW_MIN, Math.min(workspaceWidth - minimum, Math.round(workspaceWidth - currentWidth)))}
  title={`${label} · ${resetLabel}`}
  tabindex={available ? 0 : -1}
  hidden={!available}
  on:pointerdown={beginDrag}
  on:lostpointercapture={cancelDrag}
  on:dblclick={reset}
  on:keydown={keydown}
></div>

<style>
  .workspace-splitter { position: absolute; z-index: 8; top: 0; bottom: 0; right: calc(var(--user-drawer-width, var(--drawer-width)) - 5px); width: 10px; cursor: col-resize; touch-action: none; user-select: none; outline: none; }
  .workspace-splitter[hidden] { display: none; }
  .workspace-splitter::before { content: ""; position: absolute; top: 0; bottom: 0; left: 4px; width: 2px; background: var(--accent); opacity: 0; transition: opacity var(--dur-fast); }
  .workspace-splitter::after { content: ""; position: absolute; top: 50%; left: 3px; width: 4px; height: 32px; border: 1px solid var(--line-strong); border-radius: var(--radius-pill); background: var(--raised); transform: translateY(-50%); }
  .workspace-splitter:hover::before, .workspace-splitter:focus-visible::before, :global(.workspace.resizing) > .workspace-splitter::before { opacity: 1; }
  .workspace-splitter:focus-visible::after { background: var(--accent); border-color: var(--accent); }
  @media (max-width: 700px) { .workspace-splitter { display: none; } }
  @media (prefers-reduced-motion: reduce) { .workspace-splitter::before { transition: none; } }
</style>
