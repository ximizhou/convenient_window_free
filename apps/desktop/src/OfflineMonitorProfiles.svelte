<script lang="ts">
  import type { AppSettings, DisplayInfo } from "./types";
  import { canManageMonitorProfiles, listOfflineMonitorProfiles, removeOfflineMonitorProfile, monitorProfileLabels } from "./monitor-profile-management";

  export let settings: AppSettings;
  export let displays: DisplayInfo[];
  export let ready = false;
  export let english = false;
  // The host persists the replacement and publishes it only after saving succeeds.
  export let onRemove: (next: AppSettings) => boolean | Promise<boolean>;

  let confirming = "";
  let busy = false;
  let feedback: "failed" | "changed" | null = null;
  $: t = english ? monitorProfileLabels.en : monitorProfileLabels.zh;
  $: canManage = canManageMonitorProfiles(displays, ready);
  $: profiles = listOfflineMonitorProfiles(settings, displays, ready);
  $: if (confirming && !profiles.some((profile) => profile.monitorId === confirming)) confirming = "";

  async function remove(monitorId: string): Promise<void> {
    if (busy) return;
    const next = removeOfflineMonitorProfile(settings, displays, ready, monitorId);
    if (!next) { confirming = ""; feedback = "changed"; return; }
    busy = true;
    feedback = null;
    try {
      if (await onRemove(next)) confirming = "";
      else feedback = "failed";
    } catch { feedback = "failed"; }
    finally { busy = false; }
  }
</script>

<section class="offline-profiles" aria-label={t.title}>
  <h3>{t.title}</h3>
  <p class="description">{t.description}</p>
  {#if !canManage}
    <p class="profile-empty">{t.waiting}</p>
  {:else if profiles.length === 0}
    <p class="profile-empty">{t.empty}</p>
  {:else}
    <ul>
      {#each profiles as profile, index (profile.monitorId)}
        <li>
          <div class="profile-row">
            <div><strong>{t.screen} {index + 1}</strong><p>{#if profile.hasHotzones}{profile.hotzoneCount} {t.hotzones}{/if}{#if profile.hasHotzones && profile.hasEdgeHide} · {/if}{#if profile.hasEdgeHide}{profile.edgeCount} {t.edges}{/if}</p></div>
            <button class="quiet" type="button" disabled={busy} on:click={() => { confirming = profile.monitorId; feedback = null; }}>{t.remove}</button>
          </div>
          <details><summary>{t.identity}</summary><code>{profile.monitorId}</code></details>
          {#if confirming === profile.monitorId}
            <div class="profile-confirm">
              <p>{t.confirmation}</p>
              <div><button class="danger" type="button" disabled={busy} on:click={() => remove(profile.monitorId)}>{t.confirm}</button><button class="quiet" type="button" disabled={busy} on:click={() => { confirming = ""; }}>{t.cancel}</button></div>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if feedback}<p role="status">{t[feedback]}</p>{/if}
</section>

<style>
  .offline-profiles { min-width: 0; margin-top: 16px; padding-top: 14px; border-top: 1px solid var(--line); }
  h3 { margin: 0; color: var(--ink); font-size: var(--fs-md); }
  p { margin: 5px 0 0; color: var(--faint-text); font-size: var(--fs-xs); line-height: 1.6; }
  ul { margin: 12px 0 0; padding: 0; list-style: none; }
  li { padding: 12px 0; border-top: 1px solid var(--line); }
  li:first-child { border-top: 0; }
  .profile-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  strong { font-size: var(--fs-sm); font-weight: 600; color: var(--ink); }
  .profile-row button { flex: none; }
  details { margin-top: 8px; min-width: 0; color: var(--faint-text); font-size: var(--fs-xs); }
  summary { cursor: pointer; }
  summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  code { display: block; margin-top: 6px; overflow-wrap: anywhere; font-size: var(--fs-xs); }
  .profile-confirm { margin-top: 10px; padding: 10px; border-radius: var(--radius-sm); background: var(--surface); }
  .profile-confirm p { color: var(--muted); }
  .profile-confirm > div { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 8px; }
  .profile-empty { margin-top: 10px; }
</style>
