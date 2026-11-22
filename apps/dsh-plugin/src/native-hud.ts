import { mount } from 'svelte';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import HudHost from '../../desktop/src/AdjustmentHudHost.svelte';
import { LANGUAGE_KEY } from '../../desktop/src/i18n';
async function language(value: unknown) {
  if (value !== 'zh-CN' && value !== 'en-US') return;
  localStorage.setItem(LANGUAGE_KEY, value);
  window.dispatchEvent(
    new StorageEvent('storage', { key: LANGUAGE_KEY, newValue: value }),
  );
}
void (async () => {
  await listen<string>('dsh-hud-language', (e) => language(e.payload));
  await language(await invoke('hud_language'));
  mount(HudHost, { target: document.querySelector('#app')! });
})();
