import assert from 'node:assert/strict';
import { mkdtemp, readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { createServer } from 'node:net';
import { fork } from 'node:child_process';
import { once } from 'node:events';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { Controller } from '../host/index.js';
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
async function command(action, prefs) {
  const { session, rev } = await snapshot();
  return ctl.request({ action, session, rev, prefs });
}
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
    const prefs = (await snapshot()).prefs;
    prefs.edgeHide = true;
    prefs.edges = ['left'];
    // Do not synthesize mouse input or minimize any real user window in this test.
    await command('save', prefs);
    assert.equal((await command('start')).running, true);
    const pid = ctl.helper.child.pid;
    prefs.hoverDelayMs = 650;
    const applied = await command('save', prefs);
    assert.equal(applied.running, true);
    const config = JSON.parse(
      await readFile(join(dir, 'runtime', 'config.json'), 'utf8'),
    );
    assert.equal(config.schemaVersion, 9);
    assert.deepEqual(config.edgeHide.edges, ['left']);
    assert.equal(config.hoverDelayMs, 650);
    await command('stop');
    assert.equal(ctl.helper.child, null);
    assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    const again = new Controller(dir);
    assert.equal((await again.request(null)).running, false);
    assert.equal((await again.request(null)).prefs.hoverDelayMs, 650);
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
    'PASS: occupied port protected, owned native handshake, applied revision, persisted settings, stop and owner-death cleanup.',
  );
}
