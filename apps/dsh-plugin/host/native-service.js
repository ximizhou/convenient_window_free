import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { readFile, lstat } from 'node:fs/promises';
import { createHash, randomUUID } from 'node:crypto';
import { join, dirname, basename } from 'node:path';
import { root } from './assets.js';

const lib = join(root, 'lib');
const file = 'cw-dsh-native.exe';
export async function verifyNative(dir = lib) {
  try {
    const identity = JSON.parse(
      await readFile(join(dir, 'native-identity.json'), 'utf8'),
    );
    const path = join(dir, file);
    const info = await lstat(path);
    if (
      identity.name === file &&
      info.isFile() &&
      info.size === identity.bytes &&
      createHash('sha256')
        .update(await readFile(path))
        .digest('hex') === identity.sha256
    )
      return identity;
  } catch {
    /* Missing or truncated bytes are treated alike. */
  }
  throw Error('payload-invalid');
}
function dshExecutable(path) {
  if (basename(path).toLowerCase() === 'deepseek harness.exe') return path;
  // The backend may use DSH's embedded Node process. Native code checks this sibling before registration.
  for (let dir = dirname(path); dir !== dirname(dir); dir = dirname(dir)) {
    if (basename(dir).toLowerCase() === 'resources')
      return join(dirname(dir), 'DeepSeek Harness.exe');
  }
  return path;
}
function errorCode(raw) {
  if (typeof raw !== 'string') return 'native-failed';
  if (
    /^(?:admin[A-Z]\w+|invalid-state|entry-changed|pick-again|cancelled|missing-dsh|missing-profile|native-busy|invalid-command)$/.test(
      raw,
    )
  )
    return raw;
  return /HELPER_INSTANCE_CONFLICT|实例占用/.test(raw)
    ? 'helper-in-use'
    : 'native-failed';
}
export class NativeService {
  constructor(dir) {
    this.dir = dir;
    this.lang = 'zh-CN';
    this.closed = false;
  }
  async open() {
    if (this.closed) throw Error('unloaded');
    if (this.session) return this.session;
    await verifyNative();
    if (this.closed) throw Error('unloaded');
    if (this.session) return this.session;
    const env = {
      ...process.env,
      CW_DSH_DATA: this.dir,
      CW_DSH_EXECUTABLE: dshExecutable(process.execPath),
      CW_DSH_LANG: this.lang,
    };
    delete env.ELECTRON_RUN_AS_NODE;
    const child = spawn(join(lib, file), ['--dsh-host'], {
      cwd: root,
      windowsHide: true,
      stdio: ['pipe', 'pipe', 'ignore'],
      env,
    });
    const session = { child, pending: new Map() };
    this.session = session;
    session.done = new Promise((resolve) => {
      const finish = () => {
        for (const p of session.pending.values())
          p.reject(
            Error(
              child.exitCode === 78
                ? 'ordinary-host-required'
                : 'native-disconnected',
            ),
          );
        session.pending.clear();
        if (this.session === session) {
          this.session = null;
          this.onExit?.(child.exitCode);
        }
        resolve();
      };
      child.once('close', finish);
      child.on('error', () => {}); // A spawn error also closes all three streams.
    });
    child.stdin.on('error', () => {});
    createInterface({ input: child.stdout, crlfDelay: Infinity }).on(
      'line',
      (line) => {
        let m;
        try {
          m = JSON.parse(line);
        } catch {
          return;
        }
        const p = session.pending.get(m.id);
        if (!p) return;
        session.pending.delete(m.id);
        if (m.ok) p.resolve(m.data);
        else p.reject(Error(errorCode(m.error)));
      },
    );
    return session;
  }
  get child() {
    return this.session?.child;
  }
  async call(op, data = null) {
    return this.send(await this.open(), op, data);
  }
  send(session, op, data = null) {
    if (this.closed || !session || session !== this.session)
      return Promise.reject(Error('native-disconnected'));
    if (session.pending.size >= 12) return Promise.reject(Error('native-busy'));
    const id = randomUUID();
    return new Promise((resolve, reject) => {
      session.pending.set(id, { resolve, reject });
      session.child.stdin.write(
        JSON.stringify({ id, op, data }) + '\n',
        (e) => {
          if (e && session.pending.delete(id))
            reject(Error('native-disconnected'));
        },
      );
    });
  }
  async close() {
    this.closed = true;
    const s = this.session;
    if (!s) return;
    for (const p of s.pending.values()) p.reject(Error('unloaded'));
    s.pending.clear();
    s.child.stdin.end();
    const timer = setTimeout(() => {
      if (this.session === s) s.child.kill();
    }, 6000);
    try {
      await s.done;
    } finally {
      clearTimeout(timer);
    }
  }
}
