<script lang="ts">
  import NavGlyph from "./NavGlyph.svelte";
  import { format, type UiKey } from "./i18n";
  import { MAX_ACTIVE_NAVIGATION as MAX_ACTIVE, type NavigationFeatureId, type NavigationPreferences } from "./navigation-preferences";

  interface Props {
    preferences: NavigationPreferences;
    label: (id: NavigationFeatureId) => string;
    description: (id: NavigationFeatureId) => string;
    ui: (key: UiKey) => string;
    onChange: (value: NavigationPreferences) => void;
    onOpen: (id: NavigationFeatureId) => void;
  }

  let { preferences, label, description, ui, onChange, onOpen }: Props = $props();

  type ComposerRow = "active" | "hidden";
  type OptionalNavigationFeatureId = Exclude<NavigationFeatureId, "power">;
  type RowSet = Record<ComposerRow, OptionalNavigationFeatureId[]>;
  type Point = { x: number; y: number };

  const ROW_HEIGHT = 54;
  const ROW_CAPTION = 16;
  const ROW_GROUP_GAP = 8;
  const SIZING_SLOTS = 5;
  const DRAG_THRESHOLD = 4;
  const REJECT_HOLD_MS = 720;

  let stage = $state<HTMLDivElement>();
  let clientWidth = $state(0);
  let pointerId = $state<number | null>(null);
  let pointerSource = $state<OptionalNavigationFeatureId | null>(null);
  let pointerStart: Point = { x: 0, y: 0 };
  let pointerPoint = $state<Point>({ x: 0, y: 0 });
  let pointerInside = $state(false);
  let dragging = $state(false);
  let previewRows = $state<RowSet>({ active: [], hidden: [] });
  let previewRow = $state<ComposerRow | null>(null);
  let previewIndex = $state(-1);
  let rejectRow = $state<ComposerRow | null>(null);
  let suppressClick = false;
  let rejectTimer: number | undefined;
  let measured = $derived(clientWidth > 0);
  let optionalOrder = $derived(preferences.order.filter(isOptionalFeature));
  let activeItems = $derived(optionalOrder.filter((id) => preferences.visible.includes(id)));
  let hiddenItems = $derived(optionalOrder.filter((id) => !preferences.visible.includes(id)));
  let settledRows = $derived<RowSet>({ active: activeItems, hidden: hiddenItems });
  let renderedRows = $derived(dragging ? previewRows : settledRows);
  let layoutWidth = $derived(Math.max(clientWidth, 220));
  let bubbleSize = $derived(Math.min(46, Math.max(36, (layoutWidth - 32) / SIZING_SLOTS)));
  let slotGap = $derived(Math.min(10, Math.max(5, (layoutWidth - bubbleSize * SIZING_SLOTS - 22) / (SIZING_SLOTS - 1))));
  let stageHeight = $derived(ROW_CAPTION * 2 + ROW_HEIGHT * 2 + ROW_GROUP_GAP);
  let activeFull = $derived(activeItems.length >= MAX_ACTIVE);
  let rejected = $derived(rejectRow === "active");
  let positionedItems = $derived(optionalOrder.map((id) => {
    const row = rowFor(id, renderedRows);
    const slot = renderedRows[row].indexOf(id);
    const position = slotPosition(row, slot, renderedRows[row].length);
    return { id, row, x: position.x, y: position.y };
  }));
  let placeholderPosition = $derived(previewRow && previewIndex >= 0
    ? slotPosition(previewRow, previewIndex, renderedRows[previewRow].length)
    : null);

  $effect(() => () => {
    window.clearTimeout(rejectTimer);
  });

  function isOptionalFeature(id: NavigationFeatureId): id is OptionalNavigationFeatureId {
    return id !== "power";
  }

  function rowFor(id: OptionalNavigationFeatureId, rows: RowSet): ComposerRow {
    return rows.active.includes(id) ? "active" : "hidden";
  }

  function rowTop(row: ComposerRow): number {
    return row === "active" ? ROW_CAPTION : ROW_CAPTION + ROW_HEIGHT + ROW_GROUP_GAP + ROW_CAPTION;
  }

  function slotPosition(row: ComposerRow, slot: number, count: number): Point {
    const safeCount = Math.max(count, 1);
    const contentWidth = safeCount * bubbleSize + Math.max(0, safeCount - 1) * slotGap;
    const start = Math.max(0, (layoutWidth - contentWidth) / 2);
    return { x: start + Math.max(0, slot) * (bubbleSize + slotGap), y: rowTop(row) + (ROW_HEIGHT - bubbleSize) / 2 };
  }

  function rowAt(point: Point): ComposerRow | null {
    if (point.x < -8 || point.x > layoutWidth + 8) return null;
    const activeTop = ROW_CAPTION;
    const hiddenTop = rowTop("hidden");
    if (point.y >= activeTop - 5 && point.y <= activeTop + ROW_HEIGHT + 5) return "active";
    if (point.y >= hiddenTop - 5 && point.y <= hiddenTop + ROW_HEIGHT + 5) return "hidden";
    return null;
  }

  function cloneRows(rows: RowSet): RowSet {
    return { active: [...rows.active], hidden: [...rows.hidden] };
  }

  function rowsWithoutSource(source: OptionalNavigationFeatureId): RowSet {
    return {
      active: activeItems.filter((id) => id !== source),
      hidden: hiddenItems.filter((id) => id !== source)
    };
  }

  function insertionIndex(row: ComposerRow, x: number, baseCount: number): number {
    const slotCount = baseCount + 1;
    const first = slotPosition(row, 0, slotCount).x;
    const step = bubbleSize + slotGap;
    return Math.max(0, Math.min(baseCount, Math.floor((x - first + step / 2) / step)));
  }

  function updatePreview(): void {
    const source = pointerSource;
    if (!source) return;
    const row = rowAt(pointerPoint);
    if (!row) {
      pointerInside = false;
      rejectRow = null;
      previewRows = cloneRows(settledRows);
      previewRow = null;
      previewIndex = -1;
      return;
    }
    pointerInside = true;
    const base = rowsWithoutSource(source);
    const sourceInActive = activeItems.includes(source);
    if (row === "active" && !sourceInActive && base.active.length >= MAX_ACTIVE) {
      previewRows = cloneRows(settledRows);
      previewRow = null;
      previewIndex = -1;
      rejectRow = "active";
      return;
    }
    rejectRow = null;
    const index = insertionIndex(row, pointerPoint.x, base[row].length);
    const next = { active: [...base.active], hidden: [...base.hidden] };
    next[row] = [...next[row].slice(0, index), source, ...next[row].slice(index)];
    previewRows = next;
    previewRow = row;
    previewIndex = index;
  }

  function flashReject(row: ComposerRow): void {
    rejectRow = row;
    window.clearTimeout(rejectTimer);
    rejectTimer = window.setTimeout(() => {
      if (pointerId === null) rejectRow = null;
    }, REJECT_HOLD_MS);
  }

  function beginPointer(event: PointerEvent, id: OptionalNavigationFeatureId): void {
    if (event.button !== 0 || pointerId !== null) return;
    pointerId = event.pointerId;
    pointerSource = id;
    pointerStart = { x: event.clientX, y: event.clientY };
    pointerPoint = pointerStart;
    pointerInside = true;
    dragging = false;
    rejectRow = null;
    window.clearTimeout(rejectTimer);
    previewRows = cloneRows(settledRows);
    const initialRow = rowFor(id, previewRows);
    previewRow = initialRow;
    previewIndex = previewRows[initialRow].indexOf(id);
    (event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId);
  }

  function movePointer(event: PointerEvent): void {
    if (pointerId !== event.pointerId || !pointerSource) return;
    const moved = Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y);
    if (!dragging && moved < DRAG_THRESHOLD) return;
    dragging = true;
    suppressClick = true;
    event.preventDefault();
    const bounds = stage?.getBoundingClientRect();
    if (!bounds) return;
    pointerPoint = { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
    updatePreview();
  }

  function buildPreferences(rows: RowSet, source: OptionalNavigationFeatureId | null): NavigationPreferences {
    const visible = new Set(preferences.visible.filter((id) => id !== source));
    if (source && rows.active.includes(source)) visible.add(source);
    return {
      order: ["power", ...rows.active, ...rows.hidden],
      visible: ["power", ...optionalOrder.filter((id) => visible.has(id))]
    };
  }

  function finishPointer(event: PointerEvent): void {
    if (pointerId !== event.pointerId) return;
    const source = pointerSource;
    const rows = previewRows;
    const shouldCommit = dragging && pointerInside && source !== null && !rejected;
    const wasRejected = rejected;
    (event.currentTarget as HTMLElement).releasePointerCapture?.(event.pointerId);
    pointerId = null;
    pointerSource = null;
    pointerInside = false;
    dragging = false;
    previewRow = null;
    previewIndex = -1;
    if (shouldCommit) {
      rejectRow = null;
      onChange(buildPreferences(rows, source));
    } else if (wasRejected) {
      flashReject("active");
    } else {
      rejectRow = null;
    }
    window.setTimeout(() => { suppressClick = false; }, 0);
  }

  function cancelPointer(): void {
    if (pointerId === null) return;
    window.clearTimeout(rejectTimer);
    pointerId = null;
    pointerSource = null;
    pointerInside = false;
    dragging = false;
    suppressClick = false;
    previewRows = { active: [], hidden: [] };
    previewRow = null;
    previewIndex = -1;
    rejectRow = null;
  }

  function moveKeyboard(id: OptionalNavigationFeatureId, direction: "left" | "right" | "up" | "down"): void {
    const rows = cloneRows(settledRows);
    const row = rowFor(id, rows);
    const sourceIndex = rows[row].indexOf(id);
    if (sourceIndex < 0) return;
    if (direction === "left" || direction === "right") {
      const next = sourceIndex + (direction === "left" ? -1 : 1);
      if (next < 0 || next >= rows[row].length) return;
      [rows[row][sourceIndex], rows[row][next]] = [rows[row][next], rows[row][sourceIndex]];
    } else {
      const destination: ComposerRow = direction === "up" ? "active" : "hidden";
      if (destination === row) return;
      if (destination === "active" && rows.active.length >= MAX_ACTIVE) {
        flashReject("active");
        return;
      }
      rows[row].splice(sourceIndex, 1);
      rows[destination].splice(Math.min(sourceIndex, rows[destination].length), 0, id);
    }
    onChange(buildPreferences(rows, id));
  }

  function handleBubbleKeydown(event: KeyboardEvent, id: OptionalNavigationFeatureId): void {
    if (event.key === " ") {
      event.preventDefault();
      const row = rowFor(id, settledRows);
      moveKeyboard(id, row === "active" ? "down" : "up");
      return;
    }
    if (!event.altKey) return;
    const direction = event.key === "ArrowLeft" ? "left" : event.key === "ArrowRight" ? "right" : event.key === "ArrowUp" ? "up" : event.key === "ArrowDown" ? "down" : null;
    if (!direction) return;
    event.preventDefault();
    moveKeyboard(id, direction);
  }

  function openBubble(id: OptionalNavigationFeatureId): void {
    if (suppressClick) return;
    onOpen(id);
  }

  function handleWindowKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && pointerId !== null) {
      event.preventDefault();
      cancelPointer();
    }
  }

  function bubbleStyle(item: { id: OptionalNavigationFeatureId; row: ComposerRow; x: number; y: number }): string {
    const isDragging = dragging && item.id === pointerSource;
    const x = isDragging ? pointerPoint.x - bubbleSize / 2 : item.x;
    const y = isDragging ? pointerPoint.y - bubbleSize / 2 : item.y;
    const tooltipWidth = Math.min(220, layoutWidth);
    const tooltipCenter = Math.max(tooltipWidth / 2, Math.min(layoutWidth - tooltipWidth / 2, x + bubbleSize / 2));
    // Keep hidden and visible tooltips inside the stage; the button has a 1px border.
    return `transform: translate3d(${x}px, ${y}px, 0); --tooltip-left:${tooltipCenter - x - 1}px; --tooltip-width:${tooltipWidth}px;`;
  }

  function placeholderStyle(): string {
    if (!placeholderPosition) return "display: none;";
    return `transform: translate3d(${placeholderPosition.x}px, ${placeholderPosition.y}px, 0);`;
  }

  function capacityLabel(): string {
    return format(ui("navigationCapacityAria"), { used: activeItems.length, max: MAX_ACTIVE });
  }

  function rowLimitLabel(): string {
    return rejected ? format(ui("navigationRowFull"), { count: MAX_ACTIVE }) : format(ui("navigationRowLimit"), { count: MAX_ACTIVE });
  }
</script>

<svelte:window onpointermove={movePointer} onpointerup={finishPointer} onpointercancel={cancelPointer} onblur={cancelPointer} onkeydown={handleWindowKeydown} />

<section class="navigation-composer" aria-label={ui("navigationSettings")}>
  <header class="composer-head">
    <div class="composer-head-copy">
      <span>{ui("navigationSettings")}</span>
      <p>{format(ui("navigationSettingsDescription"), { count: MAX_ACTIVE })}</p>
    </div>
    <span aria-label={capacityLabel()} class:full={activeFull} class="composer-capacity"><b>{activeItems.length}</b><i>/{MAX_ACTIVE}</i></span>
  </header>

  <div class:measured class:dragging class="composer-stage" bind:this={stage} bind:clientWidth style={`--row-height:${ROW_HEIGHT}px; --row-caption:${ROW_CAPTION}px; --row-gap:${ROW_GROUP_GAP}px; --bubble-size:${bubbleSize}px; --stage-height:${stageHeight}px;`}>
    <div class="composer-layer" style="--layer-top: 0px;">
      <p class="composer-caption">
        <span>{ui("navigationActiveRow")}</span>
        <em class:alert={rejected}>{rowLimitLabel()}</em>
      </p>
      <div class:rejected class="composer-tray composer-tray-active composer-row-active" role="group" aria-label={capacityLabel()}>
        {#if renderedRows.active.length === 0 && !dragging}<span class="composer-empty">{ui("navigationDropHere")}</span>{/if}
      </div>
    </div>

    <div class="composer-layer composer-layer-hidden" style={`--layer-top:${ROW_CAPTION + ROW_HEIGHT + ROW_GROUP_GAP}px;`}>
      <p class="composer-caption">
        <span>{ui("navigationInactiveRow")}</span>
        <em>{ui("navigationHiddenHint")}</em>
      </p>
      <div class="composer-tray composer-tray-hidden composer-row-hidden" role="group" aria-label={ui("navigationInactiveRow")}>
        {#if renderedRows.hidden.length === 0 && !dragging}<span class="composer-empty">{ui("navigationDropHere")}</span>{/if}
      </div>
    </div>

    {#if dragging && previewRow}
      <div aria-hidden="true" class="composer-placeholder" style={placeholderStyle()}></div>
    {/if}

    {#each positionedItems as item (item.id)}
      <div
        class:dragging={dragging && pointerSource === item.id}
        class:dim={item.row === "hidden"}
        class:rejected={rejected && dragging && pointerSource === item.id}
        class="composer-bubble"
        data-feature-id={item.id}
        style={bubbleStyle(item)}
      >
        <button
          aria-describedby={`navigation-composer-description-${item.id}`}
          aria-label={label(item.id)}
          class="composer-bubble-button"
          onclick={() => openBubble(item.id)}
          onkeydown={(event) => handleBubbleKeydown(event, item.id)}
          onpointerdown={(event) => beginPointer(event, item.id)}
          type="button"
        >
          <NavGlyph id={item.id} />
          <span class="composer-tooltip" role="tooltip"><strong>{label(item.id)}</strong><small id={`navigation-composer-description-${item.id}`}>{description(item.id)}</small></span>
        </button>
      </div>
    {/each}
  </div>
  <p aria-live="polite" class="composer-status">{rejected ? format(ui("navigationRejectedAria"), { count: MAX_ACTIVE }) : ui("navigationLimit")}</p>
</section>

<style>
  .navigation-composer { display: grid; gap: 9px; width: 100%; margin-bottom: 10px; color: var(--ink); }
  .composer-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; }
  .composer-head-copy > span { display: block; color: var(--ink); font-size: var(--fs-lg); font-weight: 620; letter-spacing: -.01em; }
  .composer-head-copy > p { margin: 5px 0 0; color: var(--muted); font-size: var(--fs-sm); line-height: 1.55; }
  .composer-capacity { display: inline-flex; flex: none; align-items: baseline; gap: 1px; padding: 3px 9px 4px; border: 1px solid var(--line); border-radius: 999px; background: color-mix(in srgb, var(--panel) 78%, var(--surface)); color: var(--faint-text); font-size: var(--fs-2xs); font-variant-numeric: tabular-nums; transition: border-color var(--dur-base) ease, background var(--dur-base) ease; }
  .composer-capacity b { color: var(--accent-soft); font-size: var(--fs-sm); font-weight: 660; }
  .composer-capacity i { font-style: normal; opacity: .72; }
  .composer-capacity.full { border-color: color-mix(in srgb, var(--accent-line) 62%, var(--line)); background: color-mix(in srgb, var(--accent-bg) 55%, var(--panel)); }

  .composer-stage { position: relative; width: 100%; height: var(--stage-height); min-height: var(--stage-height); }
  .composer-stage:has(.composer-bubble-button:hover),
  .composer-stage:has(.composer-bubble-button:focus-visible) { z-index: 30; }
  .composer-layer { position: absolute; top: var(--layer-top); left: 0; right: 0; }
  .composer-caption { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; height: var(--row-caption); margin: 0; }
  .composer-caption > span { color: var(--faint-text); font-size: var(--fs-3xs); font-weight: 650; letter-spacing: .08em; text-transform: uppercase; }
  .composer-caption > em { color: var(--faint-text); font-size: var(--fs-3xs); font-style: normal; letter-spacing: .01em; white-space: nowrap; transition: color var(--dur-fast) ease; }
  .composer-caption > em.alert { color: var(--danger); font-weight: 620; }

  .composer-tray { position: relative; display: flex; align-items: center; justify-content: center; height: var(--row-height); border-radius: 18px; transition: border-color var(--dur-base) ease, background var(--dur-base) ease, box-shadow var(--dur-mid) ease; }
  .composer-tray-active { border: 1px solid color-mix(in srgb, var(--accent-line) 46%, var(--line)); background: linear-gradient(180deg, color-mix(in srgb, var(--accent-bg) 74%, var(--surface)), color-mix(in srgb, var(--accent-bg) 26%, var(--surface))); box-shadow: inset 0 1px 0 rgba(255, 255, 255, .32), 0 1px 2px rgba(15, 23, 42, .04); }
  .composer-tray-hidden { border: 1px dashed color-mix(in srgb, var(--line-strong) 62%, transparent); background: color-mix(in srgb, var(--surface) 58%, transparent); }
  .composer-tray-active.rejected { border-color: color-mix(in srgb, var(--danger) 64%, transparent); background: linear-gradient(180deg, color-mix(in srgb, var(--danger-bg) 88%, var(--surface)), color-mix(in srgb, var(--danger-bg) 30%, var(--surface))); box-shadow: 0 0 0 1px color-mix(in srgb, var(--danger) 24%, transparent), 0 0 20px color-mix(in srgb, var(--danger) 24%, transparent), inset 0 1px 0 rgba(255, 255, 255, .28); animation: composer-reject-pulse 640ms var(--ease-out); }
  .composer-empty { color: var(--faint-text); font-size: var(--fs-xs); letter-spacing: .02em; pointer-events: none; }

  .composer-bubble, .composer-placeholder { position: absolute; top: 0; left: 0; width: var(--bubble-size); height: var(--bubble-size); border-radius: 50%; }
  .composer-bubble { z-index: 2; }
  /* transform 已让每个气泡形成独立层叠上下文；必须提升整个悬停/聚焦气泡，
     仅提高内部 tooltip 的 z-index 无法越过设置面板的后续内容。 */
  .composer-bubble:has(.composer-bubble-button:hover),
  .composer-bubble:has(.composer-bubble-button:focus-visible) { z-index: 30; }
  .composer-stage.measured .composer-bubble { transition: transform var(--dur-mid) var(--ease-out), opacity var(--dur-base) ease; }
  .composer-stage.measured .composer-bubble.dragging { z-index: 8; opacity: 1; transition: none; }
  .composer-bubble.dragging .composer-bubble-button { background: linear-gradient(180deg, var(--chip-a), var(--chip-b)); --glyph-cutout: var(--chip-b); }
  .composer-bubble.dim { opacity: .6; }
  .composer-bubble.dim:hover { opacity: .92; }
  .composer-bubble-button {
    position: relative;
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    padding: 0;
    border: 1px solid color-mix(in srgb, var(--line-strong) 68%, transparent);
    border-radius: 50%;
    background: linear-gradient(180deg, var(--chip-a), var(--chip-b));
    color: var(--muted);
    --glyph-cutout: var(--chip-b);
    cursor: grab;
    touch-action: none;
    user-select: none;
    box-shadow: 0 1px 1px rgba(15, 23, 42, .05), 0 6px 14px rgba(15, 23, 42, .07), inset 0 1px 0 rgba(255, 255, 255, .5);
    transition: color var(--dur-fast) ease, border-color var(--dur-fast) ease, box-shadow var(--dur-base) ease, transform var(--dur-base) ease, background var(--dur-base) ease;
  }
  .composer-bubble-button:hover, .composer-bubble-button:focus-visible { color: var(--accent-soft); border-color: color-mix(in srgb, var(--accent-line) 72%, var(--line)); box-shadow: 0 8px 20px rgba(15, 23, 42, .12), 0 0 0 3px color-mix(in srgb, var(--accent-line) 20%, transparent), inset 0 1px 0 rgba(255, 255, 255, .55); outline: none; }
  .composer-bubble.dragging .composer-bubble-button { cursor: grabbing; color: var(--accent-soft); transform: scale(1.06); box-shadow: 0 18px 34px rgba(15, 23, 42, .22), 0 0 0 1px color-mix(in srgb, var(--accent-line) 42%, transparent), inset 0 1px 0 rgba(255, 255, 255, .55); }
  .composer-bubble.rejected .composer-bubble-button { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 58%, transparent); box-shadow: 0 18px 34px rgba(15, 23, 42, .2), 0 0 0 2px color-mix(in srgb, var(--danger) 52%, transparent), 0 0 24px color-mix(in srgb, var(--danger) 30%, transparent), inset 0 1px 0 rgba(255, 255, 255, .4); }
  .composer-bubble.dim .composer-bubble-button { background: linear-gradient(180deg, var(--chip-dim-a), var(--chip-dim-b)); box-shadow: 0 1px 1px rgba(15, 23, 42, .03), inset 0 1px 0 rgba(255, 255, 255, .3); --glyph-cutout: var(--chip-dim-b); }

  /* 图标与左下角底部页签共用同一份 NavGlyph；这里只按 46px 气泡等比放大，形状不变。 */
  .composer-bubble-button :global(.nav-icon) { width: 18px; height: 18px; }
  .composer-bubble-button :global(.nav-icon-svg) { width: 20px; height: 20px; }
  .composer-bubble-button :global(.nav-beauty) { width: 23px; height: 23px; }

  .composer-placeholder { z-index: 1; border: 1.5px dashed color-mix(in srgb, var(--accent) 68%, transparent); background: color-mix(in srgb, var(--accent-bg) 62%, transparent); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent-line) 46%, transparent), 0 0 0 4px color-mix(in srgb, var(--accent-line) 10%, transparent); pointer-events: none; transition: transform var(--dur-mid) var(--ease-out); }

  .composer-tooltip { position: absolute; top: calc(100% + 8px); left: var(--tooltip-left); z-index: 30; display: grid; width: max-content; max-width: var(--tooltip-width); gap: 3px; padding: 7px 9px; border: 1px solid var(--line-strong); border-radius: 11px; background: color-mix(in srgb, var(--panel) 95%, var(--raised)); box-shadow: 0 12px 28px rgba(15, 23, 42, .18), 0 1px 2px rgba(15, 23, 42, .08); color: var(--ink); text-align: left; opacity: 0; visibility: hidden; pointer-events: none; transform: translate(-50%, -4px) scale(.98); transform-origin: top center; transition: opacity var(--dur-fast) ease, transform var(--dur-fast) ease, visibility var(--dur-fast) ease; }
  .composer-tooltip strong { color: var(--ink); font-size: var(--fs-xs); font-weight: 650; white-space: nowrap; }
  .composer-tooltip small { color: var(--muted); font-size: var(--fs-3xs); line-height: 1.35; white-space: normal; }
  .composer-bubble-button:hover .composer-tooltip, .composer-bubble-button:focus-visible .composer-tooltip { opacity: 1; visibility: visible; transform: translate(-50%, 0) scale(1); }
  /* 拖拽时提示条会挡住落点和红色反馈，先收起。 */
  .composer-stage.dragging .composer-tooltip { opacity: 0; visibility: hidden; }

  .composer-status { margin: 0; color: var(--faint-text); font-size: var(--fs-xs); line-height: 1.5; transition: color var(--dur-fast) ease; }
  .composer-status::before { content: ""; display: inline-block; width: 5px; height: 5px; margin-right: 6px; border-radius: 50%; background: color-mix(in srgb, var(--faint) 70%, transparent); vertical-align: 1px; }

  @keyframes composer-reject-pulse {
    0% { box-shadow: 0 0 0 0 color-mix(in srgb, var(--danger) 0%, transparent), inset 0 1px 0 rgba(255, 255, 255, .28); }
    32% { box-shadow: 0 0 0 3px color-mix(in srgb, var(--danger) 30%, transparent), 0 0 26px color-mix(in srgb, var(--danger) 32%, transparent), inset 0 1px 0 rgba(255, 255, 255, .28); }
    100% { box-shadow: 0 0 0 1px color-mix(in srgb, var(--danger) 24%, transparent), 0 0 20px color-mix(in srgb, var(--danger) 22%, transparent), inset 0 1px 0 rgba(255, 255, 255, .28); }
  }

  @media (prefers-reduced-motion: reduce) {
    .composer-bubble, .composer-placeholder, .composer-tooltip, .composer-capacity, .composer-caption > em { transition-duration: 0s; }
    .composer-tray-active.rejected { animation-duration: 1ms; }
  }
</style>
