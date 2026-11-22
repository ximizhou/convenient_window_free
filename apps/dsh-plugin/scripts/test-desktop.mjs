// Opt-in Windows acceptance test. It operates only its own DSH profile, shell and helper.
// Real drag-to-edge and show-desktop observations are deliberately left to a human.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtemp, mkdir, readFile, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { promisify } from 'node:util';
import { setTimeout as wait } from 'node:timers/promises';
import { _electron } from 'playwright-core';
import { assets, metadata, verifyPayload } from '../host/assets.js';

const [appPath, packagePath] = process.argv.slice(2);
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
async function panelPage(app) {
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
          .getByRole('button', { name: /^(便捷窗口|Convenient Window)$/ })
          .first()
          .isVisible()
          .catch(() => false)
      )
        return p;
    }
  });
  const sidebar = page
    .getByRole('button', { name: /^(便捷窗口|Convenient Window)$/ })
    .first();
  const panel = page.locator('[data-cw-panel]');
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
  await panel.locator('[data-cw-power]').waitFor();
  await dismissWelcome();
  if ((await panel.locator('button.lang').innerText()) !== 'English')
    await panel.locator('button.lang').click();
  const button = (name) => panel.getByRole('button', { name, exact: true });
  const tab = (name) =>
    panel.locator('nav').getByRole('button', { name, exact: true });
  const post = async (el) => {
    const [r] = await Promise.all([
      page.waitForResponse(
        (r) =>
          r.url().endsWith('/convenient-window/control') &&
          r.request().method() === 'POST',
        { timeout: 40000 },
      ),
      el.click(),
    ]);
    return { code: r.status(), data: await r.json() };
  };
  const state = () =>
    page.evaluate(
      async () => await (await fetch('/convenient-window/control')).json(),
    );
  return { page, panel, sidebar, tab, button, post, state };
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
  const { page, panel, sidebar, tab, button, post, state } =
    await panelPage(app);
  assert.equal((await state()).running, false);
  const busy = await claim();
  try {
    const conflict = await post(panel.locator('[data-cw-power]'));
    assert.equal(conflict.code, 409);
    assert.equal(conflict.data.error, 'helper-in-use');
    assert(busy.listening);
  } finally {
    await release(busy);
  }
  await tab('窗口增强').click();
  await panel.getByRole('checkbox', { name: '贴边隐藏', exact: true }).check();
  for (const name of ['右边', '上边', '下边']) await button(name).click();
  assert.equal((await post(button('保存设置'))).code, 200);
  await tab('触发角').click();
  await panel.getByRole('combobox').selectOption('show-desktop');
  assert.equal((await post(button('保存设置'))).code, 200);
  await tab('运行中心').click();
  assert.equal(
    (await post(panel.locator('[data-cw-power]'))).data.running,
    true,
  );
  const live = JSON.parse(
    await readFile(
      join(profile, 'convenient-window', 'runtime', 'config.json'),
      'utf8',
    ),
  );
  assert.deepEqual(live.edgeHide.edges, ['left']);
  assert.equal(live.edgeHide.enabled, true);
  assert.equal(
    live.hotzones
      .find((z) => z.id === 'top-left')
      .actions.find((a) => a.trigger === 'hover').action.kind,
    'show-desktop',
  );
  await button('English').click();
  for (const label of ['Control center', 'Hot corners', 'Window controls']) {
    await tab(label).click();
    assert(
      !/[\u3400-\u9fff]/.test((await panel.innerText()).replace('中文', '')),
    );
  }
  await button('中文').click();
  await tab('触发角').click();
  await panel.getByRole('slider').focus();
  await page.keyboard.press('ArrowRight');
  assert.equal((await post(button('保存设置'))).data.running, true);
  const alt = page
    .getByRole('button', {
      name: /^(聊天|对话|会话|新会话|Chat|插件|Plugins)$/,
    })
    .first();
  await alt.click();
  await panel.waitFor({ state: 'detached' });
  assert.equal((await state()).running, true);
  const win = await app.browserWindow(page);
  await win.evaluate((w) => w.minimize());
  await lookup(() => win.evaluate((w) => w.isMinimized()));
  assert.equal((await state()).running, true);
  await win.evaluate((w) => w.restore());
  await sidebar.click();
  const power = panel.locator('[data-cw-power]');
  assert.equal((await post(power)).data.running, false);
  assert.equal((await post(power)).data.running, true);
  await app.close();
  app = null;
  await release(await claim());
  app = await boot();
  const restored = await (await panelPage(app)).state();
  assert.equal(restored.running, false);
  assert.deepEqual(restored.prefs.edges, ['left']);
  assert.equal(restored.prefs.corners['top-left'], 'show-desktop');
  await app.close();
  app = null;
  await cli(['plugin', '--profile', 'desktop', 'remove', metadata.name]);
  await assert.rejects(stat(moduleDir), { code: 'ENOENT' });
  await stat(join(profile, 'convenient-window', 'settings.json'));
  console.log(
    `PASS: clean offline desktop install, sidebar, native apply, both languages, conflict, minimize, stop/restart, quit, persistence and removal.\nSHA-256: ${createHash(
      'sha256',
    )
      .update(await readFile(tgz))
      .digest('hex')}`,
  );
} finally {
  if (app) await app.close();
  console.log(`Isolated QA data (do not publish): ${qa}`);
}
