<script lang="ts">
  import { translator } from "./i18n";
  import type { ModifierKey } from "./types";

  export let value: ModifierKey[] = [];
  export let onChange: (value: ModifierKey[]) => void;
  export let label = "录制组合键";
  export let english = false;
  export let allowEmpty = true;
  // 空状态提示可定制：手势草稿区需要「点击按下组合键」这样更明确的引导，
  // 其他场景沿用默认文案。
  export let emptyHint = "";

  $: ui = translator(english ? "en-US" : "zh-CN");

  const order: ModifierKey[] = ["ctrl", "alt", "shift", "win"];
  const names: Record<ModifierKey, string> = { ctrl: "Ctrl", alt: "Alt", shift: "Shift", win: "Win" };
  let recording = false;
  let captured: ModifierKey[] = [];
  let root: HTMLButtonElement;

  function modifiersFromEvent(event: KeyboardEvent): ModifierKey[] {
    const keys: ModifierKey[] = [];
    if (event.ctrlKey || event.key === "Control") keys.push("ctrl");
    if (event.altKey || event.key === "Alt") keys.push("alt");
    if (event.shiftKey || event.key === "Shift") keys.push("shift");
    if (event.metaKey || event.key === "Meta") keys.push("win");
    return order.filter((key) => keys.includes(key));
  }

  function start(): void {
    captured = [];
    recording = true;
    root.focus();
  }

  function finish(): void {
    if (!recording) return;
    recording = false;
    if (captured.length) onChange([...captured]);

    captured = [];
  }

  function keydown(event: KeyboardEvent): void {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      recording = false;
      captured = [];
      return;
    }
    const modifiers = modifiersFromEvent(event);
    if (modifiers.length) captured = modifiers;
    if (!["Control", "Alt", "Shift", "Meta"].includes(event.key)) finish();
  }

  function keyup(event: KeyboardEvent): void {
    if (!recording) return;
    event.preventDefault();
    const released = event.key === "Control" ? "ctrl"
      : event.key === "Alt" ? "alt"
      : event.key === "Shift" ? "shift"
      : event.key === "Meta" ? "win"
      : null;
    const remaining = modifiersFromEvent(event).filter((modifier) => modifier !== released);
    if (!remaining.length) finish();
  }

  function blur(): void {
    finish();
  }

  function clear(event: MouseEvent): void {
    event.stopPropagation();
    recording = false;
    captured = [];
    if (allowEmpty) onChange([]);
  }
</script>

<div class:recording class="modifier-recorder">
  <button
    aria-label={label}
    bind:this={root}
    on:blur={blur}
    on:click={start}
    on:keydown={keydown}
    on:keyup={keyup}
    type="button"
  >
    {#if recording && captured.length}
      <span class="key-row">{#each captured as key}<kbd>{names[key]}</kbd>{/each}</span>
    {:else if value.length}
      <span class="key-row">{#each value as key}<kbd>{names[key]}</kbd>{/each}</span>
    {:else}
      <span class="empty">{recording ? ui("modifierPressKeys") : (emptyHint || (allowEmpty ? ui("direct") : ui("modifierRequired")))}</span>
    {/if}
    {#if recording}<i aria-hidden="true"></i>{/if}
  </button>
  {#if allowEmpty && value.length}
    <button aria-label={ui("modifierClear")} class="clear" on:click={clear} title={ui("modifierClear")} type="button">×</button>
  {/if}
</div>

<style>
  .modifier-recorder{min-width:0;display:grid;grid-template-columns:minmax(0,1fr) 30px;gap:6px;align-items:center}.modifier-recorder:not(:has(.clear)){grid-template-columns:1fr}
  .modifier-recorder>button:first-child{height:38px;min-width:0;padding:5px 9px;border:1px solid var(--line-strong);border-radius: var(--radius-xs);background:var(--bg);color:var(--ink);display:flex;align-items:center;justify-content:space-between;transition: border-color var(--dur-fast),box-shadow var(--dur-fast),background-color var(--dur-base)}
  .modifier-recorder>button:first-child:hover,.modifier-recorder.recording>button:first-child{border-color:var(--accent);box-shadow:0 0 0 3px var(--accent-bg)}
  .key-row{min-width:0;display:flex;gap:4px;overflow:hidden}.key-row kbd{min-width:34px;height:24px;padding:0 6px;border:1px solid var(--line-strong);border-bottom-width:2px;border-radius: var(--radius-2xs);background:var(--surface);color:var(--ink);font: 600 var(--fs-xs)/21px "Segoe UI Variable","Microsoft YaHei",sans-serif;text-align:center}
  .empty{color:var(--muted);font-size: var(--fs-sm)}.modifier-recorder i{width:7px;height:7px;border-radius:50%;background:var(--accent);box-shadow:0 0 0 3px var(--accent-bg)}
  .clear{width:30px;height:30px;padding:0;border:1px solid var(--line);border-radius: var(--radius-xs);background:transparent;color:var(--muted);font-size: var(--fs-xl);line-height:1}.clear:hover{border-color:var(--danger);color:var(--danger)}
</style>
