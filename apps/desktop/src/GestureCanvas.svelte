<script lang="ts">
  import type { GesturePoint } from "./types";

  export let sample: GesturePoint[] = [];
  export let recording = true;
  export let onRecord: (sample: GesturePoint[]) => void = () => {};

  let drawing: GesturePoint[] = [];

  function pointFromEvent(event: PointerEvent): GesturePoint {
    const rect = (event.currentTarget as SVGSVGElement).getBoundingClientRect();
    return {
      x: Math.min(1, Math.max(0, (event.clientX - rect.left) / Math.max(1, rect.width))),
      y: Math.min(1, Math.max(0, (event.clientY - rect.top) / Math.max(1, rect.height)))
    };
  }

  function pointerDown(event: PointerEvent): void {
    if (!recording || event.button !== 0) return;
    (event.currentTarget as SVGSVGElement).setPointerCapture(event.pointerId);
    drawing = [pointFromEvent(event)];
  }

  function pointerMove(event: PointerEvent): void {
    if (!drawing.length || !(event.currentTarget as SVGSVGElement).hasPointerCapture(event.pointerId)) return;
    const point = pointFromEvent(event);
    const previous = drawing[drawing.length - 1];
    if (Math.hypot(point.x - previous.x, point.y - previous.y) >= 0.008) drawing = [...drawing, point];
  }

  function pointerUp(event: PointerEvent): void {
    if (!drawing.length) return;
    const svg = event.currentTarget as SVGSVGElement;
    if (svg.hasPointerCapture(event.pointerId)) svg.releasePointerCapture(event.pointerId);
    const completed = [...drawing, pointFromEvent(event)];
    drawing = [];
    if (completed.length >= 3) onRecord(completed);
  }

  function path(points: GesturePoint[]): string {
    return points.map((point, index) => `${index ? "L" : "M"} ${point.x * 100} ${point.y * 100}`).join(" ");
  }
</script>

<svg
  class:recording
  aria-label={recording ? "按住鼠标左键录制单笔手势" : "手势轨迹预览"}
  on:pointercancel={pointerUp}
  on:pointerdown={pointerDown}
  on:pointermove={pointerMove}
  on:pointerup={pointerUp}
  role="img"
  viewBox="0 0 100 100"
>
  <path class="guide" d="M 12 50 H 88 M 50 12 V 88" />
  {#if sample.length}<path class="saved" d={path(sample)} />{/if}
  {#if drawing.length}<path class="live" d={path(drawing)} />{/if}
  {#if !sample.length && !drawing.length}
    <text x="50" y="47">按住左键</text><text class="small" x="50" y="57">画一个单笔图案</text>
  {/if}
</svg>

<style>
  svg { width: min(100%, var(--gesture-canvas-height, 190px)); height: auto; aspect-ratio: 1; margin-inline: auto; box-sizing: border-box; display: block; overflow: visible; border: 1px solid var(--line-strong); border-radius: 8px; background: radial-gradient(circle at 50% 50%, var(--accent-bg), transparent 58%), var(--bg); touch-action: none; user-select: none; transition: background-color .32s, border-color .32s; }
  svg.recording { cursor: crosshair; }
  .guide { fill: none; stroke: var(--line-strong); stroke-width: .45; }
  .saved { fill: none; stroke: var(--accent-soft); stroke-width: 2.2; stroke-linecap: round; stroke-linejoin: round; }
  .live { fill: none; stroke: var(--green); stroke-width: 2.8; stroke-linecap: round; stroke-linejoin: round; filter: drop-shadow(0 0 3px rgba(62,207,142,.45)); }
  text { fill: var(--muted); font: 6px "Microsoft YaHei", sans-serif; text-anchor: middle; }
  text.small { fill: var(--faint); font-size: 4px; }
</style>
