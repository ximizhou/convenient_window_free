// Lexically injected into this iframe's build only; never replace DSH globals.
let state,
  pending = Promise.resolve();
const prefs = Object.create(null);
export const zh = () =>
  (prefs['convenient-window-language'] ?? navigator.language)
    .toLowerCase()
    .startsWith('zh');
export const text = (cn, en) => (zh() ? cn : en);
const errors = {
  'ordinary-host-required': [
    '请用普通权限打开 DSH，然后在运行中心为助手请求管理员权限。',
    'Open DSH normally. You can elevate its helper in Runtime Center.',
  ],
  stale: [
    '设置已在其他页签改变，请重新打开便捷窗口。',
    'Settings changed in another panel. Reopen Convenient Window.',
  ],
  'helper-in-use': [
    '另一个版本的助手正在运行。请先在那个版本里关闭总开关。',
    'Another helper is running. Turn it off in its own host first.',
  ],
  cancelled: ['已取消，保留原设置。', 'Cancelled. Your settings were kept.'],
  'native-disconnected': [
    '系统服务已退出，请重新打开便捷窗口。',
    'The native service closed. Reopen Convenient Window.',
  ],
  'pick-again': [
    '请重新选择启动程序。',
    'Choose the startup executable again.',
  ],
  'entry-changed': [
    '启动项已变化，请刷新后重试。',
    'The startup item changed. Refresh and try again.',
  ],
  'settings-unreadable': [
    '无法读取设置，原文件未被覆盖。',
    'Settings could not be read. The original files were not overwritten.',
  ],
};
export function describe(code) {
  if (/^admin[A-Z]/.test(code)) return code;
  const parts = errors[code];
  return parts
    ? text(...parts)
    : `${text('便捷窗口', 'Convenient Window')}: ${code}`;
}
export async function call(data, signal) {
  const res = await fetch('./control', {
    method: data ? 'POST' : 'GET',
    credentials: 'same-origin',
    cache: 'no-store',
    headers: data
      ? { 'content-type': 'application/json', 'x-convenient-window': 'panel' }
      : {},
    body: data
      ? JSON.stringify({ ...data, session: state.session })
      : undefined,
    signal,
  });
  const v = await res.json();
  if (!res.ok) throw Error(describe(v.error));
  return v;
}
export async function init() {
  state = await call(null);
  Object.assign(prefs, state.view);
  // The URL hints the initial locale only; never overwrite an explicit per-profile choice.
  const initialLang = new URLSearchParams(location.search).get('lang');
  if (
    !prefs['convenient-window-language'] &&
    ['zh-CN', 'en-US'].includes(initialLang)
  )
    localStorage.setItem('convenient-window-language', initialLang);
  return state;
}
function serial(fn) {
  const next = pending.then(fn);
  pending = next.catch(() => {});
  return next;
}
export const save = (settings) =>
  serial(async () => {
    const v = await call({ action: 'save', rev: state.rev, settings });
    state.rev = v.rev;
  });
export const start = () =>
  serial(() => call({ action: 'start', rev: state.rev }));
export const elevate = (enabled) =>
  serial(() => call({ action: 'elevation', enabled, rev: state.rev }));
export const localStorage = {
  getItem: (key) => prefs[key] ?? null,
  setItem(key, value) {
    const previous = prefs[key] ?? null;
    if (value === previous) return;
    prefs[key] = String(value);
    void serial(() =>
      call({ action: 'view', key, value: String(value), previous }),
    ).catch((e) => {
      if (prefs[key] === String(value)) {
        if (previous === null) delete prefs[key];
        else prefs[key] = previous;
      }
      const el = document.querySelector('#panel-message');
      if (el) el.textContent = e.message;
    });
  },
};
// Keep the desktop protocol client's request correlation, recoveries and UI events. Its
// socket is backed by an authenticated, finite set of host operations, not a token in JS.
export class WebSocket extends EventTarget {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  readyState = 0;
  abort = new AbortController();
  queue = Promise.resolve();
  constructor() {
    super();
    void this.poll();
  }
  frame(frame) {
    this.dispatchEvent(
      new MessageEvent('message', { data: JSON.stringify(frame) }),
    );
  }
  async poll() {
    let cursor = null;
    try {
      while (this.readyState !== 3) {
        const v = await call(
          { action: 'observe', cursor, epoch: this.epoch, wait: true },
          this.abort.signal,
        );
        if (this.readyState === 3) return;
        this.epoch = v.epoch;
        cursor = v.cursor;
        if (this.readyState === 0) {
          this.readyState = 1;
          this.dispatchEvent(new Event('open'));
        }
        for (const frame of v.frames) this.frame(frame);
        await new Promise((r) => setTimeout(r, 100));
      }
    } catch {
      if (this.readyState !== 3) this.close();
    }
  }
  send(raw) {
    if (this.readyState !== 1) throw Error('Disconnected');
    const { type, data, id } = JSON.parse(raw);
    const task = this.queue.then(async () => {
      if (this.readyState !== 1) return;
      await pending;
      const d = type === 'config.update' ? { revision: data.revision } : data;
      const res = await call(
        {
          action: 'engine',
          epoch: this.epoch,
          type,
          id,
          data: d,
          rev: state.rev,
        },
        this.abort.signal,
      );
      if (type === 'config.update' && this.readyState === 1)
        this.frame({
          id,
          type: 'config.applied',
          data: { requestId: id, revision: res.applied },
        });
      if (type === 'helper.stop') this.close();
    });
    this.queue = task.catch(() => {
      if (this.readyState === 1)
        this.frame({
          id,
          type: 'runtime.error',
          data: { requestId: id, details: 'Operation failed' },
        });
    });
  }
  close() {
    if (this.readyState === 3) return;
    this.readyState = 3;
    this.abort.abort();
    this.dispatchEvent(new Event('close'));
  }
}
