import { execFileSync } from 'node:child_process';
import { mkdir, copyFile, readFile, writeFile, stat } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { homedir } from 'node:os';
import { nativeInputs } from './native-inputs.mjs';
import { root } from '../host/assets.js';
const native = join(root, 'native'),
  lib = join(root, 'lib');
if (process.platform !== 'win32' || process.arch !== 'x64')
  throw Error('Build the DSH native host on Windows x64.');
const notices = resolve(
  process.argv[2] || join(root, '../../target/THIRD-PARTY-NOTICES.txt'),
);
if ((await stat(notices)).size < 100000)
  throw Error(
    'Generate third-party notices for the matching desktop native dependency lock.',
  );
const desktopLock = await readFile(
  join(root, '../desktop/src-tauri/Cargo.lock'),
  'utf8',
);
if (
  (await readFile(join(native, 'Cargo.lock'), 'utf8'))
    .replace('name = "cw-dsh-native"', 'name = "convenient-window"')
    .replaceAll('\r\n', '\n') !== desktopLock.replaceAll('\r\n', '\n')
)
  throw Error(
    'Review third-party notices for the changed native dependency lock.',
  );
// Let the Svelte compiler and esbuild release their memory before starting Rust.
execFileSync(process.execPath, [join(root, 'scripts/build.mjs')], {
  cwd: root,
  stdio: 'inherit',
  windowsHide: true,
});
const target = resolve(process.env.CARGO_TARGET_DIR || join(native, 'target'));
const env = { ...process.env, CARGO_TARGET_DIR: target };
const paths = [
  [target, '/build'],
  [resolve(root, '../..'), '/source'],
  [resolve(env.CARGO_HOME || join(homedir(), '.cargo')), '/cargo'],
  [resolve(env.RUSTUP_HOME || join(homedir(), '.rustup')), '/toolchain'],
];
const remap = paths.flatMap(([path, alias]) => [
  '--remap-path-prefix',
  path + '=' + alias,
]);
if (env.RUSTFLAGS && !env.CARGO_ENCODED_RUSTFLAGS)
  throw Error(
    'Pass custom Rust flags through CARGO_ENCODED_RUSTFLAGS for this build.',
  );
env.CARGO_ENCODED_RUSTFLAGS = [env.CARGO_ENCODED_RUSTFLAGS, ...remap]
  .filter(Boolean)
  .join('\x1f');
execFileSync(
  'cargo',
  ['build', '--release', '--locked', '--features', 'tauri/custom-protocol'],
  { cwd: native, env, stdio: 'inherit', windowsHide: true },
);
const exe = join(lib, 'cw-dsh-native.exe');
await mkdir(lib, { recursive: true });
await copyFile(join(target, 'release', 'cw-dsh-native.exe'), exe);
const bin = await readFile(exe);
await writeFile(
  join(lib, 'native-identity.json'),
  JSON.stringify({
    name: 'cw-dsh-native.exe',
    sha256: createHash('sha256').update(bin).digest('hex'),
    bytes: bin.length,
    inputs: await nativeInputs(),
    notices: createHash('sha256')
      .update(await readFile(notices))
      .digest('hex'),
  }) + '\n',
);
await copyFile(notices, join(lib, 'NATIVE-NOTICES.txt'));
