<script lang="ts">
  export let summary = "";
  export let description: string;
  export let detailsLabel: string;
</script>

<div class="feature-intro-copy">
  {#if summary}<p class="feature-summary">{summary}</p>{/if}
  <div class="feature-copy-heading">
    <slot />
    <details class="feature-details">
      <summary>{detailsLabel}</summary>
      <p>{description}</p>
    </details>
  </div>
</div>

<style>
  .feature-intro-copy { position: relative; min-width: 0; width: 100%; }
  .feature-copy-heading { position: relative; display: flex; align-items: flex-start; gap: 4px; min-width: 0; margin-top: 6px; }
  .feature-intro-copy p { color: var(--muted); font-size: var(--fs-sm); line-height: 1.6; overflow-wrap: anywhere; }
  .feature-intro-copy .feature-summary { margin: 0; }
  .feature-details { flex: 1; min-width: 0; margin: 0; }
  .feature-details > summary { width: fit-content; min-height: 24px; padding: 2px 0; color: var(--muted); font-size: var(--fs-sm); line-height: 1.6; cursor: pointer; }
  .feature-details > summary:hover { color: var(--accent-soft); }
  .feature-details > summary:focus-visible { outline: 2px solid var(--accent-soft); outline-offset: 2px; border-radius: var(--radius-2xs); }
  .feature-details > p { margin: 6px 0 0; }
</style>
