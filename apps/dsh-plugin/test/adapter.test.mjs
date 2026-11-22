import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { once } from 'node:events';
import { dataDir, handler, permitted } from '../host/index.js';
import { initial, runtimeConfig, validate } from '../host/settings.js';
import { listenerPids, matchesReady } from '../host/helper.js';
import { assets, metadata } from '../host/assets.js';
import {
  PanelController,
  readPanel,
  configuredCommands,
} from '../host/panel-controller.js';
import { defaultSettings } from '../lib/model.mjs';

async function fixture(t) {
  const dir = await mkdtemp(
    join(process.env.CW_TEST_TMP || tmpdir(), 'cw-unit-'),
  );
  t.after(() => rm(dir, { recursive: true, force: true }));
  const calls = [];
  const helper = {
    running: false,
    start() {
      this.running = true;
      this.onFrame({ type: 'helper.ready', data: {} });
    },
    stop() {
      this.running = false;
    },
    apply(c, r, l) {
      calls.push([c, r, l]);
    },
    ws: {
      send(raw) {
        calls.push(JSON.parse(raw));
      },
    },
  };
  const c = new PanelController(dir, helper);
  return {
    dir,
    c,
    calls,
    helper,
    cmd: async (action, args = {}) => {
      const v = await c.request(null);
      return c.request({ session: v.session, rev: v.rev, action, ...args });
    },
  };
}
test('maps choices to the existing schema, enables only the chosen hover action', async () => {
  const p = initial(),
    empty = await runtimeConfig(p);
  assert.equal(empty.schemaVersion, 9);
  assert.equal(empty.hotzonesEnabled, false);
  p.edgeHide = true;
  p.edges = ['left', 'top'];
  p.corners['top-left'] = 'show-desktop';
  p.hoverDelayMs = 850;
  const cfg = await runtimeConfig(p);
  assert.deepEqual(cfg.edgeHide.edges, p.edges);
  assert(cfg.edgeHide.enabled);
  assert(cfg.hotzonesEnabled);
  for (const key of [
    'topmostPin',
    'mouseGestures',
    'windowDrag',
    'taskbarAppearance',
  ])
    assert.equal(cfg[key].enabled, false);
  const active = cfg.hotzones.filter((z) => z.enabled);
  assert.deepEqual(
    active.map((z) => z.id),
    ['top-left'],
  );
  const slots = cfg.hotzones
    .flatMap((z) => z.actions)
    .filter((a) => a.action.kind !== 'none');
  assert.equal(slots.length, 1);
  assert.deepEqual(
    [slots[0].trigger, slots[0].action.kind, slots[0].hoverDelayMs],
    ['hover', 'show-desktop', 850],
  );
});
test('rejects invalid or unknown settings instead of passing arbitrary actions to the helper', () => {
  for (const edit of [
    { edges: [] },
    { edges: ['left', 'left'] },
    { edgeHide: 1 },
    { hoverDelayMs: 99 },
    { hoverDelayMs: 2500 },
    { extra: true },
    { corners: { ...initial().corners, 'top-left': 'unknown' } },
  ])
    assert.throws(
      () => validate({ ...initial(), ...edit }),
      /invalid-settings/,
    );
  assert.throws(() => dataDir({ dir: 'relative' }));
  assert.equal(
    dataDir({ dir: resolve('profile') }),
    resolve('profile', 'convenient-window'),
  );
});
test('reads legacy choices without modifying them, saves a new desktop schema and rejects stale or corrupt writes', async (t) => {
  const { dir, c, cmd } = await fixture(t);
  const p = initial();
  p.edgeHide = true;
  p.edges = ['left'];
  p.corners['top-right'] = 'show-desktop';
  const old = JSON.stringify({ format: 1, rev: 13, prefs: p });
  await writeFile(join(dir, 'settings.json'), old);
  const s = await c.request(null);
  assert.equal(s.settings.enabled, false);
  assert(s.settings.edgeHide.enabled);
  assert.deepEqual(s.settings.edgeHide.edges, ['left']);
  assert(s.settings.hotzones.find((z) => z.id === 'top-right').enabled);
  assert.deepEqual(await readdir(dir), ['settings.json']);
  const next = await cmd('save', {
    settings: {
      ...s.settings,
      windowDrag: { ...s.settings.windowDrag, enabled: true },
    },
  });
  assert.equal(next.rev, 1);
  assert.equal((await readPanel(dir)).settings.windowDrag.enabled, true);
  assert.equal(await readFile(join(dir, 'settings.json'), 'utf8'), old);
  await assert.rejects(
    c.request({
      session: s.session,
      rev: 0,
      action: 'save',
      settings: s.settings,
    }),
    /stale/,
  );
  await writeFile(join(dir, 'panel-settings.json'), 'bad');
  await assert.rejects(readPanel(dir), /unreadable/);
});
test('uses only saved desktop settings for IPC, explicit feature allowlist; never returns a token', async (t) => {
  const { c, cmd, calls } = await fixture(t);
  const base = await c.request(null);
  const s = {
    ...base.settings,
    enabled: true,
    windowDrag: { ...base.settings.windowDrag, enabled: true },
    topmostPin: { ...base.settings.topmostPin, enabled: true },
  };
  await cmd('save', { settings: s });
  await cmd('start');
  const first = await cmd('observe', { cursor: null });
  assert.equal(first.frames[0].type, 'helper.ready');
  await cmd('engine', {
    id: 'id',
    type: 'config.update',
    epoch: first.epoch,
    data: { revision: 4, config: { enabled: false } },
  });
  assert(calls.at(-1)[0].enabled);
  assert(calls.at(-1)[0].windowDrag.enabled);
  assert.equal(calls.at(-1)[1], 4);
  await assert.rejects(
    cmd('engine', { id: 'cmd', epoch: first.epoch, type: 'unknown', data: {} }),
    /invalid-command/,
  );
  await assert.rejects(
    cmd('engine', {
      id: 'cmd',
      epoch: 'previous',
      type: 'helper.ping',
      data: {},
    }),
  );
  assert(!Object.hasOwn(await c.request(null), 'token'));
  await cmd('stop');
  assert.equal((await c.request(null)).running, false);
});
test('view data persists within its profile, concurrent edits conflict; command actions are not silently accepted', async (t) => {
  const { c, cmd, dir } = await fixture(t);
  await cmd('view', {
    key: 'convenient-window-language',
    value: 'en-US',
    previous: null,
  });
  await assert.rejects(
    cmd('view', {
      key: 'convenient-window-language',
      value: 'zh-CN',
      previous: null,
    }),
    /stale/,
  );
  await assert.rejects(
    cmd('view', { key: 'unknown', value: 'x', previous: null }),
  );
  assert.equal(
    (await new PanelController(dir).request(null)).view[
      'convenient-window-language'
    ],
    'en-US',
  );
  const cfg = structuredClone(defaultSettings);
  cfg.mouseGestures.gestures[0].action = {
    kind: 'open-command',
    value: 'not run',
  };
  assert.deepEqual(configuredCommands(cfg), ['not run']);
  let allow = false;
  const prompts = [];
  c.helper.native = {
    call: async (op, data) => {
      assert.equal(op, 'confirm-commands');
      prompts.push(data);
      return allow;
    },
  };
  const before = await c.request(null);
  await assert.rejects(cmd('save', { settings: cfg }), /cancelled/);
  assert.deepEqual(await c.request(null), before);
  allow = true;
  await cmd('save', { settings: cfg });
  await cmd('save', { settings: cfg });
  assert.deepEqual(prompts, [['not run'], ['not run']]);
  await assert.rejects(
    cmd('native', { op: 'confirm-commands', data: [] }),
    /invalid-command/,
  );
});
test('bounded event history rejects obsolete epoch and cursor; owner-side hotkeys persist without the tab', async (t) => {
  const { c, calls, cmd } = await fixture(t);
  await cmd('view', {
    key: 'convenient-window-layouts',
    previous: null,
    value: JSON.stringify([{ id: 'desk', shortcut: 'Ctrl+1', windows: [] }]),
  });
  await cmd('save', {
    settings: { ...(await c.request(null)).settings, enabled: true },
  });
  await cmd('start');
  assert.equal(calls.at(-1).type, 'shortcut.hotkeys.sync');
  c.record({ type: 'shortcut.hotkey.triggered', data: { hotkeyId: 'desk' } });
  assert.equal(calls.at(-1).type, 'window.layout.restore');
  const { epoch, cursor } = await cmd('observe', { cursor: null });
  c.record({ type: 'helper.pong', data: {} });
  assert.equal(
    (await cmd('observe', { epoch, cursor })).frames[0].type,
    'helper.pong',
  );
  for (let i = 0; i < 140; i++) c.record({ type: 'helper.pong' });
  await assert.rejects(cmd('observe', { epoch, cursor }), /stream-expired/);
  await assert.rejects(cmd('observe', { epoch: 'old', cursor: null }));
});
test('requires owned listener and handshaken PID, protocol and ordinary privilege', () => {
  const ready = {
    processId: 12,
    version: assets.version,
    protocolVersion: 7,
    schemaVersion: 9,
    desktopManaged: true,
    elevated: false,
  };
  assert(matchesReady(ready, 12));
  for (const bad of [
    { processId: 3 },
    { elevated: true },
    { desktopManaged: false },
    { protocolVersion: 6 },
  ])
    assert(!matchesReady({ ...ready, ...bad }, 12));
  assert.deepEqual(
    [
      ...listenerPids(
        'TCP  127.0.0.1:56873 0.0.0.0:0 LISTENING 12\nTCP 127.0.0.1:56873 127.0.0.1:65500 ESTABLISHED 13',
      ),
    ],
    [12],
  );
});
test('requires both DSH authentication and a direct loopback request', () => {
  const req = {
    headers: { host: '127.0.0.1:1234', 'sec-fetch-site': 'same-origin' },
    socket: { remoteAddress: '::ffff:127.0.0.1', localAddress: '::1' },
  };
  const ok = { requestRejection: () => undefined };
  assert(permitted(req, ok));
  assert(!permitted(req, { requestRejection: () => 401 }));
  assert(
    !permitted(
      { ...req, socket: { ...req.socket, remoteAddress: '192.0.2.10' } },
      ok,
    ),
  );
  for (const headers of [
    { host: 'remote.example' },
    { 'sec-fetch-site': 'cross-site' },
    { 'x-forwarded-for': '' },
    { forwarded: 'for=localhost' },
  ])
    assert(!permitted({ ...req, headers: { ...req.headers, ...headers } }, ok));
});
test('loopback route rejects malformed requests, limits bodies and only exposes operator state', async (t) => {
  const { c } = await fixture(t);
  const server = createServer(
    handler(c, { requestRejection: () => undefined }),
  );
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(() => server.close());
  const url = `http://127.0.0.1:${server.address().port}/convenient-window/control`,
    hdr = {
      'content-type': 'application/json',
      'x-convenient-window': 'panel',
    };
  const get = await fetch(url);
  const v = await get.json();
  assert.equal(get.headers.get('cache-control'), 'no-store');
  assert(!Object.keys(v).some((k) => /token|path|pid/i.test(k)));
  for (const [method, body, headers, status] of [
    ['DELETE', undefined, {}, 405],
    ['POST', '{}', {}, 415],
    ['POST', 'null', hdr, 400],
    ['POST', '[', hdr, 400],
    ['POST', ' '.repeat(1024 * 1024 + 1), hdr, 413],
    ['POST', '{"action":"save"}', hdr, 409],
  ])
    assert.equal((await fetch(url, { method, body, headers })).status, status);
});
test('package and native asset versions agree without install hooks', () => {
  assert.equal(assets.platform, 'win32-x64');
  assert.equal(metadata.productVersion, assets.version);
  assert.equal(metadata.dependencies, undefined);
  for (const k of ['preinstall', 'install', 'postinstall', 'prepare'])
    assert.equal(metadata.scripts[k], undefined);
  assert.equal(metadata.version, '0.2.0-beta.2');
});

test('refuses missing, unexpected, or corrupt native assets before launch', async (t) => {
  const { verifyPayload } = await import('../host/assets.js');
  const dir = await mkdtemp(
    join(process.env.CW_TEST_TMP || tmpdir(), 'cw-asset-check-'),
  );
  t.after(() => rm(dir, { recursive: true, force: true }));
  await assert.rejects(verifyPayload(dir), /payload-invalid/);
  for (const f of assets.files)
    await writeFile(join(dir, f.name), 'not a release file');
  await assert.rejects(verifyPayload(dir), /payload-invalid/);
  await writeFile(join(dir, 'unexpected.dll'), 'extra');
  await assert.rejects(verifyPayload(dir), /payload-invalid/);
});

test('background restore honors saved power and scopes administrator requests to the registered login profile', async (t) => {
  const { c, helper, cmd, dir } = await fixture(t);
  let reg = { enabled: true, admin: true };
  const switches = [];
  helper.native = {
    lang: 'zh-CN',
    call: async (op) => {
      assert.equal(op, 'login');
      return reg;
    },
  };
  helper.elevation = async (enabled) => {
    switches.push(enabled);
    return { elevated: enabled };
  };
  await c.activate(dir, '1');
  assert.equal(helper.running, false);
  const settings = { ...(await c.request(null)).settings, enabled: true };
  await cmd('save', { settings });
  await c.activate(dir + '-other', '1');
  assert(helper.running);
  assert.deepEqual(switches, []);
  await cmd('stop');
  reg = { enabled: false, admin: true };
  await c.activate(dir, '1');
  assert.deepEqual(switches, []);
  await cmd('stop');
  reg = { enabled: true, admin: true };
  await c.activate(dir, '1');
  assert.deepEqual(switches, [true]);
});
test('missing or changed native service fails verification independently of the helper', async (t) => {
  const { verifyNative } = await import('../host/native-service.js');
  const { dir } = await fixture(t);
  await assert.rejects(verifyNative(dir), /payload-invalid/);
  await writeFile(join(dir, 'cw-dsh-native.exe'), 'bad');
  await writeFile(
    join(dir, 'native-identity.json'),
    JSON.stringify({
      name: 'cw-dsh-native.exe',
      bytes: 3,
      sha256: '0'.repeat(64),
    }),
  );
  await assert.rejects(verifyNative(dir), /payload-invalid/);
});
