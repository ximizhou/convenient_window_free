import {
  copyFile,
  readFile,
  writeFile,
  mkdir,
  readdir,
} from 'node:fs/promises';
import { resolve, join, dirname } from 'node:path';
import { build } from 'esbuild';
import { compile } from 'svelte/compiler';
import { root, metadata, assets } from '../host/assets.js';

const lib = join(root, 'lib');
await mkdir(lib, { recursive: true });
await copyFile(resolve(root, '../../LICENSE'), join(root, 'LICENSE'));
const product = JSON.parse(
  await readFile(resolve(root, '../../package.json'), 'utf8'),
);
if (
  metadata.productVersion !== product.version ||
  assets.version !== metadata.productVersion
)
  throw Error('Review product/helper identities before building.');
// Import only portable settings defaults. The distributed DSH runtime never imports Tauri.
const defaults = await build({
  entryPoints: [resolve(root, '../desktop/src/settings-store.ts')],
  tsconfigRaw: { compilerOptions: { target: 'ES2022' } },
  bundle: true,
  write: false,
  platform: 'node',
  format: 'esm',
});
const { defaultSettings } = await import(
  'data:text/javascript;base64,' +
    Buffer.from(defaults.outputFiles[0].contents).toString('base64')
);
if (defaultSettings.schemaVersion !== 9)
  throw Error('Review the adapter for the new settings schema.');
await writeFile(
  join(lib, 'defaults.json'),
  JSON.stringify(defaultSettings) + '\n',
);
const result = await build({
  absWorkingDir: root,
  entryPoints: ['src/client.js'],
  outfile: 'lib/client.js',
  bundle: true,
  platform: 'browser',
  target: 'es2022',
  format: 'cjs',
  minify: true,
  metafile: true,
  conditions: ['svelte', 'browser', 'production'],
  external: ['react'],
  define: { 'process.env.NODE_ENV': '"production"' },
  banner: {
    js: `window.__ModuleLoader__.load({id:${JSON.stringify(metadata.name)},factory:(require)=>{var module={exports:{}};var exports=module.exports;`,
  },
  footer: { js: 'return module.exports;}});' },
  plugins: [
    {
      name: 'panel-svelte',
      setup(b) {
        b.onLoad({ filter: /\.svelte$/ }, async ({ path }) => {
          const { js, warnings } = compile(await readFile(path, 'utf8'), {
            filename: path,
            generate: 'client',
            css: 'injected',
            dev: false,
          });
          if (warnings.length)
            throw Error(warnings.map((w) => w.message).join('\n'));
          return { contents: js.code, loader: 'js', resolveDir: dirname(path) };
        });
      },
    },
  ],
});
const modules = new Set(
  Object.entries(result.metafile.outputs['lib/client.js'].inputs)
    .filter(([, v]) => v.bytesInOutput > 0)
    .map(([path]) => path.match(/^node_modules\/((?:@[^/]+\/)?[^/]+)\//)?.[1])
    .filter(Boolean),
);
const notices = [];
for (const mod of [...modules].sort()) {
  const dir = join(root, 'node_modules', mod),
    license = (await readdir(dir)).find((f) =>
      /^licen[cs]e(?:\.md|\.txt)?$/i.test(f),
    );
  if (!license) throw Error(`Missing bundled license for ${mod}`);
  notices.push(`${mod}\n${await readFile(join(dir, license), 'utf8')}`);
}
await writeFile(join(lib, 'THIRD-PARTY.txt'), notices.join('\n\n'));
