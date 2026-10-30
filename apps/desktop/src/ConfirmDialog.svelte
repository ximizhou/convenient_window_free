<script lang="ts">
  import { onMount } from "svelte";

  export let title: string;
  export let description: string;
  export let confirmLabel: string;
  export let cancelLabel: string;
  export let onConfirm: () => void;
  export let onCancel: () => void;

  let dialog: HTMLDialogElement;
  let cancelButton: HTMLButtonElement;

  onMount(() => {
    dialog.showModal();
    cancelButton.focus();
  });
</script>

<dialog
  aria-describedby="confirm-dialog-description"
  aria-labelledby="confirm-dialog-title"
  bind:this={dialog}
  class="confirm-dialog"
  on:cancel={(event) => { event.preventDefault(); onCancel(); }}
  on:click={(event) => { if (event.target === dialog) onCancel(); }}
>
  <div class="confirm-dialog-body">
    <h2 id="confirm-dialog-title">{title}</h2>
    <p id="confirm-dialog-description">{description}</p>
    <div class="confirm-dialog-actions">
      <button bind:this={cancelButton} class="quiet" on:click={onCancel} type="button">{cancelLabel}</button>
      <button class="danger" on:click={onConfirm} type="button">{confirmLabel}</button>
    </div>
  </div>
</dialog>

<style>
  .confirm-dialog {
    width: min(380px, calc(100vw - 32px));
    padding: 0;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-lg);
    background: var(--panel);
    color: var(--ink);
    box-shadow: var(--shadow-menu);
  }
  .confirm-dialog::backdrop { background: rgba(4, 7, 12, .58); backdrop-filter: blur(3px); }
  .confirm-dialog-body { padding: 22px; }
  h2 { margin: 0; font-size: var(--fs-xl); line-height: 1.35; }
  p { margin: 10px 0 0; color: var(--muted); font-size: var(--fs-md); line-height: 1.65; }
  .confirm-dialog-actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 22px; }
  .confirm-dialog-actions button { min-height: 36px; }
</style>
