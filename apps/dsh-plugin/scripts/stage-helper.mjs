import { resolve, join } from 'node:path';
import { mkdir, copyFile } from 'node:fs/promises';
import { root, assets, verifyPayload } from '../host/assets.js';
const from = process.argv[2] || process.env.CW_HELPER_ASSETS;
if (!from)
  throw Error('Provide the directory of the matching official helper release.');
await verifyPayload(resolve(from), false);
const to = join(root, 'payload');
await mkdir(to, { recursive: true });
for (const { name } of assets.files)
  await copyFile(join(resolve(from), name), join(to, name));
await verifyPayload(to);
console.log(`Verified ${assets.files.length} helper ${assets.version} files.`);
