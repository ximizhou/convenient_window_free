import { execFile, spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { promisify } from 'node:util';
import { assets, root, verifyPayload } from './assets.js';
import { atomicJSON, initial, runtimeConfig } from './settings.js';

const exec = promisify(execFile);
const system = join(process.env.SystemRoot || 'C:\\Windows', 'System32');
const PORT = 56873; // Fixed by the shared engine; never probe its protocol to discover its owner.
const commandOptions = {
  windowsHide: true,
  timeout: 9500,
  maxBuffer: 8 * 1024 * 1024,
  encoding: 'utf8',
};

// Use numeric endpoints and PID, not localized status text or a WMI provider.
export function listenerPids(table, port = PORT) {
  const result = new Set();
  for (const line of table.split(/\r?\n/)) {
    const [protocol, local, remote, , pid] = line.trim().split(/\s+/);
    if (
      protocol !== 'TCP' ||
      !local.endsWith(`:${port}`) ||
      !['0.0.0.0:0', '[::]:0'].includes(remote)
    )
      continue;
    if (!/^\d+$/.test(pid)) throw Error('owner-unavailable');
    result.add(Number(pid));
  }
  return result;
}
async function checkListener(expectedPid) {
  let table;
  try {
    ({ stdout: table } = await exec(
      join(system, 'netstat.exe'),
      ['-ano'],
      commandOptions,
    ));
  } catch {
    throw Error('owner-unavailable');
  }
  const pids = listenerPids(table);
  if (!pids.size) return false;
  if (pids.size !== 1 || !pids.has(expectedPid)) throw Error('helper-in-use');
  return true;
}
async function ownerBirth() {
  const script = `$ErrorActionPreference='Stop'; $p=[Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent()); if($p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){'elevated'}else{(Get-Process -Id ${process.pid}).StartTime.ToUniversalTime().ToFileTimeUtc().ToString([Globalization.CultureInfo]::InvariantCulture)}`;
  let value;
  try {
    const { stdout } = await exec(
      join(system, 'WindowsPowerShell', 'v1.0', 'powershell.exe'),
      ['-NoProfile', '-NonInteractive', '-Command', script],
      commandOptions,
    );
    value = stdout.trim();
  } catch {
    throw Error('owner-unavailable');
  }
  if (value === 'elevated') throw Error('ordinary-host-required');
  if (!/^\d{16,20}$/.test(value)) throw Error('owner-unavailable');
  return value; // FILETIME is a 64-bit integer: pass decimal text without rounding.
}
export function matchesReady(d, pid) {
  return (
    !!d &&
    d.processId === pid &&
    d.version === assets.version &&
    d.protocolVersion === 7 &&
    d.schemaVersion === 9 &&
    d.desktopManaged === true &&
    d.elevated === false
  );
}
function endWithin(done, ms) {
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(false), ms);
    done.then(() => {
      clearTimeout(timer);
      resolve(true);
    });
  });
}
export class Helper {
  constructor(dir) {
    this.dir = dir;
    this.child = this.ws = this.pending = this.fault = null;
    this.ready = this.stopping = false;
  }
  get running() {
    return this.ready && this.ws?.readyState === 1 && !!this.child;
  }
  async start() {
    if (this.running) return;
    if (this.child) await this.stop();
    if (process.platform !== 'win32' || process.arch !== 'x64')
      throw Error('unsupported-platform');
    await verifyPayload().catch(() => {
      throw Error('payload-invalid');
    });
    await checkListener(null);
    const birth = await ownerBirth();
    // The helper reads its last snapshot before IPC is ready. Start inert, even after a previous crash.
    // Only touch our private runtime directory; never modify an existing host's configuration.
    const inert = await runtimeConfig(initial());
    inert.enabled = false;
    await atomicJSON(this.dir, 'config.json', inert);
    const dir = join(root, 'payload');
    const child = spawn(
      join(dir, 'magic-corners-helper.exe'),
      ['--data-dir', this.dir, '--desktop-owner', String(process.pid), birth],
      { cwd: dir, windowsHide: true, stdio: 'ignore', shell: false },
    );
    this.child = child;
    this.stopping = this.ready = false;
    this.fault = null;
    this.done = new Promise((resolve) => {
      const finish = () => {
        if (this.child === child) {
          this.child = null;
          this.ready = false;
          if (!this.stopping) this.fault = 'helper-exited';
          this.pending?.fail('helper-disconnected');
          this.ws?.close();
        }
        resolve();
      };
      child.once('exit', finish);
      child.once('error', finish);
    });
    try {
      const deadline = Date.now() + 18000;
      let owned = false;
      while (this.child === child && Date.now() < deadline) {
        if (await checkListener(child.pid)) {
          owned = true;
          break;
        }
        await sleep(150);
      }
      if (!owned || this.child !== child) throw Error('helper-start-failed');
      const token = (
        await readFile(join(this.dir, 'auth-token'), 'utf8')
      ).trim();
      if (!/^[0-9a-f]{64}$/i.test(token) || this.child !== child)
        throw Error('helper-start-failed');
      const ws = new WebSocket(`ws://127.0.0.1:${PORT}`, token);
      this.ws = ws;
      await new Promise((resolve, reject) => {
        const fail = (code) => {
          clearTimeout(timer);
          reject(Error(code));
        };
        const timer = setTimeout(() => fail('helper-timeout'), 7000);
        ws.addEventListener('error', () => fail('helper-disconnected'));
        ws.addEventListener('close', () => {
          if (this.ws === ws) {
            this.ready = false;
            this.pending?.fail('helper-disconnected');
            if (!this.stopping && this.child === child) {
              this.fault = 'helper-disconnected';
              child.kill();
            }
          }
          fail('helper-disconnected');
        });
        ws.addEventListener('message', (event) => {
          if (this.ws !== ws || this.child !== child) return;
          let m;
          try {
            m = JSON.parse(event.data);
          } catch {
            return;
          }
          if (m.type === 'helper.ready' && !this.ready) {
            if (!matchesReady(m.data, child.pid))
              return fail('incompatible-helper');
            clearTimeout(timer);
            this.ready = true;
            resolve();
            return;
          }
          const p = this.pending;
          if (!this.ready || !p || m.data?.requestId !== p.id) return;
          if (m.type === 'config.applied' && m.data.revision === p.rev) p.ok();
          else if (m.type === 'runtime.error') p.fail('apply-failed');
        });
      });
    } catch (error) {
      await this.stop();
      throw error;
    }
  }
  async apply(config, rev) {
    if (!this.running) throw Error('helper-disconnected');
    if (this.pending) throw Error('apply-busy');
    const id = randomUUID();
    return new Promise((resolve, reject) => {
      const finish = (code) => {
        if (this.pending?.id !== id) return;
        clearTimeout(timer);
        this.pending = null;
        code ? reject(Error(code)) : resolve();
      };
      const timer = setTimeout(() => finish('helper-timeout'), 5500);
      this.pending = { id, rev, ok: () => finish(), fail: finish };
      try {
        this.ws.send(
          JSON.stringify({
            id,
            type: 'config.update',
            time: Date.now(),
            data: { protocolVersion: 7, revision: rev, config },
          }),
        );
      } catch {
        finish('helper-disconnected');
      }
    });
  }
  async stop() {
    this.stopping = true;
    const child = this.child,
      done = this.done,
      ws = this.ws,
      ownedReady = this.running;
    this.ready = false;
    this.pending?.fail('helper-disconnected');
    if (child) {
      if (ownedReady) {
        try {
          ws.send(
            JSON.stringify({
              id: randomUUID(),
              type: 'helper.stop',
              time: Date.now(),
              data: {},
            }),
          );
        } catch {}
      }
      if (!ownedReady || !(await endWithin(done, 4000))) {
        if (this.child === child) child.kill(); // ChildProcess handle, never a discovered PID or process name.
        if (!(await endWithin(done, 3500))) throw Error('stop-failed');
      }
    }
    this.ws = this.fault = null;
    ws?.close();
  }
}
