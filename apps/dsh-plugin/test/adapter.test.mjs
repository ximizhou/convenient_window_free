import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { once } from 'node:events';
import { Controller, dataDir, handler, permitted } from '../host/index.js';
import { initial, runtimeConfig, validate } from '../host/settings.js';
import { listenerPids, matchesReady } from '../host/helper.js';
import { assets, metadata } from '../host/assets.js';
import { zh, en } from '../src/copy.js';

async function fixture(t) {
  const dir = await mkdtemp(
    join(process.env.CW_TEST_TMP || tmpdir(), 'cw-unit-'),
  );
  t.after(() => rm(dir, { recursive: true, force: true }));
  const calls = [],
    helper = {
      running: false,
      fault: null,
      start() {
        calls.push('start');
        this.running = true;
      },
      apply(c, r) {
        calls.push(['apply', c, r]);
      },
      stop() {
        calls.push('stop');
        this.running = false;
      },
    };
  const c = new Controller(dir, helper);
  const state = () => c.request(null);
  const cmd = async (action, prefs) => {
    const { rev, session } = await state();
    return c.request({ action, prefs, rev, session });
  };
  return { dir, helper, c, calls, state, cmd };
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
test('reads are side-effect free; saves are atomic and profile-local; restarting does not auto-enable', async (t) => {
  const a = await fixture(t),
    b = await fixture(t);
  const p = initial();
  p.edgeHide = true;
  assert.deepEqual(await readdir(a.dir), []);
  await a.cmd('save', p);
  assert.deepEqual(await readdir(a.dir), ['settings.json']);
  assert.equal((await b.state()).prefs.edgeHide, false);
  assert.deepEqual(a.calls, []);
  await a.cmd('start');
  await a.c.dispose();
  const next = new Controller(a.dir, b.helper);
  assert.equal((await next.request(null)).prefs.edgeHide, true);
  assert.equal((await next.request(null)).running, false);
});
test('stale edits and previous plugin sessions cannot overwrite settings', async (t) => {
  const a = await fixture(t),
    base = await a.state(),
    edit = {
      action: 'save',
      session: base.session,
      rev: base.rev,
      prefs: base.prefs,
    };
  const r = await Promise.allSettled([a.c.request(edit), a.c.request(edit)]);
  assert.deepEqual(
    r.map((x) => x.status),
    ['fulfilled', 'rejected'],
  );
  assert.match(r[1].reason.message, /stale/);
  await assert.rejects(
    a.c.request({ ...edit, session: 'old', rev: 1 }),
    /stale/,
  );
  assert.equal((await a.state()).rev, 1);
});
test('waits for the running helper acknowledgement, stops on failure and on disposal', async (t) => {
  const a = await fixture(t);
  await a.cmd('start');
  let release, entered;
  const inApply = new Promise((r) => (entered = r));
  a.helper.apply = () => {
    entered();
    return new Promise((r) => (release = r));
  };
  let finished = false;
  const write = a.cmd('save', initial()).then(() => (finished = true));
  await inApply;
  assert.equal(finished, false);
  release();
  await write;
  a.helper.apply = () => {
    throw Error('apply-failed');
  };
  await assert.rejects(a.cmd('save', initial()), /apply-failed/);
  assert.equal(a.helper.running, false);
  await a.c.dispose();
  await assert.rejects(a.state(), /unloaded/);
});
test('does not overwrite corrupt settings and still allows stopping an owned helper', async (t) => {
  const a = await fixture(t);
  await a.cmd('start');
  await writeFile(join(a.dir, 'settings.json'), 'broken');
  assert.equal((await a.state()).blocked, 'settings-unreadable');
  await assert.rejects(a.cmd('save', initial()), /settings-unreadable/);
  assert.equal((await a.cmd('stop')).running, false);
  assert.equal(await readFile(join(a.dir, 'settings.json'), 'utf8'), 'broken');
});
test('listener ownership is numeric, locale independent, and ignores established connections', () => {
  const s =
    ' TCP 127.0.0.1:56873 0.0.0.0:0 侦听 123\r\n TCP [::]:56873 [::]:0 LISTENING 22\n TCP 127.0.0.1:56873 127.0.0.1:35678 ESTABLISHED 123\n UDP 0.0.0.0:56873 *:* 52';
  assert.deepEqual([...listenerPids(s)], [123, 22]);
  const d = {
    processId: 5,
    version: assets.version,
    protocolVersion: 7,
    schemaVersion: 9,
    elevated: false,
    desktopManaged: true,
  };
  assert(matchesReady(d, 5));
  for (const v of [
    { processId: 3 },
    { version: '0' },
    { protocolVersion: 8 },
    { schemaVersion: 10 },
    { elevated: true },
    { desktopManaged: false },
  ])
    assert.equal(matchesReady({ ...d, ...v }, 5), false);
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
    ['POST', ' '.repeat(9000), hdr, 413],
    ['POST', '{"action":"save"}', hdr, 409],
  ])
    assert.equal((await fetch(url, { method, body, headers })).status, status);
});
test('install metadata and both dictionaries agree with the shipped surface', () => {
  assert.equal(assets.platform, 'win32-x64');
  assert.equal(metadata.productVersion, assets.version);
  assert.equal(metadata.dependencies, undefined);
  for (const k of ['preinstall', 'install', 'postinstall', 'prepare'])
    assert.equal(metadata.scripts[k], undefined);
  assert.deepEqual(Object.keys(zh).sort(), Object.keys(en).sort());
  assert.deepEqual(Object.keys(zh.error).sort(), Object.keys(en.error).sort());
  assert(!/[\u3400-\u9fff]/.test(JSON.stringify({ ...en, lang: '' })));
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
