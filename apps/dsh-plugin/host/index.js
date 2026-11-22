import { isAbsolute, join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { Helper } from './helper.js';
import {
  initial,
  readPrefs,
  writePrefs,
  runtimeConfig,
  validate,
} from './settings.js';
import { metadata, assets } from './assets.js';

export const name = 'convenient-window';
export const inject = ['webServer', 'connection', 'profileContext'];
export const ROUTE = '/convenient-window/control';
const errors = new Set([
  'unsupported-platform',
  'payload-invalid',
  'settings-unreadable',
  'invalid-settings',
  'helper-in-use',
  'ordinary-host-required',
  'owner-unavailable',
  'helper-start-failed',
  'incompatible-helper',
  'helper-timeout',
  'helper-disconnected',
  'helper-exited',
  'apply-failed',
  'apply-busy',
  'stop-failed',
  'stale',
  'invalid-command',
  'unloaded',
  'settings-write-failed',
]);
const loopIP = (a) => ['127.0.0.1', '::1', '::ffff:127.0.0.1'].includes(a);
export function permitted(req, connection) {
  if (!loopIP(req.socket.remoteAddress) || !loopIP(req.socket.localAddress))
    return false;
  if (
    !/^(?:localhost|127\.0\.0\.1|\[::1\])(?::\d+)?$/i.test(
      req.headers.host || '',
    )
  )
    return false;
  if (
    Object.keys(req.headers).some(
      (h) => h === 'forwarded' || h.startsWith('x-forwarded-'),
    )
  )
    return false;
  if (
    req.headers['sec-fetch-site'] &&
    req.headers['sec-fetch-site'] !== 'same-origin'
  )
    return false;
  return connection.requestRejection(req) === undefined;
}
export function dataDir(profile) {
  if (!profile || typeof profile.dir !== 'string' || !isAbsolute(profile.dir))
    throw Error('Missing local DSH profile');
  return join(profile.dir, 'convenient-window'); // Never guess the default profile or another host's paths.
}
export class Controller {
  constructor(dir, helper = new Helper(join(dir, 'runtime'))) {
    this.dir = dir;
    this.helper = helper;
    this.session = randomUUID();
    this.queue = Promise.resolve();
    this.disposed = false;
  }
  serial(fn) {
    const p = this.queue.then(fn);
    this.queue = p.catch(() => {});
    return p;
  }
  snapshot(v) {
    return {
      ...v,
      session: this.session,
      running: !!this.helper.running,
      fault: this.helper.fault,
      product: metadata.productVersion,
      adapter: metadata.version,
      helper: assets.version,
    };
  }
  async current() {
    try {
      return await readPrefs(this.dir);
    } catch {
      return { rev: null, prefs: initial(), blocked: 'settings-unreadable' };
    }
  }
  request(body) {
    return this.serial(async () => {
      if (this.disposed) throw Error('unloaded');
      const v = await this.current();
      if (body === null) return this.snapshot(v);
      if (!body || !['start', 'stop', 'save'].includes(body.action))
        throw Error('invalid-command');
      if (body.session !== this.session) throw Error('stale');
      // Even a damaged settings file must not trap a running helper.
      if (body.action === 'stop') {
        await this.helper.stop();
        return this.snapshot(v);
      }
      if (v.blocked) throw Error(v.blocked);
      if (body.rev !== v.rev) throw Error('stale');
      const next =
        body.action === 'save'
          ? { rev: v.rev + 1, prefs: validate(body.prefs) }
          : v;
      if (body.action === 'save')
        await writePrefs(this.dir, next).catch(() => {
          throw Error('settings-write-failed');
        });
      if (body.action === 'start') await this.helper.start();
      if (this.helper.running) {
        try {
          await this.helper.apply(await runtimeConfig(next.prefs), next.rev);
        } catch (e) {
          await this.helper.stop();
          throw e;
        }
      }
      return this.snapshot(next);
    });
  }
  dispose() {
    this.disposed = true;
    return this.serial(() => this.helper.stop());
  }
}
export function handler(ctl, connection) {
  return async (req, res) => {
    const send = (status, data) => {
      res.writeHead(status, {
        'content-type': 'application/json; charset=utf-8',
        'cache-control': 'no-store',
        'x-content-type-options': 'nosniff',
      });
      res.end(JSON.stringify(data));
    };
    try {
      if (!permitted(req, connection))
        return send(403, { error: 'local-only' });
      let command = null;
      if (req.method === 'POST') {
        if (
          req.headers['x-convenient-window'] !== 'panel' ||
          req.headers['content-type']?.split(';')[0] !== 'application/json'
        )
          return send(415, { error: 'invalid-command' });
        let len = 0;
        const chunks = [];
        for await (const c of req) {
          len += c.length;
          if (len > 8192) return send(413, { error: 'invalid-command' });
          chunks.push(c);
        }
        try {
          command = JSON.parse(Buffer.concat(chunks).toString('utf8'));
        } catch {
          return send(400, { error: 'invalid-command' });
        }
        if (!command || typeof command !== 'object' || Array.isArray(command))
          return send(400, { error: 'invalid-command' });
      } else if (req.method !== 'GET')
        return send(405, { error: 'invalid-command' });
      return send(200, await ctl.request(command));
    } catch (e) {
      const error = errors.has(e.message) ? e.message : 'host-failed';
      return send(
        ['stale', 'helper-in-use'].includes(error)
          ? 409
          : error.startsWith('invalid-')
            ? 400
            : 503,
        { error },
      );
    }
  };
}
export function apply(ctx) {
  const ctl = new Controller(dataDir(ctx.profileContext));
  ctx.effect(() => () => ctl.dispose());
  ctx.effect(() =>
    ctx.webServer.register({
      kind: 'exact',
      path: ROUTE,
      handler: handler(ctl, ctx.connection),
    }),
  );
}
