import { readFile, lstat, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const root = fileURLToPath(new URL('../', import.meta.url));
export const assets = JSON.parse(
  await readFile(join(root, 'runtime-assets.json'), 'utf8'),
);
export const metadata = JSON.parse(
  await readFile(join(root, 'package.json'), 'utf8'),
);
// The release manifest is a pinned build input, not a remotely refreshable download policy.
export async function verifyPayload(dir = join(root, 'payload'), exact = true) {
  const names = assets.files.map((f) => f.name).sort();
  if (
    exact &&
    JSON.stringify((await readdir(dir)).sort()) !== JSON.stringify(names)
  )
    throw Error('payload-invalid');
  for (const f of assets.files) {
    const file = join(dir, f.name),
      stat = await lstat(file);
    if (
      !stat.isFile() ||
      stat.size !== f.bytes ||
      createHash('sha256')
        .update(await readFile(file))
        .digest('hex') !== f.sha256
    )
      throw Error('payload-invalid');
  }
}
