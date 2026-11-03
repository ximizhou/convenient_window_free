<script lang="ts">
  import { onMount, tick } from "svelte";
  import { fly } from "svelte/transition";

  export let options: { label: string; group: string }[];
  export let value: number;
  export let onSelect: (index: number) => void;

  let root: HTMLDivElement;
  let trigger: HTMLButtonElement;
  let menu: HTMLDivElement | null = null;
  let open = false;
  let activeIndex = value;
  let menuStyle = "";
  let selected: { label: string; group: string };
  let reducedMotion = false;

  onMount(() => {
    const preference = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => { reducedMotion = preference.matches; };
    update();
    preference.addEventListener("change", update);
    return () => preference.removeEventListener("change", update);
  });

  $: if (!open) activeIndex = value;
  $: selected = options[value] ?? options[0];

  onMount(() => {
    const closeOutside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (open && !root.contains(target) && !menu?.contains(target)) open = false;
    };
    const reposition = () => { if (open) positionMenu(); };
    document.addEventListener("pointerdown", closeOutside);
    window.addEventListener("resize", reposition);
    window.addEventListener("scroll", reposition, true);
    return () => {
      document.removeEventListener("pointerdown", closeOutside);
      window.removeEventListener("resize", reposition);
      window.removeEventListener("scroll", reposition, true);
    };
  });

  function portal(node: HTMLElement) {
    document.body.appendChild(node);
    return { destroy: () => node.remove() };
  }

  async function openMenu(): Promise<void> {
    open = true;
    activeIndex = value;
    await tick();
    positionMenu();
    menu?.querySelector<HTMLElement>(`#action-option-${activeIndex}`)?.scrollIntoView({ block: "nearest" });
  }

  function positionMenu(): void {
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const below = window.innerHeight - rect.bottom - 9;
    const above = rect.top - 9;
    const openUp = below < 210 && above > below;
    const maxHeight = Math.max(150, Math.min(320, openUp ? above : below));
    const top = openUp ? Math.max(8, rect.top - maxHeight - 6) : rect.bottom + 6;
    menuStyle = `left:${rect.left}px;top:${top}px;width:${rect.width}px;max-height:${maxHeight}px`;
  }

  function choose(index: number): void {
    onSelect(index);
    open = false;
    trigger.focus();
  }

  function handleKey(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      if (open) { event.preventDefault(); open = false; }
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) { void openMenu(); return; }
      const offset = event.key === "ArrowDown" ? 1 : -1;
      activeIndex = (activeIndex + offset + options.length) % options.length;
      menu?.querySelector<HTMLElement>(`#action-option-${activeIndex}`)?.scrollIntoView({ block: "nearest" });
      return;
    }
    if ((event.key === "Enter" || event.key === " ") && open) {
      event.preventDefault();
      choose(activeIndex);
    }
  }
</script>

<div class="action-picker" bind:this={root}>
  <button
    aria-controls="action-picker-menu"
    aria-expanded={open}
    aria-haspopup="listbox"
    class:open
    class="picker-trigger"
    bind:this={trigger}
    on:click={() => open ? open = false : void openMenu()}
    on:keydown={handleKey}
    type="button"
  >
    <span><small>{selected.group}</small><strong>{selected.label}</strong></span><i aria-hidden="true"></i>
  </button>
</div>

{#if open}
  <div
    aria-activedescendant={`action-option-${activeIndex}`}
    class="picker-menu"
    id="action-picker-menu"
    bind:this={menu}
    in:fly={{ y: -5, duration: reducedMotion ? 0 : 150 }}
    role="listbox"
    style={menuStyle}
    tabindex="-1"
    use:portal
  >
    {#each options as option, index}
      {#if index === 0 || options[index - 1].group !== option.group}
        <span class="group-label">{option.group}</span>
      {/if}
      <button
        aria-selected={value === index}
        class:active={activeIndex === index}
        class:selected={value === index}
        id={`action-option-${index}`}
        on:click={() => choose(index)}
        on:mouseenter={() => { activeIndex = index; }}
        role="option"
        type="button"
      ><span>{option.label}</span>{#if value === index}<i>✓</i>{/if}</button>
    {/each}
  </div>
{/if}

<style>
  .action-picker{position:relative}
  .picker-trigger{width:100%;min-height:42px;padding:6px 12px;border:1px solid var(--line-strong);border-radius: var(--radius-xs);background:var(--bg);display:flex;align-items:center;justify-content:space-between;text-align:left;transition: border-color var(--dur-fast),box-shadow var(--dur-fast),background-color var(--dur-slow)}
  .picker-trigger:hover,.picker-trigger.open{border-color:var(--accent);box-shadow:0 0 0 3px var(--accent-bg)}
  .picker-trigger span{display:grid;gap:2px}.picker-trigger small{color: var(--faint-text);font: var(--fs-xs)/1.2 "Segoe UI Variable","Microsoft YaHei",sans-serif;letter-spacing:.04em}.picker-trigger strong{color:var(--ink);font: 500 var(--fs-md)/1.3 "Segoe UI Variable","Microsoft YaHei",sans-serif}
  .picker-trigger>i{width:8px;height:8px;border-right:1.5px solid var(--muted);border-bottom:1.5px solid var(--muted);transform:translate(-2px,-2px) rotate(45deg);transition: transform var(--dur-base)}.picker-trigger.open>i{transform:translate(-2px,2px) rotate(225deg)}
  .picker-menu{position:fixed;z-index:999;overflow:auto;padding:6px;background:var(--tooltip-bg);border:1px solid var(--line-strong);border-radius: var(--radius-sm);box-shadow:var(--shadow-menu);scrollbar-width:thin;scrollbar-color:var(--scrollbar) transparent}
  .group-label{display:block;padding:9px 9px 4px;color: var(--faint-text);font: 600 var(--fs-xs)/1.2 "Segoe UI Variable","Microsoft YaHei",sans-serif;letter-spacing:.08em}
  .picker-menu button{width:100%;min-height:30px;padding:6px 9px;border:0;border-radius: var(--radius-xs);background:transparent;color:var(--muted);display:flex;align-items:center;justify-content:space-between;text-align:left;font-size: var(--fs-sm);transition: background-color var(--dur-fast),color var(--dur-fast)}
  .picker-menu button:hover,.picker-menu button.active{background:var(--accent-bg);color:var(--ink)}.picker-menu button.selected{color:var(--accent-soft)}.picker-menu button i{color:var(--accent-soft);font-style:normal;font-size: var(--fs-sm)}
  @media(prefers-reduced-motion:reduce){.picker-trigger,.picker-trigger>i,.picker-menu button{transition-duration:.01ms}}
</style>
