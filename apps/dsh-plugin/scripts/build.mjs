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
const lib = join(root, 'lib'),
  desktop = resolve(root, '../desktop');
await mkdir(lib, { recursive: true });
for (const [src, dst] of [
  [resolve(root, '../../LICENSE'), join(root, 'LICENSE')],
  [join(desktop, 'public/app-icon.png'), join(lib, 'app-icon.png')],
  [join(root, 'src/panel.html'), join(lib, 'panel.html')],
])
  await copyFile(src, dst);
const product = JSON.parse(
  await readFile(resolve(root, '../../package.json'), 'utf8'),
);
if (
  product.version !== metadata.productVersion ||
  assets.version !== product.version
)
  throw Error('Review product/helper identities.');
const base = {
  absWorkingDir: root,
  bundle: true,
  tsconfigRaw: {
    compilerOptions: { target: 'ES2022', verbatimModuleSyntax: true },
  },
  target: 'es2022',
  metafile: true,
};
const model = await build({
  ...base,
  entryPoints: ['src/model.ts'],
  outfile: 'lib/model.mjs',
  platform: 'node',
  format: 'esm',
});
const { defaultSettings } = await import('../lib/model.mjs');
if (defaultSettings.schemaVersion !== 9)
  throw Error('Review the settings schema.');
await writeFile(
  join(lib, 'defaults.json'),
  JSON.stringify(defaultSettings) + '\n',
);
const client = await build({
  ...base,
  entryPoints: ['src/client.js'],
  outfile: 'lib/client.js',
  format: 'cjs',
  minify: true,
  external: ['react'],
  banner: {
    js: `window.__ModuleLoader__.load({id:${JSON.stringify(metadata.name)},factory:(require)=>{var module={exports:{}};var exports=module.exports;`,
  },
  footer: { js: 'return module.exports;}});' },
});
const panel = await build({
  ...base,
  entryPoints: ['src/panel.ts'],
  outfile: 'lib/panel.js',
  platform: 'browser',
  format: 'iife',
  minify: true,
  conditions: ['svelte', 'browser', 'production'],
  define: { 'process.env.NODE_ENV': '"production"' },
  inject: [join(root, 'src/globals.js')],
  plugins: [
    {
      name: 'shared-desktop',
      setup(b) {
        b.onResolve({ filter: /^\.\/diagnostics$/ }, (args) =>
          args.importer === join(desktop, 'src', 'App.svelte')
            ? { path: join(root, 'src', 'diagnostics.ts') }
            : undefined,
        );
        b.onResolve({ filter: /^svelte(?:\/|$)/ }, (args) =>
          args.pluginData?.canonical
            ? undefined
            : b.resolve(args.path, {
                kind: args.kind,
                resolveDir: root,
                pluginData: { canonical: true },
              }),
        );
        b.onLoad({ filter: /\.svelte$/ }, async ({ path }) => {
          const { js } = compile(await readFile(path, 'utf8'), {
            filename: path,
            generate: 'client',
            css: 'injected',
            dev: false,
          });
          return { contents: js.code, loader: 'js', resolveDir: dirname(path) };
        });
      },
    },
  ],
});
const inputs = Object.keys(panel.metafile.inputs);
for (const file of [
  'App.svelte',
  'styles.css',
  'WindowLayouts.svelte',
  'StartupManager.svelte',
  'TaskbarAppearance.svelte',
  'helper-client.ts',
  'i18n.ts',
])
  if (!inputs.includes('../desktop/src/' + file))
    throw Error('Missing real desktop source: ' + file);
if (inputs.some((k) => k.includes('@tauri-apps/')))
  throw Error('Native Tauri APIs must not ship in DSH renderer');
const nativeUI = join(root, 'native', 'ui');
await mkdir(nativeUI, { recursive: true });
await writeFile(
  join(nativeUI, 'hud.html'),
  '<!doctype html><html><head><meta charset="utf-8"><style>html,body{margin:0;background:#181b21}</style><script defer src="hud.js"></script></head><body><div id="app"></div></body></html>',
);
const hud = await build({
  ...base,
  entryPoints: ['src/native-hud.ts'],
  outfile: 'native/ui/hud.js',
  format: 'iife',
  platform: 'browser',
  minify: true,
  conditions: ['svelte', 'browser', 'production'],
  define: { 'process.env.NODE_ENV': '"production"' },
  plugins: [
    {
      name: 'native-hud',
      setup(b) {
        b.onResolve({ filter: /^(?:svelte|@tauri-apps\/api)(?:\/|$)/ }, (a) =>
          a.pluginData
            ? undefined
            : b.resolve(a.path, {
                resolveDir: root,
                kind: a.kind,
                pluginData: { canonical: true },
              }),
        );
        b.onLoad({ filter: /\.svelte$/ }, async ({ path }) => ({
          contents: compile(await readFile(path, 'utf8'), {
            filename: path,
            generate: 'client',
            css: 'injected',
          }).js.code,
          resolveDir: dirname(path),
        }));
      },
    },
  ],
});
const modules = new Set();
for (const result of [model, client, panel, hud])
  for (const item of Object.values(result.metafile.outputs))
    for (const [path, v] of Object.entries(item.inputs)) {
      const match = path.match(/(?:^|\/)node_modules\/((?:@[^/]+\/)?[^/]+)\//);
      if (v.bytesInOutput && match) modules.add(match[1]);
    }
const notices = [];
for (const mod of [...modules].sort()) {
  const dir = join(root, 'node_modules', mod);
  const file = (await readdir(dir)).find((n) =>
    /^licen[cs]e(?:_MIT|\.md|\.txt)?$/i.test(n),
  );
  if (!file) throw Error('Missing license: ' + mod);
  notices.push(`${mod}\n${await readFile(join(dir, file), 'utf8')}`);
}
await writeFile(join(lib, 'THIRD-PARTY.txt'), notices.join('\n\n'));
