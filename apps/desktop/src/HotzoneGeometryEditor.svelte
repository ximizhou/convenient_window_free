<script lang="ts">
  import type { DisplayInfo, HotzoneGeometry, HotzoneId, HotzoneSetting } from "./types";
  import { effectiveHotzoneGeometry, hotzonePreviewRect, resizeCornerGeometry } from "./hotzone-geometry";
  import { numberSetting } from "./number-setting";
  import { format, translator, type Language, type UiKey } from "./i18n";

  export let zone: HotzoneSetting;
  export let display: DisplayInfo | undefined;
  export let monitorId: string;
  export let edgeSize: number;
  export let ready = false;
  export let language: Language = "zh-CN";
  export let onChange: (geometry: HotzoneGeometry) => void;
  export let onReset: () => void;
  export let onSelectZone: (id: HotzoneId) => void;

  const names: Record<HotzoneId, UiKey> = {
    "top-left": "geometryTopLeft", top: "geometryTop", "top-right": "geometryTopRight", right: "geometryRight",
    "bottom-right": "geometryBottomRight", bottom: "geometryBottom", "bottom-left": "geometryBottomLeft", left: "geometryLeft"
  };
  $: ui = translator(language);
  $: geometry = effectiveHotzoneGeometry(zone, edgeSize);
  // Preserve controls; readiness only invalidates numeric drafts, never their DOM identity.
  $: context = `${monitorId}:${zone.id}`;
  $: fieldContext = `${ready}:${context}`;
  $: bounds = display?.bounds;
  $: width = bounds ? Math.max(1, bounds.right - bounds.left) : 1920;
  $: height = bounds ? Math.max(1, bounds.bottom - bounds.top) : 1080;
  $: rect = bounds ? hotzonePreviewRect(zone.id, bounds, edgeSize, zone.geometry) : null;
  $: range = rect ? `${rect.right - rect.left} × ${rect.bottom - rect.top} px` : "—";

  function cancelDraft(): void {
    const focused = document.activeElement;
    if (focused instanceof HTMLInputElement && focused.type === "number" && focused.closest(".hotzone-geometry")) {
      // Discard before native select/reset focus causes numberSetting's blur clamp.
      focused.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    }
  }
  function publish(next: HotzoneGeometry): void {
    if (ready && display?.id === monitorId) onChange({ ...next });
  }
  function resize(field: "width" | "height", value: number): void {
    if (geometry.kind === "corner") publish(resizeCornerGeometry(geometry, field, value));
  }
  function setLength(value: number): void {
    if (geometry.kind === "edge") publish({ ...geometry, lengthPercent: value });
  }
</script>

<section class="hotzone-geometry" aria-label={ui("hotzoneGeometry")} inert={!ready} data-context={context}>
  <header class="geometry-head">
    <select class="geometry-zone-select" aria-label={ui("geometrySelectArea")} value={zone.id} on:pointerdown={cancelDraft} on:change={(event) => { if (ready && display?.id === monitorId) onSelectZone(event.currentTarget.value as HotzoneId); }}>
      {#each Object.entries(names) as [id, name]}<option value={id}>{ui(name)}</option>{/each}
    </select>
    <button class="geometry-reset" type="button" on:pointerdown={cancelDraft} on:click={() => { if (ready && display?.id === monitorId) onReset(); }} title={ui("geometryResetDescription")}>{ui("geometryReset")}</button>
  </header>
  {#if geometry.kind === "corner"}
    <div class="geometry-corner-fields">
      <label class="geometry-field"><span>{ui("geometryWidth")}</span><div><input aria-label={ui("geometryWidth")} type="number" min="2" max="128" step="1" use:numberSetting={{ key: `${fieldContext}:width`, value: geometry.width, onChange: (value) => resize("width", value) }} /><em>px</em></div></label>
      <button class:linked={geometry.linked} class="geometry-link" aria-label={ui("geometryLink")} aria-pressed={geometry.linked} title={ui(geometry.linked ? "geometryLinked" : "geometryUnlinked")} type="button" on:click={() => { if (geometry.kind === "corner") publish({ ...geometry, linked: !geometry.linked }); }}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" aria-hidden="true"><path d="m10 14 4-4M8.6 15.4l-1.2 1.2a3.5 3.5 0 0 1-5-5l4-4a3.5 3.5 0 0 1 5 0m1.2 1.8 1.2-1.2a3.5 3.5 0 0 1 5 5l-4 4a3.5 3.5 0 0 1-5 0" />{#if !geometry.linked}<path d="M3 3 21 21" />{/if}</svg>
      </button>
      <label class="geometry-field"><span>{ui("geometryHeight")}</span><div><input aria-label={ui("geometryHeight")} type="number" min="2" max="128" step="1" use:numberSetting={{ key: `${fieldContext}:height`, value: geometry.height, onChange: (value) => resize("height", value) }} /><em>px</em></div></label>
    </div>
    <p class="geometry-note">{ui(geometry.linked ? "geometryLinked" : "geometryUnlinked")}</p>
  {:else}
    <div class="geometry-edge-fields">
      <label class="geometry-field"><span>{ui("geometryThickness")}</span><div><input aria-label={ui("geometryThickness")} type="number" min="2" max="48" step="1" use:numberSetting={{ key: `${fieldContext}:thickness`, value: geometry.thickness, onChange: (value) => { if (geometry.kind === "edge") publish({ ...geometry, thickness: value }); } }} /><em>px</em></div></label>
      <label class="geometry-field"><span>{ui("geometryLength")}</span><div><input aria-label={ui("geometryLength")} type="number" min="10" max="100" step="1" use:numberSetting={{ key: `${fieldContext}:length`, value: geometry.lengthPercent, onChange: setLength }} /><em>%</em></div></label>
    </div>
    <input class="geometry-slider" aria-label={ui("geometryLengthSlider")} type="range" min="10" max="100" step="1" value={geometry.lengthPercent} style={`--fill:${(geometry.lengthPercent - 10) / 90 * 100}%`} on:input={(event) => setLength(event.currentTarget.valueAsNumber)} />
    <div class="geometry-presets" role="group" aria-label={ui("geometryPresets")}>
      {#each [{ value: 20, label: "geometryShort" }, { value: 40, label: "geometryStandard" }, { value: 100, label: "geometryFull" }] as preset}
        <button aria-pressed={geometry.lengthPercent === preset.value} class:active={geometry.lengthPercent === preset.value} type="button" on:click={() => setLength(preset.value)}>{ui(preset.label as UiKey)} <span>{preset.value}%</span></button>
      {/each}
    </div>
  {/if}
  <div class="geometry-preview">
    <svg class="geometry-mini-screen" viewBox={`0 0 ${width} ${height}`} aria-hidden="true">
      <rect class="geometry-mini-bounds" x="0" y="0" width={width} height={height} />
      {#if rect && bounds}<rect class="geometry-mini-footprint" x={rect.left - bounds.left} y={rect.top - bounds.top} width={rect.right - rect.left} height={rect.bottom - rect.top} />{/if}
    </svg>
    <div class="geometry-readout"><span>{ui("geometryRange")}</span><output class="geometry-range">{range}</output><small>{geometry.kind === "edge" ? format(ui("geometryCentered"), { percent: geometry.lengthPercent }) : ui("geometryCornerAnchor")}</small></div>
  </div>
  <p class="geometry-note geometry-scope">{ui("geometryScope")}</p>
  {#if geometry.kind === "edge" && geometry.lengthPercent === 100}<p class="geometry-priority">{ui("geometryCornerPriority")}</p>{/if}
</section>
