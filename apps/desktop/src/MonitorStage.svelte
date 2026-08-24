<script lang="ts">
  import type { DisplayInfo, Edge, HotzoneId, HotzoneSetting } from "./types";
  import { unavailableEdges } from "./monitor-topology";

  export let displays: DisplayInfo[];
  export let selectedDisplayId: string;
  export let mode: "power" | "hotzones" | "edge-hide" | "gestures" | "more" | null;
  export let english = false;
  export let selectedZone: HotzoneId;
  export let hotzonesEnabled: boolean;
  export let edgeHideEnabled = false;
  export let hotzones: HotzoneSetting[];
  export let edgeHideEdges: Edge[];
  export let onSelectDisplay: (id: string) => void;
  export let onSelectZone: (id: HotzoneId) => void;
  export let onToggleEdge: (edge: Edge) => void;

  const zones: HotzoneId[] = [
    "top-left", "top", "top-right", "right",
    "bottom-right", "bottom", "bottom-left", "left"
  ];
  const zoneLabels: Record<HotzoneId, string> = {
    "top-left": "左上角", top: "上边缘", "top-right": "右上角", right: "右边缘",
    "bottom-right": "右下角", bottom: "下边缘", "bottom-left": "左下角", left: "左边缘"
  };
  const edgeLabels: Record<Edge, string> = { left: "左", top: "上", right: "右", bottom: "下" };
  const zoneLabelsEn: Record<HotzoneId, string> = {
    "top-left": "Top left", top: "Top", "top-right": "Top right", right: "Right",
    "bottom-right": "Bottom right", bottom: "Bottom", "bottom-left": "Bottom left", left: "Left"
  };
  const edgeLabelsEn: Record<Edge, string> = { left: "L", top: "T", right: "R", bottom: "B" };
  const edgeNamesEn: Record<Edge, string> = { left: "Left", top: "Top", right: "Right", bottom: "Bottom" };
  function slotConfigured(slot: HotzoneSetting["actions"][number]): boolean {
    return slot.action.kind !== "none"
      || (slot.modifierActions ?? []).some((variant) => variant.action.kind !== "none");
  }

  $: triggerCounts = Object.fromEntries(zones.map((zone) => {
    const setting = hotzones.find((item) => item.id === zone);
    const count = hotzonesEnabled && setting
      ? setting.actions.filter(slotConfigured).length
      : 0;
    return [zone, count];
  })) as Record<HotzoneId, number>;

  function inactiveIndex(id: string): number {
    return displays.filter((display) => display.id !== selectedDisplayId).findIndex((display) => display.id === id);
  }

  function monitorStyle(display: DisplayInfo): string {
    if (display.id === selectedDisplayId) return "--slot:0";
    const slot = inactiveIndex(display.id);
    return `--slot:${slot};--tilt:${slot % 2 === 0 ? -1.2 : 1.1}deg`;
  }

  function displayLabel(display: DisplayInfo): string {
    return display.primary ? (english ? "Primary" : "主显示器") : `${display.bounds.right - display.bounds.left} × ${display.bounds.bottom - display.bounds.top}`;
  }

  function zoneLabel(zone: HotzoneId): string {
    return english ? zoneLabelsEn[zone] : zoneLabels[zone];
  }

  function edgeLabel(edge: Edge): string {
    return english ? edgeLabelsEn[edge] : edgeLabels[edge];
  }

  function edgeUnavailable(edge: Edge): boolean {
    const selected = displays.find((display) => display.id === selectedDisplayId);
    return selected ? unavailableEdges(selected, displays).includes(edge) : false;
  }

</script>

<div class:has-companions={displays.length > 1} class="monitor-stage">
  <div class="blueprint-grid" aria-hidden="true"></div>
  {#if displays.length > 1}
    <svg aria-hidden="true" class="focus-path" viewBox="0 0 560 330" preserveAspectRatio="none">
      <path d="M 105 100 C 165 92, 165 142, 230 150" />
      <path class="arrow" d="M 220 139 L 231 150 L 216 156" />
    </svg>
    <span class="swap-note">{english ? "Switch display" : "点击切换焦点"}</span>
  {/if}

  {#each displays as display, index (display.id)}
    <article
      class:active-monitor={display.id === selectedDisplayId}
      class:companion-monitor={display.id !== selectedDisplayId}
      class="physical-monitor"
      class:hotzone-layer={display.id === selectedDisplayId && mode === "hotzones"}
      style={monitorStyle(display)}
    >
      <button
        aria-label={`${english ? "Select display" : "选择显示器"} S${index + 1}`}
        class="screen"
        on:click={() => onSelectDisplay(display.id)}
        type="button"
      >
        <span class="screen-index">S{index + 1}</span>
        <strong>{display.id === selectedDisplayId ? `S${index + 1} · ${english ? "Current" : "当前"}` : `S${index + 1}`}</strong>
        <small>{displayLabel(display)}</small>
      </button>

      {#if display.id === selectedDisplayId && mode === "hotzones"}
        {#each zones as zone}
          <button
            aria-label={zoneLabel(zone)}
            class:active={selectedZone === zone}
            class:configured={hotzonesEnabled && hotzones.some((item) => item.id === zone && item.actions.some(slotConfigured))}
            class={`zone zone-${zone}`}
            on:click={() => onSelectZone(zone)}
            type="button"
          >
            {#if triggerCounts[zone] > 0}<em>{triggerCounts[zone]}</em>{/if}<span>{zoneLabel(zone)}</span>
          </button>
        {/each}
      {/if}

      {#if display.id === selectedDisplayId && mode === "edge-hide"}
        <div class="window-demo">
          <span>{english ? "Window" : "窗口"}</span>
          {#each Object.keys(edgeLabels) as edge}
            <button
              aria-label={`${english ? edgeNamesEn[edge as Edge] : edgeLabel(edge as Edge)}${english ? " edge hide" : "侧贴边隐藏"}`}
              aria-pressed={edgeHideEdges.includes(edge as Edge)}
              class:active={edgeHideEnabled && !edgeUnavailable(edge as Edge) && edgeHideEdges.includes(edge as Edge)}
              class:configured={!edgeUnavailable(edge as Edge) && edgeHideEdges.includes(edge as Edge)}
              class:unavailable={edgeUnavailable(edge as Edge)}
              class={`edge-${edge}`}
              disabled={edgeUnavailable(edge as Edge)}
              on:click={() => onToggleEdge(edge as Edge)}
              title={edgeUnavailable(edge as Edge) ? (english ? "Display seam; edge hide is unavailable" : "显示器拼接缝，不执行收缩") : `${english ? edgeNamesEn[edge as Edge] : edgeLabel(edge as Edge)}${english ? " edge hide" : "侧贴边隐藏"}`}
              type="button"
            >{edgeLabel(edge as Edge)}</button>
          {/each}
        </div>
      {/if}
      <span class="monitor-neck"></span>
      <span class="monitor-foot"></span>
    </article>
  {/each}

  <p class="stage-caption">
    {mode === "hotzones" ? (english ? "Choose a corner" : "点击边角定义动作") : mode === "edge-hide" ? (english ? "Outer edges work; seams do not" : "外轮廓可用，拼接缝已禁用") : displays.length > 1 ? (english ? "Choose one display" : "一次只专注设置一块屏幕") : (english ? "Choose a feature" : "选择下方功能开始设置")}
  </p>
</div>

<style>
  .monitor-stage{position:relative;min-height:0;height:100%;overflow:hidden;isolation:isolate}
  .blueprint-grid{position:absolute;inset:3% 1% 10%;opacity:.6;background-image:linear-gradient(var(--grid-line) 1px,transparent 1px),linear-gradient(90deg,var(--grid-line) 1px,transparent 1px);background-size:28px 28px;mask-image:radial-gradient(ellipse at center,#000 20%,transparent 76%)}
  .physical-monitor{--slot:0;position:absolute;left:14%;top:13%;width:70%;height:63%;transition:left .5s cubic-bezier(.22,1,.36,1),top .5s cubic-bezier(.22,1,.36,1),width .5s cubic-bezier(.22,1,.36,1),height .5s cubic-bezier(.22,1,.36,1),opacity .25s,filter .25s,transform .5s cubic-bezier(.22,1,.36,1);z-index:3}
  .physical-monitor.hotzone-layer{z-index:20}
  .has-companions .active-monitor{left:22%;width:68%}
  .screen{position:absolute;inset:0 0 17%;border:1px solid var(--line-strong);border-radius:10px;background:linear-gradient(160deg,var(--screen-a),var(--screen-b));color:var(--ink);padding:18px;text-align:left;box-shadow:0 16px 36px rgba(0,0,0,.18),inset 0 1px 0 rgba(255,255,255,.06);transition:border-color .18s,box-shadow .18s,background .32s}
  .screen:hover{border-color:var(--faint);background:linear-gradient(160deg,var(--screen-hover-a),var(--screen-hover-b))}
  .active-monitor .screen{border-color:var(--accent);box-shadow:0 16px 36px rgba(0,0,0,.18),0 0 0 3px var(--accent-bg),inset 0 1px 0 rgba(255,255,255,.06)}
  .screen-index{position:absolute;left:50%;top:44%;transform:translate(-50%,-50%);font-family:"Cascadia Mono",monospace;font-size:26px;color:var(--accent-soft);border:1.5px solid currentColor;border-radius:50%;width:52px;height:52px;display:grid;place-items:center}
  .screen strong{position:absolute;left:50%;top:63%;transform:translateX(-50%);color:var(--accent-soft);font:600 13px/1.2 "Segoe UI Variable","Microsoft YaHei",sans-serif;white-space:nowrap}
  .screen small{position:absolute;left:16px;bottom:14px;color:var(--faint);font:11px/1.2 "Cascadia Mono","Microsoft YaHei",sans-serif}
  .monitor-neck{position:absolute;left:46%;right:46%;bottom:5%;height:12%;background:var(--stand);border-radius:2px;z-index:-1}
  .monitor-foot{position:absolute;left:34%;right:34%;bottom:3%;height:5px;background:var(--stand);border-radius:3px}
  .companion-monitor{left:2%;top:5%;width:27%;height:31%;opacity:.65;filter:saturate(.6);transform:translate(calc(var(--slot) * 18px),calc(var(--slot) * 22px));z-index:calc(6 - var(--slot))}
  .companion-monitor .screen{inset:0 0 20%;padding:10px;box-shadow:0 10px 22px rgba(0,0,0,.14)}
  .companion-monitor .screen-index{width:30px;height:30px;font-size:13px;top:43%}
  .companion-monitor .screen strong{display:none}.companion-monitor .screen small{left:9px;bottom:8px;font-size:9px}.companion-monitor .monitor-foot{left:32%;right:32%;bottom:1%}
  .focus-path{position:absolute;left:0;top:0;width:55%;height:60%;overflow:visible;z-index:2;pointer-events:none}
  .focus-path path{fill:none;stroke:var(--accent);stroke-width:1.5;stroke-dasharray:5 6;vector-effect:non-scaling-stroke;opacity:.6}
  .focus-path .arrow{stroke-dasharray:none}
  .swap-note{position:absolute;left:23%;top:13%;font:11px/1.2 "Segoe UI Variable","Microsoft YaHei",sans-serif;color:var(--accent-soft);z-index:2}
  .stage-caption{position:absolute;left:0;right:0;bottom:5%;margin:0;text-align:center;color:var(--faint);font:12px/1.2 "Segoe UI Variable","Microsoft YaHei",sans-serif;letter-spacing:.04em}
  .zone{position:absolute;z-index:30;padding:0;border:1px solid var(--line-strong);border-radius:6px;background:var(--zone-bg);color:var(--muted);box-shadow:0 2px 8px rgba(0,0,0,.12);transition:transform .15s,background-color .15s,color .15s,border-color .15s,box-shadow .18s}
  .zone::before{content:"";position:absolute;inset:-5px;border:1px dashed transparent;border-radius:9px;opacity:0;pointer-events:none;transition:opacity .18s,border-color .18s}
  .zone.configured:not(.active)::before{opacity:1;border-width:1.5px;border-color:var(--zone-configured)}
  .zone:hover{color:#fff;border-color:var(--accent);background:var(--accent);transform:scale(1.06)}
  .zone.active,.zone.configured.active{color:#fff;border-color:var(--accent);background:var(--accent);box-shadow:0 0 0 3px var(--accent-bg),0 6px 14px rgba(35,84,190,.24);transform:scale(1.06);animation:zone-select-fill .28s cubic-bezier(.2,.9,.3,1.25)}
  .zone:active{transform:scale(.96)}
  .zone.active::before,.zone:hover::before{opacity:0}
  @keyframes zone-select-fill{0%{background:var(--zone-bg);transform:scale(.94)}65%{background:var(--accent);transform:scale(1.1)}100%{background:var(--accent);transform:scale(1.06)}}
  .zone em{position:absolute;display:grid;place-items:center;min-width:16px;height:16px;padding:0 3px;border-radius:8px;background:var(--accent);color:#fff;font:9px/1 "Cascadia Mono",monospace;font-style:normal}
  .zone span{position:absolute;opacity:0;white-space:nowrap;padding:4px 8px;background:var(--tooltip-bg);border:1px solid var(--line-strong);border-radius:6px;color:var(--ink);font-size:11px;pointer-events:none;transition:opacity .15s;box-shadow:var(--shadow-menu)}
  .zone:hover span{opacity:1}
  .zone-top-left,.zone-top-right,.zone-bottom-left,.zone-bottom-right{width:26px;height:26px}.zone-top-left{left:10px;top:10px}.zone-top-right{right:10px;top:10px}.zone-bottom-left{left:10px;bottom:calc(17% + 10px)}.zone-bottom-right{right:10px;bottom:calc(17% + 10px)}
  .zone-top,.zone-bottom{width:24%;height:12px;left:38%}.zone-top{top:10px}.zone-bottom{bottom:calc(17% + 10px)}.zone-left,.zone-right{height:25%;width:12px;top:30%}.zone-left{left:10px}.zone-right{right:10px}
  .zone em{right:-7px;top:-7px}.zone span{left:32px;top:-4px}.zone-top span,.zone-bottom span{left:50%;top:18px;transform:translateX(-50%)}.zone-right span,.zone-top-right span,.zone-bottom-right span{left:auto;right:32px}
  .window-demo{position:absolute;inset:20% 15% 31%;z-index:5;border:1px solid var(--line-strong);border-radius:8px;background:linear-gradient(160deg,var(--window-a),var(--window-b));display:grid;place-items:center;box-shadow:0 10px 24px rgba(0,0,0,.14);pointer-events:auto}
  .window-demo>span{font:12px "Segoe UI Variable","Microsoft YaHei",sans-serif;color:var(--muted)}
  .window-demo button{position:absolute;border:1px solid var(--line-strong);border-radius:5px;background:var(--raised);color:var(--muted);pointer-events:auto;font-size:10.5px;padding:0;box-shadow:0 2px 8px rgba(0,0,0,.12);transition:transform .15s cubic-bezier(.2,.9,.3,1.25),background-color .15s,color .15s,border-color .15s,box-shadow .18s,opacity .18s}
  .window-demo button::before{content:"";position:absolute;inset:-5px;border:1px dashed transparent;border-radius:8px;opacity:0;pointer-events:none;transition:opacity .18s,border-color .18s}
  .window-demo button.configured:not(.active)::before{opacity:1;border-width:1.5px;border-color:var(--zone-configured)}
  .window-demo button:hover:not(:disabled){border-color:var(--accent);background:var(--accent);color:#fff;box-shadow:0 0 0 3px var(--accent-bg),0 6px 14px rgba(35,84,190,.22);transform:scale(1.08)}
  .window-demo button.active{border-color:var(--accent);background:var(--accent);color:#fff;box-shadow:0 0 0 3px var(--accent-bg),0 6px 14px rgba(35,84,190,.24);animation:edge-select-fill .28s cubic-bezier(.2,.9,.3,1.25)}
  .window-demo button:active:not(:disabled){transform:scale(.92)}
  .window-demo button.active::before,.window-demo button:hover::before{opacity:0}
  .window-demo button.unavailable{border-color:var(--line);background:var(--unavailable-bg);color:var(--unavailable-ink);cursor:not-allowed;opacity:.6;box-shadow:none}
  @keyframes edge-select-fill{0%{background:var(--raised);transform:scale(.92)}65%{background:var(--accent);transform:scale(1.1)}100%{background:var(--accent);transform:scale(1)}}
  .window-demo .edge-left,.window-demo .edge-right{top:27%;bottom:27%;width:20px}.window-demo .edge-left{left:-10px}.window-demo .edge-right{right:-10px}.window-demo .edge-top,.window-demo .edge-bottom{left:31%;right:31%;height:20px}.window-demo .edge-top{top:-10px}.window-demo .edge-bottom{bottom:-10px}
  @media(max-width:900px){.physical-monitor{left:12%;width:74%}.has-companions .active-monitor{left:23%;width:70%}.companion-monitor{left:1%;width:26%}.swap-note{left:21%}}
  @media(max-width:700px){.physical-monitor{left:10%;top:17%;width:80%;height:58%}.has-companions .active-monitor{left:20%;width:76%}.companion-monitor{left:3%;top:4%;width:25%;height:27%}.focus-path{height:52%}.swap-note{top:11%}}
  @media(prefers-reduced-motion:reduce){.physical-monitor{transition-duration:.01ms}.zone.active,.zone.configured.active,.window-demo button.active{animation:none}}
</style>
