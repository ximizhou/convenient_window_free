<script lang="ts">
  import { onMount, tick } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { feedbackText, FeedbackOrder, type AdjustmentFeedback, type AdjustmentSnapshot } from "./adjustment-feedback";

  let feedback = $state<AdjustmentFeedback | null>(null);
  const level = $derived(Math.max(0, Math.min(1, feedback?.level?.value ?? 0)));
  const brightness = $derived(feedback?.level ? level : 0.5);
  const order = new FeedbackOrder();

  async function receive(snapshot: AdjustmentSnapshot) {
    if (!order.accept(snapshot)) return;
    feedback = snapshot.feedback;
    if (!feedback) return;
    await tick();
    await invoke("adjustment_hud_present", { revision: snapshot.revision });
  }

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      unlisten = await listen<AdjustmentSnapshot>("adjustment-feedback", event => {
        void receive(event.payload);
      });
      if (disposed) { unlisten(); return; }
      // Revisions order both live events and the initial snapshot across reconnects.
      await receive(await invoke<AdjustmentSnapshot>("adjustment_hud_ready"));
    })();
    return () => { disposed = true; unlisten?.(); };
  });
</script>

{#if feedback}
  <div class="hud" class:error={Boolean(feedback.error)} role="status" aria-live="polite">
    <div class="icon" aria-hidden="true">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
        {#if feedback.kind === "brightness"}
          <circle class="sun" cx="12" cy="12" r={2.5 + brightness} />
          <g class="rays" opacity={0.4 + brightness * 0.6}>
            {#each [0, 45, 90, 135, 180, 225, 270, 315] as angle}
              <path d={`M12 6.5V${5.5 - brightness * 3}`} transform={`rotate(${angle} 12 12)`} />
            {/each}
          </g>
        {:else}
          <path d="M10 5 6 9H3v6h3l4 4V5Z" />
          {#if feedback.level?.muted}
            <path d="m15 9 6 6m0-6-6 6" />
          {:else}
            <path class="wave" opacity={level > 0 ? 1 : 0} d="M13 10a3 3 0 0 1 0 4" />
            <path class="wave" opacity={level > 1 / 3 ? 1 : 0} d="M16 7.5a6.5 6.5 0 0 1 0 9" />
            <path class="wave" opacity={level > 2 / 3 ? 1 : 0} d="M19 5a10 10 0 0 1 0 14" />
          {/if}
        {/if}
      </svg>
    </div>
    <div class="content">
      <div class="heading">
        <span>{feedback.kind === "volume" ? "音量" : "亮度"}</span>
        <span class="value">{feedbackText(feedback)}</span>
      </div>
      <div class="device">{feedback.error ?? feedback.level?.deviceName ?? "正在读取设备"}</div>
      <div class="track" class:unavailable={!feedback.level}>
        <div class="fill" style:transform={`scaleX(${level})`}></div>
      </div>
    </div>
  </div>
{/if}

<style>
  :global(*) { box-sizing: border-box; }
  :global(body) { color: #f4f5f7; font-family: "Segoe UI", "Microsoft YaHei", system-ui, sans-serif; user-select: none; overflow: hidden; }
  .hud { height: 100vh; width: 100%; display: flex; align-items: center; gap: 12px; padding: 10px 16px; border: 1px solid #343944; }
  .icon { width: 24px; flex-shrink: 0; color: #d5e1f8; }
  svg { display: block; width: 24px; height: 24px; }
  .wave, .rays { transition: opacity 80ms linear; }
  .sun { transition: r 80ms linear; }
  .content { flex: 1; min-width: 0; }
  .heading { display: flex; justify-content: space-between; align-items: baseline; font-size: 12px; line-height: 20px; }
  .value { font-size: 16px; font-weight: 600; font-variant-numeric: tabular-nums; }
  .device { margin: 1px 0 7px; font-size: 10px; line-height: 14px; color: #b6bfcd; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .track { height: 3px; border-radius: 2px; overflow: hidden; background: #414854; }
  .fill { height: 100%; background: #9bbcff; border-radius: inherit; transform-origin: left; transition: transform 80ms linear; }
  .unavailable { visibility: hidden; }
  .error .icon, .error .value { color: #f2b4a5; }
  @media (prefers-reduced-motion: reduce) { .fill, .wave, .rays, .sun { transition: none; } }
</style>
