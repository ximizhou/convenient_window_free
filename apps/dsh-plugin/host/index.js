import { isAbsolute, join } from 'node:path';
import { readFile } from 'node:fs/promises';
import { PanelController } from './panel-controller.js';
import { metadata, root } from './assets.js';
export const name = metadata.name;
export const inject = ['webServer', 'connection', 'profileContext'];
export const ROUTE = '/convenient-window/control';
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
export function handler(ctl, connection) {
  return async (req, res) => {
    const abort = new AbortController();
    res.once('close', () => abort.abort());
    const send = (s, v) => {
      if (res.destroyed) return;
      res.writeHead(s, {
        'content-type': 'application/json; charset=utf-8',
        'cache-control': 'no-store',
        'x-content-type-options': 'nosniff',
      });
      res.end(JSON.stringify(v));
    };
    try {
      if (!permitted(req, connection))
        return send(403, { error: 'local-only' });
      if (req.method === 'GET') return send(200, await ctl.request(null));
      if (req.method !== 'POST') return send(405, { error: 'invalid-command' });
      if (
        req.headers['x-convenient-window'] !== 'panel' ||
        req.headers['content-type']?.split(';')[0] !== 'application/json'
      )
        return send(415, { error: 'invalid-command' });
      const chunks = [];
      let len = 0;
      for await (const chunk of req) {
        len += chunk.length;
        if (len > 1024 * 1024) return send(413, { error: 'invalid-settings' });
        chunks.push(chunk);
      }
      let body;
      try {
        body = JSON.parse(Buffer.concat(chunks).toString());
      } catch {
        return send(400, { error: 'invalid-command' });
      }
      if (!body || typeof body !== 'object' || Array.isArray(body))
        return send(400, { error: 'invalid-command' });
      return send(200, await ctl.request(body, abort.signal));
    } catch (e) {
      const known =
        /^(stale|local-only|invalid-(?:command|settings)|cancelled|entry-changed|pick-again|invalid-state|missing-(?:dsh|profile)|native-(?:busy|disconnected|failed)|admin[A-Z]\w+|settings-unreadable|helper-(?:in-use|disconnected|start-failed|timeout)|stream-expired|stream-busy|owner-unavailable|ordinary-host-required|incompatible-helper|payload-invalid|unsupported-platform|apply-failed|stop-failed|power-off|startup-unavailable|unloaded)$/;
      const code = known.test(e.message) ? e.message : 'host-failed';
      return send(
        code === 'stale' || code === 'helper-in-use'
          ? 409
          : code.startsWith('invalid-')
            ? 400
            : 503,
        { error: code },
      );
    }
  };
}
const files = {
  'panel.html': 'text/html; charset=utf-8',
  'panel.css': 'text/css; charset=utf-8',
  'panel.js': 'text/javascript; charset=utf-8',
  'app-icon.png': 'image/png',
};
export function apply(ctx) {
  const ctl = new PanelController(dataDir(ctx.profileContext));
  ctx.effect(() => {
    void ctl
      .activate(process.env.CW_DSH_LOGIN_DIR, process.env.CW_DSH_LOGIN_ADMIN)
      .catch((error) => {
        ctl.helper.fault = error.message;
      });
    return () => ctl.dispose();
  });
  ctx.effect(() =>
    ctx.webServer.register({
      kind: 'exact',
      path: ROUTE,
      handler: handler(ctl, ctx.connection),
    }),
  );
  for (const [file, mime] of Object.entries(files))
    ctx.effect(() =>
      ctx.webServer.register({
        kind: 'exact',
        path: '/convenient-window/' + file,
        handler: async (req, res) => {
          if (!permitted(req, ctx.connection) || req.method !== 'GET') {
            res.writeHead(403);
            res.end();
            return;
          }
          try {
            const bytes = await readFile(join(root, 'lib', file));
            res.writeHead(200, {
              'content-type': mime,
              'cache-control': 'no-store',
              'x-content-type-options': 'nosniff',
              'referrer-policy': 'no-referrer',
              'content-security-policy':
                "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'self'; form-action 'none'; base-uri 'none'",
            });
            res.end(bytes);
          } catch {
            res.writeHead(503);
            res.end();
          }
        },
      }),
    );
}
