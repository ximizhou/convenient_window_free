<script lang="ts">
  import type { ModifierKey } from "./types";

  export let value: ModifierKey[] = [];
  export let onChange: (value: ModifierKey[]) => void;
  export let label = "录制组合键";
  export let english = false;

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
    onChange([]);
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
      <span class="empty">{recording ? (english ? "Press keys" : "请按组合键") : (english ? "Direct" : "直接触发")}</span>
    {/if}
    {#if recording}<i aria-hidden="true"></i>{/if}
  </button>
  {#if value.length}
    <button aria-label={english ? "Clear modifiers" : "清除修饰键"} class="clear" on:click={clear} title={english ? "Clear modifiers" : "清除修饰键"} type="button">×</button>
  {/if}
</div>

<style>
  .modifier-recorder{min-width:0;display:grid;grid-template-columns:minmax(0,1fr) 30px;gap:6px;align-items:center}.modifier-recorder:not(:has(.clear)){grid-template-columns:1fr}
  .modifier-recorder>button:first-child{height:38px;min-width:0;padding:5px 9px;border:1px solid var(--line-strong);border-radius:6px;background:var(--bg);color:var(--ink);display:flex;align-items:center;justify-content:space-between;transition:border-color .15s,box-shadow .15s,background-color .2s}
  .modifier-recorder>button:first-child:hover,.modifier-recorder.recording>button:first-child{border-color:var(--accent);box-shadow:0 0 0 3px var(--accent-bg)}
  .key-row{min-width:0;display:flex;gap:4px;overflow:hidden}.key-row kbd{min-width:34px;height:24px;padding:0 6px;border:1px solid var(--line-strong);border-bottom-width:2px;border-radius:4px;background:var(--surface);color:var(--ink);font:600 11px/21px "Segoe UI Variable","Microsoft YaHei",sans-serif;text-align:center}
  .empty{color:var(--muted);font-size:12px}.modifier-recorder i{width:7px;height:7px;border-radius:50%;background:var(--accent);box-shadow:0 0 0 3px var(--accent-bg)}
  .clear{width:30px;height:30px;padding:0;border:1px solid var(--line);border-radius:5px;background:transparent;color:var(--muted);font-size:18px;line-height:1}.clear:hover{border-color:var(--danger);color:var(--danger)}
</style>
