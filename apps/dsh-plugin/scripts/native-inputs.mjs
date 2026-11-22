import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { root } from '../host/assets.js';
// A stale sidecar (especially its embedded HUD) must not pass a fresh frontend-only pack.
export async function nativeInputs() {
  const files = [
    'native/ui/hud.html',
    'native/ui/hud.js',
    'native/Cargo.toml',
    'native/Cargo.lock',
    'native/build.rs',
    'native/src/main.rs',
    'native/rust-toolchain.toml',
    'native/tauri.conf.json',
    'native/capabilities/adjustment-hud.json',
    '../../shared/windows_process.rs',
    ...[
      'supervisor',
      'windows_elevation',
      'windows_lifecycle',
      'command_broker',
      'windows_autostart',
      'startup_manager',
      'adjustment_hud',
    ].map((name) => '../desktop/src-tauri/src/' + name + '.rs'),
  ];
  const hash = createHash('sha256');
  for (const name of files) {
    const bytes = await readFile(join(root, name));
    hash.update(name + '\0' + bytes.length + '\0').update(bytes);
  }
  return hash.digest('hex');
}
