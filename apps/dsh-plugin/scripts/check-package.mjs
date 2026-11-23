import { Script } from 'node:vm';
import { readFile, access } from 'node:fs/promises';
import { join } from 'node:path';
import { root, verifyPayload, metadata } from '../host/assets.js';
const { verifyNative } = await import('../host/native-service.js');
const patch = await readFile(join(root, 'cordis.patch.yml'), 'utf8');
if (
  !patch.split(/\r?\n/).some((line) => line.trim() === `name: ${metadata.name}`)
)
  throw Error('DSH patch package name must match the published npm name.');
await verifyPayload();
const identity = await verifyNative();
const { nativeInputs } = await import('./native-inputs.mjs');
const { createHash } = await import('node:crypto');
if (
  identity.inputs !== (await nativeInputs()) ||
  identity.notices !==
    createHash('sha256')
      .update(await readFile(join(root, 'lib/NATIVE-NOTICES.txt')))
      .digest('hex')
)
  throw Error('Rebuild the DSH native host and notices before packing.');
new Script(await readFile(join(root, 'lib/panel.js'), 'utf8'));
new Script(await readFile(join(root, 'lib/client.js'), 'utf8')); // Syntax only: no evaluation or simulated DSH.
await access(join(root, 'LICENSE'));
for (const f of [
  'lib/client.js',
  'lib/panel.js',
  'lib/panel.css',
  'lib/panel.html',
  'lib/app-icon.png',
  'lib/model.mjs',
  'host/panel-controller.js',
  'host/native-service.js',
  'lib/NATIVE-NOTICES.txt',
  'lib/defaults.json',
  'lib/THIRD-PARTY.txt',
  'host/index.js',
  'cordis.patch.yml',
  'README.md',
])
  await access(join(root, f));
if (
  metadata.dependencies ||
  metadata.optionalDependencies ||
  ['install', 'preinstall', 'postinstall', 'prepare'].some(
    (k) => k in metadata.scripts,
  )
)
  throw Error('Install must not require extra dependencies or scripts.');
if (
  !String(await readFile(join(root, 'lib/client.js'))).includes(
    'sidebar.panellist',
  )
)
  throw Error('DSH sidebar entry missing');
console.log(
  'Package: bundled payload verified; no install dependencies or hooks.',
);
