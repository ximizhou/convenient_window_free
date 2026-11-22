import assert from 'node:assert/strict';
import { mkdtemp, readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { createServer } from 'node:net';
import { promisify } from 'node:util';
import { execFile, fork } from 'node:child_process';
import { once } from 'node:events';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { PanelController as Controller } from '../host/panel-controller.js';
import { verifyPayload } from '../host/assets.js';

// Explicit integration test, not an install hook. Do not disturb another host.
if (process.platform !== 'win32' || process.arch !== 'x64')
  throw Error('This check requires Windows x64.');
await verifyPayload();
const dir =
  process.argv[2] === '--owner-fixture'
    ? resolve(process.argv[3])
    : await mkdtemp(join(process.env.CW_TEST_TMP || tmpdir(), 'cw-native-'));
const ctl = new Controller(dir);
const snapshot = () => ctl.request(null);
async function command(action, args = {}) {
  const { session, rev } = await snapshot();
  return ctl.request({ action, session, rev, ...args });
}
const base = (await snapshot()).settings;
base.enabled = true;
base.hotzonesEnabled = false;
base.topmostPin.enabled = false;
await command('save', { settings: base });
if (process.argv[2] === '--owner-fixture') {
  await command('start');
  process.send({ pid: ctl.helper.child.pid });
} else {
  const occupied = createServer();
  try {
    try {
      await new Promise((ok, fail) =>
        occupied.once('error', fail).listen(56873, '127.0.0.1', ok),
      );
    } catch (error) {
      if (error.code === 'EADDRINUSE')
        throw Error(
          'Port in use; no process was touched. Stop the other helper yourself and retry.',
        );
      throw error;
    }
    await assert.rejects(command('start'), /helper-in-use/);
    assert(occupied.listening);
    assert.equal(ctl.helper.child, null);
  } finally {
    if (occupied.listening) await new Promise((ok) => occupied.close(ok));
  }
  try {
    const settings = structuredClone(base);
    settings.edgeHide.enabled = true;
    settings.edgeHide.edges = ['left'];
    settings.hoverDelayMs = 650;
    await command('save', { settings });
    await command('start');
    assert((await snapshot()).running);
    assert.equal((await snapshot()).elevated, false);
    const native = (op, data = null) =>
      command('native', { op, data }).catch((e) => {
        throw new Error(
          `Native acceptance failed at ${op}/${data?.action ?? ''}`,
          { cause: e },
        );
      });
    await native('language', 'en-US');
    let hudTitle;
    for (let i = 0; i < 100; i++) {
      ({ hudTitle } = await ctl.helper.native.call('status'));
      if (hudTitle === 'Volume / Brightness') break;
      await sleep(150);
    }
    assert.equal(
      hudTitle,
      'Volume / Brightness',
      'desktop HUD mounted outside the DSH panel',
    );
    const login = await native('login');
    const cancel = () =>
      promisify(execFile)(
        join(
          process.env.SystemRoot,
          'System32',
          'WindowsPowerShell',
          'v1.0',
          'powershell.exe',
        ),
        [
          '-NoProfile',
          '-NonInteractive',
          '-File',
          fileURLToPath(new URL('./cancel-test-dialog.ps1', import.meta.url)),
          '-ServiceId',
          String(ctl.helper.native.child.pid),
        ],
        { windowsHide: true, timeout: 25000 },
      );
    const [after] = await Promise.all([
      native('set-login', !login.enabled),
      cancel(),
    ]);
    assert.deepEqual(after, login);
    assert.deepEqual(await native('login'), login);
    const startup = await native('startup', { action: 'list' });
    assert.equal(startup.ok, true);
    assert(Array.isArray(startup.items));
    const item = startup.items.find(
      (item) => item.editable && typeof item.enabled === 'boolean',
    );
    if (item) {
      const [answer] = await Promise.all([
        native('startup', {
          action: 'setEnabled',
          id: item.id,
          revision: item.revision,
          enabled: !item.enabled,
        }),
        cancel(),
      ]);
      assert.equal(answer.error.code, 'cancelled');
      const next = (await native('startup', { action: 'list' })).items.find(
        (x) => x.id === item.id,
      );
      assert.equal(next.revision, item.revision);
      assert.equal(next.enabled, item.enabled);
    }
    const cfg = structuredClone((await snapshot()).settings);
    cfg.mouseGestures.gestures[0].action = {
      kind: 'open-command',
      value: 'notepad.exe',
    };
    const prev = (await snapshot()).rev;
    await Promise.all([
      assert.rejects(command('save', { settings: cfg }), /cancelled/),
      cancel(),
    ]);
    assert.equal((await snapshot()).rev, prev);
    const pid = ctl.helper.child.pid;
    const { epoch } = await command('observe', { cursor: null });
    const engine = (type, id, data) =>
      command('engine', { epoch, type, id, data });
    const applied = await engine('config.update', 'apply', { revision: 7 });
    assert.equal(applied.applied, 7);
    const config = JSON.parse(
      await readFile(join(dir, 'runtime', 'config.json'), 'utf8'),
    );
    assert.deepEqual(config.edgeHide.edges, ['left']);
    assert.equal(config.hoverDelayMs, 650);
    await engine('helper.ping', 'ping', {});
    await engine('window.layout.capture', 'capture', {});
    await engine('window.layout.restore', 'empty', { windows: [] });
    let types = new Set();
    for (let i = 0; i < 70; i++) {
      const batch = await command('observe', { epoch, cursor: 0 });
      for (const m of batch.frames) types.add(m.type);
      if (
        [
          'helper.pong',
          'window.layout.captured',
          'window.layout.restored',
        ].every((t) => types.has(t))
      )
        break;
      await sleep(100);
    }
    for (const t of [
      'helper.pong',
      'window.layout.captured',
      'window.layout.restored',
    ])
      assert(types.has(t), t);
    await command('stop');
    assert.equal(ctl.helper.child, null);
    assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    const again = new Controller(dir);
    assert.equal((await again.request(null)).running, false);
    assert.equal((await again.request(null)).settings.hoverDelayMs, 650);
    await again.dispose();
  } finally {
    await ctl.dispose();
  }
  // Disposing a loaded plugin and killing a whole DSH host are separate lifecycle paths.
  const owned = fork(
    fileURLToPath(import.meta.url),
    ['--owner-fixture', join(dir, 'owner')],
    { stdio: ['ignore', 'ignore', 'inherit', 'ipc'], windowsHide: true },
  );
  let timer;
  try {
    const { pid } = await Promise.race([
      once(owned, 'message').then(([v]) => v),
      once(owned, 'exit').then(() => {
        throw Error('Fixture exited before helper handshake');
      }),
      new Promise((_, no) => {
        timer = setTimeout(() => no(Error('Owner fixture timeout')), 35000);
      }),
    ]);
    assert(Number.isInteger(pid));
    const exited = once(owned, 'exit');
    owned.kill();
    await exited;
    let alive = true;
    for (let i = 0; i < 70 && alive; i++) {
      try {
        process.kill(pid, 0);
      } catch (e) {
        if (e.code !== 'ESRCH') throw e;
        alive = false;
      }
      if (alive) await sleep(100);
    }
    assert.equal(
      alive,
      false,
      'the native helper must exit with its exact owner',
    );
  } finally {
    clearTimeout(timer);
    if (owned.exitCode === null && !owned.killed) owned.kill();
  }
  console.log(
    'PASS: native HUD, read-only startup verification, cancelled native changes, owned handshake and shutdown.',
  );
}
