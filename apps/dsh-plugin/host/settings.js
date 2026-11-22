import { readFile, writeFile, mkdir, rename, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { randomUUID } from 'node:crypto';
import { root } from './assets.js';

export const CORNERS = ['top-left', 'top-right', 'bottom-left', 'bottom-right'];
export const ACTIONS = ['none', 'show-desktop'];
export const EDGES = ['left', 'right', 'top', 'bottom'];
export function initial() {
  return {
    edgeHide: false,
    edges: [...EDGES],
    hoverDelayMs: 350,
    corners: Object.fromEntries(CORNERS.map((id) => [id, 'none'])),
  };
}
function exactKeys(x, keys) {
  return (
    x !== null &&
    typeof x === 'object' &&
    !Array.isArray(x) &&
    Object.keys(x).length === keys.length &&
    keys.every((k) => Object.hasOwn(x, k))
  );
}
export function validate(p) {
  if (
    !exactKeys(p, ['edgeHide', 'edges', 'hoverDelayMs', 'corners']) ||
    typeof p.edgeHide !== 'boolean' ||
    !Number.isInteger(p.hoverDelayMs) ||
    p.hoverDelayMs < 100 ||
    p.hoverDelayMs > 2000 ||
    !Array.isArray(p.edges) ||
    !p.edges.length ||
    p.edges.length > EDGES.length ||
    !p.edges.every((e) => EDGES.includes(e)) ||
    new Set(p.edges).size !== p.edges.length ||
    !exactKeys(p.corners, CORNERS) ||
    !CORNERS.every((c) => ACTIONS.includes(p.corners[c]))
  )
    throw Error('invalid-settings');
  return structuredClone(p);
}
export async function readPrefs(dir) {
  try {
    const v = JSON.parse(await readFile(join(dir, 'settings.json'), 'utf8'));
    if (v.format !== 1 || !Number.isSafeInteger(v.rev) || v.rev < 0)
      throw Error();
    return { rev: v.rev, prefs: validate(v.prefs) };
  } catch (e) {
    if (e.code === 'ENOENT') return { rev: 0, prefs: initial() };
    throw Error('settings-unreadable');
  }
}
export async function atomicJSON(dir, filename, value) {
  await mkdir(dir, { recursive: true });
  const tmp = join(dir, `.${filename}-${randomUUID()}.tmp`);
  try {
    await writeFile(tmp, JSON.stringify(value), { flag: 'wx', mode: 0o600 });
    await rename(tmp, join(dir, filename));
  } finally {
    await rm(tmp, { force: true }).catch(() => {});
  }
}
export function writePrefs(dir, v) {
  return atomicJSON(dir, 'settings.json', { format: 1, ...v });
}
export async function runtimeConfig(prefs) {
  const p = validate(prefs),
    config = JSON.parse(
      await readFile(join(root, 'lib', 'defaults.json'), 'utf8'),
    );
  if (config.schemaVersion !== 9) throw Error('incompatible-helper');
  config.enabled = true;
  config.edgeHide.enabled = p.edgeHide;
  config.edgeHide.edges = [...p.edges];
  config.hoverDelayMs = p.hoverDelayMs;
  // A change to a desktop default must not silently enable a feature this panel cannot control.
  for (const key of [
    'windowDrag',
    'topmostPin',
    'mouseGestures',
    'taskbarAppearance',
  ])
    config[key].enabled = false;
  config.hotzonesEnabled = CORNERS.some((id) => p.corners[id] !== 'none');
  for (const z of config.hotzones) {
    z.enabled = CORNERS.includes(z.id) && p.corners[z.id] !== 'none';
    for (const slot of z.actions) {
      slot.action = {
        kind: z.enabled && slot.trigger === 'hover' ? p.corners[z.id] : 'none',
      };
      slot.modifierActions = [];
      slot.hoverDelayMs = p.hoverDelayMs;
    }
  }
  return config;
}
