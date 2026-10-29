<script lang="ts">
  import type { NavigationFeatureId } from "./navigation-preferences";

  // 底部页签与设置页气泡共用同一套图标：这里只负责“画什么”，尺寸和颜色由宿主样式表决定。
  export let id: NavigationFeatureId;
</script>

{#if id === "power"}<span class="nav-icon power-icon" aria-hidden="true"></span>
{:else if id === "hotzones"}<span class="nav-icon corner-icon" aria-hidden="true"></span>
{:else if id === "edge-hide"}<span class="nav-icon edge-icon" aria-hidden="true"></span>
{:else if id === "gestures"}<span class="nav-icon gesture-icon" aria-hidden="true"></span>
{:else if id === "window-layouts"}<svg class="nav-icon-svg nav-layout-glyph" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.55" aria-hidden="true"><rect x="3" y="3" width="14" height="14" rx="1.5" /><rect x="9" y="9" width="12" height="12" rx="1.5" fill="var(--glyph-cutout, var(--panel))" /></svg>
{:else if id === "startup"}<svg class="nav-icon-svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.6" aria-hidden="true"><path d="M12 3v9m-5.5-7a8 8 0 1 0 11 0"/><path d="m9 10 3 3 3-3"/></svg>
{:else}<svg class="nav-beauty" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.7" aria-hidden="true"><path d="m4.5 19.5 10-10a1.4 1.4 0 0 1 2 2l-10 10a1.4 1.4 0 0 1-2-2ZM12 12l2 2M8 3l.8 2.2L11 6l-2.2.8L8 9l-.8-2.2L5 6l2.2-.8ZM19 3v4m-2-2h4M20 16v4m-2-2h4" /></svg>{/if}
