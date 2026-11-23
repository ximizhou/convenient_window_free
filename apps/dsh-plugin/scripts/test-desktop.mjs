// Opt-in Windows acceptance test. It operates only its own DSH profile, shell and helper.
// Real drag-to-edge and show-desktop observations are deliberately left to a human.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { setTimeout as wait } from 'node:timers/promises';
import { _electron } from 'playwright-core';
import { assets, metadata, verifyPayload } from '../host/assets.js';

const [appPath, packagePath, oldPackagePath] = process.argv.slice(2);
if (process.platform !== 'win32' || !appPath || !packagePath)
  throw Error(
    'Provide the installed Windows DSH executable and a prebuilt .tgz.',
  );
const exe = resolve(appPath),
  tgz = resolve(packagePath);
assert((await stat(exe)).isFile());
assert.equal(basename(tgz), `${metadata.name}-${metadata.version}.tgz`);
const qa = await mkdtemp(join(process.env.CW_TEST_TMP || tmpdir(), 'cw-dsh-'));
const home = join(qa, 'home'),
  shell = join(qa, 'electron'),
  profile = join(home, 'profiles', 'desktop');
const env = {
  ...process.env,
  DSH_HOME: home,
  HOME: qa,
  USERPROFILE: qa,
  APPDATA: join(qa, 'roaming'),
  LOCALAPPDATA: join(qa, 'local'),
  DSH_TELEMETRY_DISABLED: '1',
  DSH_DESKTOP_UPDATE_JOURNAL_DIR: join(qa, 'updates'),
  HTTP_PROXY: 'http://127.0.0.1:9',
  HTTPS_PROXY: 'http://127.0.0.1:9',
  NO_PROXY: 'localhost,127.0.0.1,::1',
  npm_config_offline: 'true',
};
for (const key of Object.keys(env))
  if (/API_KEY|ACCESS_TOKEN|^ELECTRON_RUN_AS_NODE$|^DSH_API_TOKEN$/i.test(key))
    delete env[key];
for (const dir of [
  home,
  shell,
  env.APPDATA,
  env.LOCALAPPDATA,
  env.DSH_DESKTOP_UPDATE_JOURNAL_DIR,
])
  await mkdir(dir, { recursive: true });
const run = promisify(execFile),
  cliPath = join(
    dirname(exe),
    'resources',
    'app.asar',
    'dsh',
    'node_modules',
    '@deepseek-ai',
    'dsh-desktop-host',
    'lib',
    'cli.js',
  );
const cli = (args) =>
  run(exe, ['--expose-internals', cliPath, ...args], {
    env: { ...env, ELECTRON_RUN_AS_NODE: '1' },
    cwd: qa,
    windowsHide: true,
    timeout: 90000,
  });
async function boot() {
  const app = await _electron.launch({
    executablePath: exe,
    args: [
      `--user-data-dir=${shell}`,
      '--disable-background-networking',
      '--proxy-server=http://127.0.0.1:9',
      '--proxy-bypass-list=localhost;127.0.0.1;[::1]',
      '--lang=zh-CN',
    ],
    cwd: qa,
    env,
    timeout: 40000,
  });
  try {
    assert.equal(
      await app.evaluate(({ app }) => app.getPath('userData')),
      shell,
    );
    assert.equal(await app.evaluate(() => process.env.DSH_HOME), home);
    return app;
  } catch (e) {
    await app.close();
    throw e;
  }
}
async function lookup(fn) {
  const until = Date.now() + 45000;
  while (Date.now() < until) {
    const value = await fn();
    if (value) return value;
    await wait(200);
  }
  throw Error('Timed out waiting for the DSH shell.');
}
async function panelPage(app, label = metadata.name) {
  const page = await lookup(async () => {
    for (const p of app.windows()) {
      if (p.isClosed()) continue;
      // Use the real welcome flow without entering any credential.
      let handled = false;
      for (const id of ['#skip-key', '#api-key']) {
        if (
          await p
            .locator(id)
            .isVisible()
            .catch(() => false)
        ) {
          await p.locator(id).click();
          handled = true;
          break;
        }
      }
      if (handled) continue;
      const accept = p
        .getByRole('button', { name: /^(继续|Continue)$/ })
        .first();
      if (await accept.isVisible().catch(() => false)) {
        await accept.click().catch(() => {});
        continue;
      }
      if (
        await p
          .getByRole('button', { name: label, exact: true })
          .first()
          .isVisible()
          .catch(() => false)
      )
        return p;
    }
  });
  const sidebar = page
    .getByRole('button', { name: label, exact: true })
    .first();
  assert.equal(
    await page.getByRole('button', { name: label, exact: true }).count(),
    1,
  );
  async function dismissWelcome() {
    for (let i = 0; i < 12; i++) {
      const cont = page
        .getByRole('button', { name: /^(继续|Continue)$/ })
        .first();
      if (await cont.isVisible()) await cont.click().catch(() => {});
      else if (
        await page
          .locator(
            '[role="dialog"]:visible,[role="presentation"]>[aria-hidden="true"][class*="mask"]:visible',
          )
          .count()
      )
        await page.keyboard.press('Escape');
      await wait(200);
    }
  }
  await dismissWelcome();
  await sidebar.click();
  const frame = page.frameLocator('[data-cw-panel]');
  await frame
    .locator('html[data-app-ready="true"] .app-shell')
    .waitFor({ timeout: 25000 });
  await dismissWelcome();
  const state = () =>
    page.evaluate(
      async () => await (await fetch('/convenient-window/control')).json(),
    );
  return { page, frame, sidebar, state };
}
const claim = async () => {
  const s = createServer();
  try {
    await new Promise((ok, no) =>
      s.once('error', no).listen(56873, '127.0.0.1', ok),
    );
  } catch {
    throw Error('The helper port is in use. No other process was stopped.');
  }
  return s;
};
const release = (s) => new Promise((ok) => s.close(ok));
let app;
try {
  await release(await claim());
  const version = (await cli(['--version'])).stdout.trim();
  assert(
    version.includes(metadata.engines.dsh),
    'Use the DSH version declared by this adapter.',
  );
  app = await boot();
  await lookup(() =>
    stat(join(profile, 'cordis.yml'))
      .then(() => true)
      .catch(() => false),
  );
  await app.close();
  app = null;
  let oldBytes;
  const savedFiles = new Map();
  const legacyPackage = 'dsh-plugin-convenient-window';
  const legacyV2 = `${legacyPackage}-0.2.0-beta.2.tgz`;
  if (oldPackagePath) {
    const old = resolve(oldPackagePath);
    const known = new Map([
      [
        `${legacyPackage}-0.1.0-beta.1.tgz`,
        '1aabb145f7072df6d223878e9390be0c4885ff2f2883b38a1c0a2b550c244272',
      ],
      [
        legacyV2,
        'c713a4815860e5f0c1c6d404a91d28ef017080eebd8175bbd49195be00bc1b25',
      ],
    ]);
    assert.equal(
      known.get(basename(old)),
      createHash('sha256')
        .update(await readFile(old))
        .digest('hex'),
    );
    await cli([
      'plugin',
      '--profile',
      'desktop',
      'add',
      `file:${old}`,
      '--offline',
      '--ignore-scripts',
    ]);
    const dir = join(profile, 'convenient-window');
    await mkdir(dir, { recursive: true });
    oldBytes = JSON.stringify({
      format: 1,
      rev: 6,
      prefs: {
        edgeHide: true,
        edges: ['left'],
        hoverDelayMs: 620,
        corners: {
          'top-left': 'show-desktop',
          'top-right': 'none',
          'bottom-left': 'none',
          'bottom-right': 'none',
        },
      },
    });
    await writeFile(join(dir, 'settings.json'), oldBytes);
    if (basename(old) === legacyV2) {
      // Exercise the previous package itself, including the old host's persistence rules.
      app = await boot();
      const oldPanel = await panelPage(app, /^(便捷窗口|Convenient Window)$/);
      await oldPanel.page.evaluate(async () => {
        const path = '/convenient-window/control';
        const snapshot = await (await fetch(path)).json();
        const post = async (command) => {
          const res = await fetch(path, {
            method: 'POST',
            headers: {
              'content-type': 'application/json',
              'x-convenient-window': 'panel',
            },
            body: JSON.stringify({ ...command, session: snapshot.session }),
          });
          if (!res.ok)
            throw Error(`Legacy settings save failed (${res.status})`);
        };
        snapshot.settings.edgeHide.stripSize = 19;
        await post({
          action: 'save',
          rev: snapshot.rev,
          settings: snapshot.settings,
        });
        const key = 'convenient-window-layout-exclusions';
        await post({
          action: 'view',
          key,
          previous: snapshot.view[key] ?? null,
          value: '["rename-check.exe"]',
        });
      });
      await app.close();
      app = null;
      for (const name of ['panel-settings.json', 'panel-view.json'])
        savedFiles.set(name, await readFile(join(dir, name)));
    }
    await cli(['plugin', '--profile', 'desktop', 'remove', legacyPackage]);
    await assert.rejects(stat(join(profile, 'node_modules', legacyPackage)), {
      code: 'ENOENT',
    });
  }
  const installed = await cli([
    'plugin',
    '--profile',
    'desktop',
    'add',
    `file:${tgz}`,
    '--offline',
    '--ignore-scripts',
  ]);
  assert(!/downloaded [1-9]\d*/i.test(installed.stdout));
  const moduleDir = join(profile, 'node_modules', metadata.name);
  const manifest = JSON.parse(
    await readFile(join(profile, 'package.json'), 'utf8'),
  );
  assert(!Object.hasOwn(manifest.dependencies ?? {}, legacyPackage));
  assert(Object.hasOwn(manifest.dependencies ?? {}, metadata.name));
  for (const [name, bytes] of savedFiles)
    assert.deepEqual(
      await readFile(join(profile, 'convenient-window', name)),
      bytes,
    );
  await verifyPayload(join(moduleDir, 'payload'));
  // Compare the archive to the installed bytes, not the working tree or the package manager's cache.
  const tar = join(process.env.SystemRoot, 'System32', 'tar.exe');
  const members = [
    'LICENSE',
    'README.md',
    'cordis.patch.yml',
    'host/assets.js',
    'host/helper.js',
    'host/index.js',
    'host/settings.js',
    'host/panel-controller.js',
    'host/native-service.js',
    'lib/cw-dsh-native.exe',
    'lib/native-identity.json',
    'lib/NATIVE-NOTICES.txt',
    'lib/model.mjs',
    'lib/panel.html',
    'lib/panel.css',
    'lib/panel.js',
    'lib/app-icon.png',
    'lib/client.js',
    'lib/defaults.json',
    'lib/THIRD-PARTY.txt',
    'package.json',
    'runtime-assets.json',
    ...assets.files.map((f) => 'payload/' + f.name),
  ]
    .map((n) => 'package/' + n)
    .sort();
  const listing = await run(tar, ['-tf', tgz], { windowsHide: true });
  assert.deepEqual(listing.stdout.trim().split(/\r?\n/).sort(), members);
  for (const name of members) {
    const { stdout } = await run(tar, ['-xOf', tgz, name], {
      encoding: 'buffer',
      maxBuffer: 12 * 1024 * 1024,
      windowsHide: true,
    });
    assert(
      stdout.equals(
        await readFile(join(moduleDir, name.slice('package/'.length))),
      ),
      `Installed bytes differ: ${name}`,
    );
  }
  console.log(
    `Verified ${members.length} installed archive members, including all pinned native files.`,
  );

  app = await boot();
  const { page, frame, sidebar, state } = await panelPage(app);
  const master = frame.locator('.master input'),
    connected = frame.locator('.connection.connected');
  assert.equal((await state()).running, false);
  if (oldBytes) {
    const v = await state();
    if (savedFiles.size) {
      assert.equal(v.settings.edgeHide.stripSize, 19);
      assert.equal(
        v.view['convenient-window-layout-exclusions'],
        '["rename-check.exe"]',
      );
    }
    assert(v.settings.edgeHide.enabled);
    assert.deepEqual(v.settings.edgeHide.edges, ['left']);
    assert.equal(v.settings.hoverDelayMs, 620);
    assert.equal(
      v.settings.hotzones
        .find((z) => z.id === 'top-left')
        .actions.find((a) => a.trigger === 'hover').action.kind,
      'show-desktop',
    );
  }
  const toggle = () => frame.locator('.master').click();
  const busy = await claim();
  try {
    const response = page.waitForResponse(
      (r) =>
        r.url().endsWith('/control') &&
        r.request().postDataJSON()?.action === 'start',
    );
    await toggle();
    assert.equal((await response).status(), 409);
    assert(busy.listening);
  } finally {
    await release(busy);
  }
  await lookup(
    async () => !(await master.isChecked()) && !(await master.isDisabled()),
  );
  await toggle();
  await connected.waitFor({ timeout: 35000 });
  const nativeState = () =>
    page.evaluate(async () => {
      const ctrl = '/convenient-window/control';
      const { session } = await (await fetch(ctrl)).json();
      return (
        await fetch(ctrl, {
          method: 'POST',
          headers: {
            'content-type': 'application/json',
            'x-convenient-window': 'panel',
          },
          body: JSON.stringify({ action: 'diagnostics', session }),
        })
      ).json();
    });
  assert.equal(
    (await nativeState()).loginTargetValid,
    true,
    'native login points at the installed DSH executable',
  );
  assert.equal(
    (await nativeState()).installedProfile,
    true,
    'login launcher resolves this installed profile',
  );
  const feature = (id) => frame.locator(`.mode-nav [data-feature-id="${id}"]`);
  for (const id of [
    'power',
    'hotzones',
    'edge-hide',
    'gestures',
    'beautification',
  ]) {
    await feature(id).click();
    await frame.locator('.drawer h2').first().waitFor();
    assert.equal(await feature(id).getAttribute('aria-current'), 'page');
  }
  await frame.locator('.theme-toggle.settings-toggle').click();
  await frame.locator('.language-setting select').selectOption('en-US');
  await frame.locator('html[lang="en-US"]').waitFor();
  await lookup(
    async () =>
      !/[\u3400-\u9fff]/.test(
        (await frame.locator('body').innerText()).replace(/简体中文|中文/g, ''),
      ),
  );
  for (const id of ['window-layouts', 'startup']) {
    await frame.locator(`.drawer [data-feature-id="${id}"]`).first().click();
    await frame
      .locator(id === 'startup' ? '.startup-filters' : '.drawer h2')
      .first()
      .waitFor();
    if (id === 'startup') await frame.locator('.startup-list').waitFor();
    await frame.locator('.theme-toggle.settings-toggle').click();
  }
  await lookup(
    async () => (await nativeState()).hudTitle === 'Volume / Brightness',
  );
  // Read the chosen test file through the browser's actual file chooser and verify the owned helper's disk snapshot.
  const change = (await state()).settings;
  change.edgeHide.stripSize = 23;
  const selected = page.waitForEvent('filechooser');
  await frame.locator('.config-actions button').nth(1).click();
  await (
    await selected
  ).setFiles({
    name: 'panel-import.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(change)),
  });
  await lookup(async () => {
    try {
      const c = JSON.parse(
        await readFile(
          join(profile, 'convenient-window', 'runtime', 'config.json'),
          'utf8',
        ),
      );
      return c.edgeHide.stripSize === 23;
    } catch {
      return false;
    }
  });
  await frame.locator('.language-setting select').selectOption('zh-CN');
  await frame.locator('html[lang="zh-CN"]').waitFor();
  await frame.locator('.theme-toggle:not(.settings-toggle)').click();
  await feature('edge-hide').click();
  await frame.locator('.drawer').waitFor();
  await wait(400);
  await page.screenshot({ path: join(qa, 'dsh-desktop-panel.png') });
  await lookup(async () => !!(await state()).view['convenient-window-theme']);
  await feature('power').click();
  await frame.locator('.diagnostics-heading button').click();
  const report = JSON.parse(
    await frame.locator('.diagnostics-panel textarea').inputValue(),
  );
  assert.equal(report.host, 'dsh');
  assert.equal(report.adapterVersion, metadata.version);
  assert.equal(report.runtime.helperRunning, true);
  assert.equal(report.runtime.installed, true);
  const ping = frame.locator('.connection-test');
  await ping.click();
  await frame.locator('.connection-test.success').waitFor({ timeout: 5000 });
  const other = page.getByRole('button', { name: /^(插件|Plugins)$/ }).first();
  await other.click();
  await lookup(
    async () => !(await page.locator('[data-cw-panel]').isVisible()),
  );
  assert.equal((await state()).running, true);
  await sidebar.click();
  await connected.waitFor();
  const win = await app.browserWindow(page);
  await win.evaluate((w) => w.minimize());
  await lookup(() => win.evaluate((w) => w.isMinimized()));
  assert.equal((await state()).running, true);
  await win.evaluate((w) => w.restore());
  await other.click();
  await lookup(
    async () => !(await page.locator('[data-cw-panel]').isVisible()),
  );
  // Exit with power ON, then check the engine BEFORE opening its tab again.
  await app.close();
  app = null;
  await release(await claim());
  app = await boot();
  await lookup(async () => {
    for (const shellPage of app.windows()) {
      if (shellPage.isClosed()) continue;
      const restoredInBackground = await shellPage
        .evaluate(async () => {
          try {
            if (document.querySelector('[data-cw-panel]')) return false;
            const v = await (await fetch('/convenient-window/control')).json();
            return v.running === true && v.settings?.edgeHide?.stripSize === 23;
          } catch {
            return false;
          }
        })
        .catch(() => false);
      if (restoredInBackground) return true;
    }
    return false;
  });
  const restored = await panelPage(app);
  await restored.frame.locator('.connection.connected').waitFor();
  await restored.frame.locator('.master').click();
  await lookup(async () => !(await restored.state()).running);
  await app.close();
  app = null;
  await release(await claim());
  app = await boot();
  const next = await panelPage(app);
  assert.equal((await next.state()).settings.enabled, false);
  if (oldBytes)
    assert.equal(
      await readFile(
        join(profile, 'convenient-window', 'settings.json'),
        'utf8',
      ),
      oldBytes,
    );
  assert.equal(
    (await next.state()).view['convenient-window-language'],
    'zh-CN',
  );
  await app.close();
  app = null;
  await cli(['plugin', '--profile', 'desktop', 'remove', metadata.name]);
  await assert.rejects(stat(moduleDir), { code: 'ENOENT' });
  await stat(join(profile, 'convenient-window', 'panel-settings.json'));
  console.log(
    `PASS: actual installed desktop panel, navigation, locale, minimized helper, restart/removal. Evidence: ${qa}; SHA-256 ${createHash(
      'sha256',
    )
      .update(await readFile(tgz))
      .digest('hex')}`,
  );
} finally {
  if (app) await app.close();
  console.log(`Isolated QA data (do not publish): ${qa}`);
}
