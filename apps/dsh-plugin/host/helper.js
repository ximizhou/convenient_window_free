import { execFile } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { NativeService } from './native-service.js';

import { promisify } from 'node:util';
import { assets, verifyPayload } from './assets.js';
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
export function matchesReady(d, pid, elevated = false) {
  return (
    !!d &&
    d.processId === pid &&
    d.version === assets.version &&
    d.protocolVersion === 7 &&
    d.schemaVersion === 9 &&
    d.desktopManaged === true &&
    d.elevated === elevated
  );
}
export class Helper {
  constructor(dir) {
    this.dir = dir;
    this.native = new NativeService(dirname(dir));
    this.native.onExit = () => {
      if (this.child) this.fault = 'native-disconnected';
      this.invalidate();
    };
    this.child = this.ws = this.pending = this.fault = null;
    this.ready = false;
  }
  get running() {
    return !!(this.ready && this.ws?.readyState === 1 && this.child);
  }
  invalidate() {
    this.ready = false;
    this.child = null;
    const ws = this.ws;
    this.ws = null;
    ws?.close();
    this.pending?.fail('helper-disconnected');
    this.onEnd?.();
  }
  async start() {
    if (this.running) return { alreadyRunning: true, elevated: this.elevated };
    if (process.platform !== 'win32' || process.arch !== 'x64')
      throw Error('unsupported-platform');
    await verifyPayload().catch(() => {
      throw Error('payload-invalid');
    });
    await checkListener(null);
    const inert = await runtimeConfig(initial());
    inert.enabled = false;
    await atomicJSON(this.dir, 'config.json', inert);
    return this.launch('start', null);
  }
  async elevation(enabled) {
    if (!this.running) throw Error('helper-disconnected');
    this.invalidate();
    return this.launch('elevation', { enabled });
  }
  async launch(op, data) {
    const session = await this.native.open();
    let info;
    try {
      info = await this.native.send(session, op, data);
      if (!Number.isInteger(info.pid) || !(await checkListener(info.pid)))
        throw Error('owner-unavailable');
      const token = (
        await readFile(join(this.dir, 'auth-token'), 'utf8')
      ).trim();
      if (!/^[a-f0-9]{64}$/i.test(token)) throw Error('helper-start-failed');
      this.child = { pid: info.pid };
      this.elevated = !!info.elevated;
      this.fault = null;
      const ws = new WebSocket(`ws://127.0.0.1:${PORT}`, token);
      this.ws = ws;
      await new Promise((ok, no) => {
        const fail = () => {
          clearTimeout(timer);
          no(Error('helper-disconnected'));
        };
        const timer = setTimeout(() => no(Error('helper-timeout')), 7000);
        ws.addEventListener('error', fail);
        ws.addEventListener('close', () => {
          if (this.ws === ws) {
            this.fault = 'helper-disconnected';
            this.invalidate();
            void this.native.send(session, 'stop').catch(() => {});
          }
          fail();
        });
        ws.addEventListener('message', (event) => {
          if (this.ws !== ws) return;
          let m;
          try {
            m = JSON.parse(event.data);
          } catch {
            return;
          }
          if (!this.ready && m.type === 'helper.ready') {
            if (!matchesReady(m.data, info.pid, info.elevated)) {
              clearTimeout(timer);
              no(Error('incompatible-helper'));
              return;
            }
            clearTimeout(timer);
            this.ready = true;
            this.onFrame?.(m);
            ok();
            return;
          }
          if (!this.ready) return;
          this.onFrame?.(m);
          const p = this.pending;
          if (!p || m.data?.requestId !== p.id) return;
          if (m.type === 'config.applied' && m.data.revision === p.rev) p.ok();
          else if (m.type === 'runtime.error') p.fail('apply-failed');
        });
      });
      return info;
    } catch (e) {
      this.invalidate();
      await this.native.send(session, 'stop').catch(() => {});
      throw e;
    }
  }
  async apply(config, rev, labels) {
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
            data: {
              protocolVersion: 7,
              revision: rev,
              config,
              gestureLabels: labels,
            },
          }),
        );
      } catch {
        finish('helper-disconnected');
      }
    });
  }
  async stop() {
    this.invalidate();
    if (this.native.session)
      await this.native.send(this.native.session, 'stop');
    this.elevated = false;
    this.fault = null;
  }
  async dispose() {
    this.invalidate();
    await this.native.close();
  }
}
