import { mount } from 'svelte';

import App from '../../desktop/src/App.svelte';
import '../../desktop/src/styles.css';
import './panel.css';
import {
  configureHostBridge,
  type HostBridge,
} from '../../desktop/src/host-bridge';
import { init, start, save, call, text, elevate } from './globals.js';
async function importFile() {
  return new Promise<unknown | null>((ok, no) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.json,application/json';
    input.oncancel = () => ok(null);
    input.onchange = async () => {
      try {
        const file = input.files?.[0];
        if (!file) return ok(null);
        if (file.size > 900 * 1024)
          throw Error(text('文件超过 900 KB', 'File exceeds 900 KB'));
        ok(JSON.parse(await file.text()));
      } catch (e) {
        no(e);
      }
    };
    input.click();
  });
}
async function exportFile(name: string, body: string) {
  const blob = new Blob([body], { type: 'application/json' });
  const w = window as Window & { showSaveFilePicker?: Function };
  if (w.showSaveFilePicker) {
    try {
      const file = await w.showSaveFilePicker({
        suggestedName: name,
        types: [
          { description: 'JSON', accept: { 'application/json': ['.json'] } },
        ],
      });
      const writer = await file.createWritable();
      await writer.write(blob);
      await writer.close();
      return true;
    } catch (e) {
      if ((e as Error).name === 'AbortError') return false;
      throw e;
    }
  }
  const a = document.createElement('a');
  const url = URL.createObjectURL(blob);
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 60000);
  return true;
}
async function boot() {
  const s = await init();
  const repo = 'https://github.com/ximizhou/convenient_window_free';
  let elevated: boolean | null = s.running ? s.elevated : false;
  const native = (op: string, data: unknown = null) =>
    call({ action: 'native', op, data });
  const login = async (op: string, admin = false, value?: boolean) => {
    try {
      const v = await native(op, value);
      return { enabled: admin ? v.admin : v.enabled };
    } catch (e) {
      return { enabled: null, error: (e as Error).message };
    }
  };
  const host: HostBridge = {
    kind: 'desktop',
    async setLanguage(lang) {
      document.title = lang === 'zh-CN' ? '便捷窗口' : 'Convenient Window';
      await native('language', lang);
    },
    getInitialSettings: () => s.settings,
    saveSettings: save,
    getHelperToken: () => null,
    getHelperState: () => ({
      installed: s.installed,
      error: s.installed
        ? undefined
        : text(
            '助手文件缺失或校验失败',
            'Bundled helper is missing or failed verification',
          ),
      development: false,
      version: s.helperVersion,
      bytes: s.bytes,
      repository: repo,
      release: repo,
      installDir: 'DSH · payload',
    }),
    async startHelper() {
      try {
        const info = await start();
        elevated = !!info.elevated;
        return { ok: true, ...info };
      } catch (e) {
        return { ok: false, error: (e as Error).message };
      }
    },
    async stopHelper() {
      try {
        await call({ action: 'stop' });
        return { ok: true };
      } catch (e) {
        return { ok: false, error: (e as Error).message };
      }
    },
    getPrivilegeSupport: () => ({ supported: true }),
    getPrivilegeState: () => ({ supported: true, elevated }),
    async setHelperElevation(enabled) {
      try {
        const v = await elevate(enabled);
        elevated = v.elevated;
        return { ok: true, ...v };
      } catch (e) {
        try {
          const v = await call(null);
          elevated = v.running ? v.elevated : false;
        } catch {
          elevated = null;
        }
        return { ok: false, elevated, error: (e as Error).message };
      }
    },
    getStartup: () => login('login'),
    setStartup: (enabled) => login('set-login', false, enabled),
    getAdminStartup: () => login('login', true),
    setAdminStartup: (enabled) => login('set-login-admin', true, enabled),
    openExternal(url) {
      if (url !== repo) return { ok: false };
      window.open(repo, '_blank', 'noopener,noreferrer');
      return { ok: true };
    },
    redirect: () => ({ ok: false }),
    diagnostics: async () => ({
      appDataDir: '',
      settingsPath: '',
      helperDataDir: '',
      helperPath: '',
      helperPayloadBytes: s.bytes,
      logTail: [],
      ...(await call({ action: 'diagnostics' })),
    }),
    importSettings: importFile,
    exportSettings: (v) =>
      exportFile(
        'convenient-window-settings.json',
        JSON.stringify(v, null, 2) + '\n',
      ),
    saveDiagnosticReport: (v) =>
      exportFile('convenient-window-diagnostics.json', v),
    startupItems: async (req) => {
      try {
        return await native('startup', req);
      } catch (e) {
        return {
          ok: false,
          version: 1,
          items: [],
          error: { code: 'unavailable', message: (e as Error).message },
        };
      }
    },
    pickStartupExecutable: () => native('pick'),
  };
  configureHostBridge(host);
  mount(App, { target: document.querySelector('#app')! });

  document.documentElement.dataset.appReady = 'true';
}
void boot().catch((e) => {
  document.documentElement.dataset.appError = 'true';
  document.querySelector('#panel-message')!.textContent = e.message;
});
