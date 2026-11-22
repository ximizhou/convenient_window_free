// The DSH renderer never receives the engine token or a generic IPC/shell/file API.
import { readFile, stat } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { verifyNative } from './native-service.js';
import { Helper } from './helper.js';
import { assets, metadata, root, verifyPayload } from './assets.js';
import { atomicJSON, readPrefs, runtimeConfig } from './settings.js';
import {
  defaultSettings,
  normalizeSettings,
  settingsForHelper,
  MAX_SETTINGS_STORAGE_BYTES,
  gestureLabels,
  translator,
  toHostAccelerator,
  filterLayoutWindows,
  normalizeLayoutExclusions,
} from '../lib/model.mjs';
const keys = new Set([
  'convenient-window-theme',
  'convenient-window-language',
  'convenient-window-navigation',
  'convenient-window-pane-widths',
  'convenient-window-layouts',
  'convenient-window-layout-exclusions',
]);
const allowedEvents = new Set([
  'helper.ready',
  'helper.pong',
  'runtime.status',
  'runtime.error',
  'action.triggered',
  'gesture.recognized',
  'ocr.completed',
  'usage.status',
  'taskbar.status',
  'shortcut.recording',
  'shortcut.hotkeys.synced',
  'window.layout.captured',
  'window.layout.restored',
  'adjustment.updated',
]);
const plain = (v) => !!v && typeof v === 'object' && !Array.isArray(v);
const idOk = (v) => typeof v === 'string' && /^[a-zA-Z0-9._:-]{1,128}$/.test(v);
async function boundedJSON(path, limit) {
  if ((await stat(path)).size > limit) throw Error('settings-unreadable');
  return JSON.parse(await readFile(path, 'utf8'));
}
export function cleanConfig(s) {
  if (
    !plain(s) ||
    s.schemaVersion !== 9 ||
    typeof s.enabled !== 'boolean' ||
    !Array.isArray(s.hotzones) ||
    !plain(s.mouseGestures) ||
    Buffer.byteLength(JSON.stringify(s)) > MAX_SETTINGS_STORAGE_BYTES
  )
    throw Error('invalid-settings');
  const cfg = normalizeSettings(s);
  return cfg;
}
export function configuredCommands(cfg) {
  const actions = [
    ...cfg.hotzones,
    ...cfg.monitorProfiles.flatMap((p) => p.hotzones),
  ]
    .flatMap((z) => z.actions)
    .concat(cfg.mouseGestures.gestures);
  const commands = new Set();
  for (const s of actions)
    for (const a of [
      s.action,
      ...(s.modifierActions ?? []).map((x) => x.action),
    ])
      if (a?.kind === 'open-command' && a.value?.trim()) commands.add(a.value);
  return [...commands].sort();
}
export async function readPanel(dir) {
  try {
    const v = await boundedJSON(
      join(dir, 'panel-settings.json'),
      MAX_SETTINGS_STORAGE_BYTES + 1024,
    );
    if (v.format !== 2 || !Number.isSafeInteger(v.rev) || v.rev < 0)
      throw Error();
    return { rev: v.rev, settings: cleanConfig(v.settings) };
  } catch (e) {
    if (e.code !== 'ENOENT') throw Error('settings-unreadable');
  }
  try {
    await stat(join(dir, 'settings.json'));
    const legacy = await readPrefs(dir);
    const cfg = await runtimeConfig(legacy.prefs);
    return { rev: 0, settings: { ...cfg, enabled: false } };
  } catch (e) {
    if (e.code !== 'ENOENT') throw Error('settings-unreadable');
  }
  return {
    rev: 0,
    settings: { ...structuredClone(defaultSettings), enabled: false },
  };
}
export async function loadView(dir) {
  try {
    const v = await boundedJSON(join(dir, 'panel-view.json'), 2 * 1024 * 1024);
    if (
      !plain(v) ||
      Object.entries(v).some(([k, v]) => !keys.has(k) || typeof v !== 'string')
    )
      throw Error();
    return v;
  } catch (e) {
    if (e.code === 'ENOENT') return {};
    throw Error('settings-unreadable');
  }
}
function viewJSON(view, key, fallback) {
  try {
    return JSON.parse(view[key]);
  } catch {
    return fallback;
  }
}
export class PanelController {
  constructor(dir, helper = new Helper(join(dir, 'runtime'))) {
    this.dir = dir;
    this.helper = helper;
    this.session = randomUUID();
    this.queue = Promise.resolve();
    this.seq = 0;
    this.events = [];
    this.disposed = false;
    this.waiters = new Set();
    helper.onEnd = () => this.notify();
    helper.onFrame = (m) => this.record(m);
  }
  load() {
    return (this.loaded ??= Promise.all([
      readPanel(this.dir),
      loadView(this.dir),
      Promise.all([verifyPayload(), verifyNative()]).then(
        () => true,
        () => false,
      ),
    ]).then(([v, view, valid]) => {
      this.v = v;
      this.view = view;
      this.payloadValid = valid;
      if (this.helper.native)
        this.helper.native.lang =
          view['convenient-window-language'] === 'en-US' ? 'en-US' : 'zh-CN';
    }));
  }
  serial(fn) {
    const p = this.queue.then(fn);
    this.queue = p.catch(() => {});
    return p;
  }
  snapshot() {
    return {
      ...this.v,
      view: this.view,
      session: this.session,
      helperVersion: assets.version,
      installed: this.payloadValid,
      product: metadata.productVersion,
      bytes: assets.files.reduce((n, f) => n + f.bytes, 0),
      running: !!this.helper.running,
      elevated: this.helper.running ? (this.helper.elevated ?? null) : false,
      adapter: metadata.version,
    };
  }
  send(type, data = {}, id = randomUUID()) {
    if (!this.helper.running) throw Error('helper-disconnected');
    this.helper.ws.send(JSON.stringify({ type, id, data, time: Date.now() }));
  }
  record(m) {
    if (m.type === 'shortcut.hotkey.triggered') {
      const layouts = viewJSON(
          this.view ?? {},
          'convenient-window-layouts',
          [],
        ),
        l =
          Array.isArray(layouts) &&
          layouts.find((l) => l?.id === m.data?.hotkeyId);
      if (l && Array.isArray(l.windows) && l.windows.length <= 64) {
        const exclusions = normalizeLayoutExclusions(
          viewJSON(this.view, 'convenient-window-layout-exclusions', []),
        );
        try {
          this.send('window.layout.restore', {
            windows: filterLayoutWindows(l.windows, exclusions),
          });
        } catch {}
      }
      return;
    }
    if (!allowedEvents.has(m.type)) return;
    if (m.type === 'helper.ready') {
      this.epoch = randomUUID();
      this.events = [];
      this.status = null;
      this.taskbar = null;
      this.ready = m;
    }
    if (m.type === 'runtime.status' && Array.isArray(m.data?.displays))
      this.status = m;
    if (m.type === 'taskbar.status') this.taskbar = m;
    const item = { seq: ++this.seq, m };
    this.events.push(item);
    if (this.events.length > 128) this.events.shift();
    this.notify();
  }
  notify() {
    for (const wake of this.waiters) wake();
  }
  async observe(b, signal) {
    const check = () => {
      if (this.disposed || b.session !== this.session) throw Error('stale');
      if (
        !this.helper.running ||
        !this.ready ||
        (b.epoch && b.epoch !== this.epoch)
      )
        throw Error('helper-disconnected');
    };
    check();
    if (b.cursor === null)
      return {
        epoch: this.epoch,
        cursor: this.seq,
        frames: [this.ready, this.status, this.taskbar].filter(Boolean),
      };
    if (!Number.isSafeInteger(b.cursor) || b.cursor < 0 || b.cursor > this.seq)
      throw Error('stream-expired');
    if (b.wait === true && b.cursor === this.seq && !signal?.aborted) {
      if (this.waiters.size >= 4) throw Error('stream-busy');
      await new Promise((ok) => {
        const wake = () => {
          clearTimeout(timer);
          signal?.removeEventListener('abort', wake);
          this.waiters.delete(wake);
          ok();
        };
        const timer = setTimeout(wake, 8000);
        this.waiters.add(wake);
        signal?.addEventListener('abort', wake, { once: true });
      });
    }
    check();
    if (b.cursor < (this.events[0]?.seq ?? 1) - 1)
      throw Error('stream-expired');
    return {
      epoch: this.epoch,
      cursor: this.seq,
      frames: this.events.filter((x) => x.seq > b.cursor).map((x) => x.m),
    };
  }
  syncHotkeys(id) {
    const layouts = viewJSON(this.view, 'convenient-window-layouts', []);
    const hotkeys = (Array.isArray(layouts) ? layouts : [])
      .filter((l) => l && idOk(l.id) && typeof l.shortcut === 'string')
      .map((l) => ({ id: l.id, accelerator: toHostAccelerator(l.shortcut) }))
      .filter((l) => l.accelerator)
      .slice(0, 24);
    this.send('shortcut.hotkeys.sync', { hotkeys }, id);
  }
  activate(loginDir, requestAdmin) {
    return this.serial(async () => {
      await this.load();
      if (this.disposed || !this.v.settings.enabled || this.helper.running)
        return;
      try {
        // Restore the saved master switch even when the settings tab is never opened.
        await this.helper.start();
        if (
          loginDir &&
          resolve(loginDir).toLowerCase() === resolve(this.dir).toLowerCase() &&
          requestAdmin === '1'
        ) {
          const login = await this.helper.native.call('login');
          if (login.enabled && login.admin) await this.helper.elevation(true);
        }
        const cfg = settingsForHelper(this.v.settings);
        const lang = this.helper.native.lang;
        await this.helper.apply(
          cfg,
          this.v.rev,
          gestureLabels(cfg.mouseGestures.gestures, translator(lang)),
        );
        this.syncHotkeys();
      } catch (e) {
        await this.helper.stop().catch(() => {});
        this.helper.fault = e.message;
      }
    });
  }
  request(b, signal) {
    if (b?.action === 'observe') return this.observe(b, signal);
    return this.serial(async () => {
      if (this.disposed) throw Error('unloaded');
      await this.load();
      if (b === null) return this.snapshot();
      if (!plain(b) || b.session !== this.session) throw Error('stale');
      if (b.action === 'stop') {
        await this.helper.stop();
        return { ok: true };
      }
      if (b.action === 'save') {
        if (b.rev !== this.v.rev) throw Error('stale');
        const settings = cleanConfig(b.settings);
        const previous = new Set(configuredCommands(this.v.settings));
        const added = configuredCommands(settings).filter(
          (x) => !previous.has(x),
        );
        if (
          added.length &&
          !(await this.helper.native.call('confirm-commands', added))
        )
          throw Error('cancelled');
        const next = { rev: this.v.rev + 1, settings };
        await atomicJSON(this.dir, 'panel-settings.json', {
          format: 2,
          ...next,
        });
        this.v = next;
        return { rev: next.rev };
      }
      if (b.action === 'view') {
        if (
          !keys.has(b.key) ||
          typeof b.value !== 'string' ||
          b.value.length > 750000
        )
          throw Error('invalid-settings');
        if ((this.view[b.key] ?? null) !== b.previous) throw Error('stale');
        const v = { ...this.view, [b.key]: b.value };
        if (Buffer.byteLength(JSON.stringify(v)) > 2 * 1024 * 1024)
          throw Error('invalid-settings');
        await atomicJSON(this.dir, 'panel-view.json', v);
        this.view = v;
        if (b.key === 'convenient-window-layouts' && this.helper.running)
          this.syncHotkeys();
        return { ok: true };
      }
      if (b.action === 'start') {
        if (b.rev !== this.v.rev) throw Error('stale');
        if (!this.v.settings.enabled) throw Error('power-off');
        const alreadyRunning = !!this.helper.running;
        const info = await this.helper.start();
        if (!alreadyRunning) this.syncHotkeys();
        return {
          alreadyRunning,
          elevated: info?.elevated ?? false,
          warning: info?.warning ?? null,
        };
      }
      if (b.action === 'native') {
        if (!this.payloadValid && this.helper instanceof Helper)
          throw Error('payload-invalid');
        if (
          ![
            'startup',
            'pick',
            'login',
            'set-login',
            'set-login-admin',
            'language',
          ].includes(b.op)
        )
          throw Error('invalid-command');
        if (
          b.op === 'startup' &&
          !['list', 'add', 'setEnabled'].includes(b.data?.action)
        )
          throw Error('invalid-command');
        if (
          ['set-login', 'set-login-admin'].includes(b.op) &&
          typeof b.data !== 'boolean'
        )
          throw Error('invalid-command');
        if (b.op === 'language') {
          this.helper.native.lang = b.data === 'en-US' ? 'en-US' : 'zh-CN';
          if (!this.helper.native.child) return null;
        }
        return this.helper.native.call(b.op, b.data);
      }
      if (b.action === 'elevation') {
        if (
          b.rev !== this.v.rev ||
          typeof b.enabled !== 'boolean' ||
          !this.helper.running
        )
          throw Error('stale');
        const r = await this.helper.elevation(b.enabled);
        this.syncHotkeys();
        return { elevated: r.elevated, warning: r.warning };
      }
      if (b.action === 'diagnostics') {
        const native = this.helper.native?.session
          ? await this.helper.native.call('status').catch(() => null)
          : null;
        return {
          installed: this.payloadValid,
          development: false,
          helperRunning: !!this.helper.running,
          lastExitCode: native?.lastExitCode ?? null,
          hudTitle: native?.hudTitle ?? null,
          loginTargetValid: native?.loginTargetValid ?? false,
          installedProfile: native?.installedProfile ?? false,
          lastError: this.helper.fault ?? null,
        };
      }
      if (
        b.action !== 'engine' ||
        !this.helper.running ||
        !idOk(b.id) ||
        b.epoch !== this.epoch ||
        !plain(b.data)
      )
        throw Error('invalid-command');
      const d = b.data;
      switch (b.type) {
        case 'config.update': {
          if (
            b.rev !== this.v.rev ||
            !Number.isSafeInteger(d.revision) ||
            d.revision < 0
          )
            throw Error('stale');
          const cfg = settingsForHelper(this.v.settings);
          const lang =
            this.view['convenient-window-language'] === 'en-US'
              ? 'en-US'
              : 'zh-CN';
          try {
            await this.helper.apply(
              cfg,
              d.revision,
              gestureLabels(cfg.mouseGestures.gestures, translator(lang)),
            );
          } catch (e) {
            await this.helper.stop();
            throw e;
          }
          return { applied: d.revision };
        }
        case 'helper.stop':
          await this.helper.stop();
          break;
        case 'helper.ping':
        case 'usage.get':
        case 'window.layout.capture':
          this.send(b.type, {}, b.id);
          break;
        case 'shortcut.record.begin':
        case 'shortcut.record.cancel':
          if (!idOk(d.sessionId)) throw Error('invalid-command');
          this.send(b.type, { sessionId: d.sessionId }, b.id);
          break;
        case 'shortcut.hotkeys.sync':
          this.syncHotkeys(b.id);
          break;
        case 'window.layout.restore':
          if (
            !Array.isArray(d.windows) ||
            d.windows.length > 64 ||
            d.windows.some(
              (w) =>
                !plain(w) ||
                typeof w.processName !== 'string' ||
                !plain(w.rect),
            )
          )
            throw Error('invalid-command');
          this.send(b.type, { windows: d.windows }, b.id);
          break;
        default:
          throw Error('invalid-command');
      }
      return { ok: true };
    });
  }
  dispose() {
    this.disposed = true;
    this.notify();
    return this.helper.dispose
      ? this.helper.dispose()
      : this.serial(() => this.helper.stop());
  }
}
