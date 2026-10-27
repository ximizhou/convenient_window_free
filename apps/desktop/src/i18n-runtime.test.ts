import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import nodePath from "node:path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { build } from "vite";
import { expect, it } from "vitest";

// 中英切换运行时回归：真实 App 在 happy-dom 里渲染，验证两件事——
//   1. 冷启动时保存的是英文，首帧中文必须在 initLanguage 后整体变成英文；
//   2. 运行中从“更多 → 界面语言”切到英文，底部页签、功能入口气泡等所有
//      由 helper 函数派生的文案也要跟着变。
// 背景：0.6.6 / 0.6.7 两次中英混排都是因为 Svelte 旧模式把模板里的函数调用整体
// untrack，只有直接调用 ui(...) 或响应式函数变量才会随语言重算。静态审计看不到
// 这种运行时行为，所以这里用断言把“英文界面不出现中文”固定成回归门。
it("keeps every rendered label switchable between Chinese and English", async () => {
  const directory = mkdtempSync(nodePath.join(import.meta.dirname, ".i18n-runtime-test-"));
  try {
    const entry = nodePath.join(directory, "entry.ts");
    writeFileSync(entry, `
      import assert from 'node:assert/strict';
      import { mount, unmount, flushSync } from 'svelte';
      import App from '../App.svelte';
      import { HelperClient } from '../helper-client';
      import { configureHostBridge } from '../host-bridge';
      import { defaultSettings, normalizeSettings } from '../settings-store';

      const settle = async () => { for (let i = 0; i < 8; i++) { await Promise.resolve(); flushSync(); } };
      // 语言下拉里的“中文”按惯例用母语展示，不算漏翻译；扫描其余正文。
      const cjk = () => {
        const clone = document.body.cloneNode(true);
        clone.querySelectorAll('select').forEach((node) => node.remove());
        return (clone.textContent ?? '').match(/[\\u4e00-\\u9fff]+/g) ?? [];
      };
      const nav = () => [...document.querySelectorAll('.mode-nav button b')].map((node) => node.textContent.trim());
      const navTabs = () => [...document.querySelectorAll('.mode-nav button')].map((node) => ({
        text: node.querySelector('b')?.textContent?.trim() ?? '',
        title: node.getAttribute('title') ?? ''
      }));
      const navButtons = () => [...document.querySelectorAll('.mode-nav button')];
      const composerLabels = () => [...document.querySelectorAll('.composer-bubble button')].map((node) => node.getAttribute('aria-label'));
      const openSettings = () => { document.querySelector('.settings-toggle').click(); flushSync(); };
      const chooseLanguage = async (value) => {
        const select = document.querySelector('.language-setting select');
        assert.ok(select, 'language selector must be available in the More panel');
        select.value = value;
        select.dispatchEvent(new Event('change', { bubbles: true }));
        flushSync(); await settle();
      };

      HelperClient.prototype.connect = function() {};
      HelperClient.prototype.disconnect = function() {};
      HelperClient.prototype.stop = function() {};
      HelperClient.prototype.sendConfig = function() { return true; };
      HelperClient.prototype.onMessage = function() { return () => {}; };
      HelperClient.prototype.onStatus = function() { return () => {}; };
      HelperClient.prototype.ping = function() {};

      let startFailure = null;
      const settings = normalizeSettings(defaultSettings);
      configureHostBridge({
        kind: 'desktop',
        getInitialSettings: () => structuredClone(settings),
        saveSettings: async () => {},
        importSettings: async () => null,
        exportSettings: async () => true,
        getHelperToken: () => 'test-token',
        getHelperState: () => ({ installed: true, development: false, version: '0.6.7', bytes: 0 }),
        getPrivilegeSupport: () => ({ supported: true }),
        getPrivilegeState: () => ({ supported: true, elevated: false }),
        getStartup: async () => ({ enabled: false }),
        getAdminStartup: async () => ({ enabled: false }),
        startHelper: async () => startFailure ? { ok: false, error: startFailure } : { ok: true },
        stopHelper: async () => ({ ok: true }),
        setHelperElevation: async () => ({ ok: true, elevated: false }),
        openExternal: () => ({ ok: true }),
        redirect: () => ({ ok: true }),
        diagnostics: async () => ({
          appDataDir: '', settingsPath: '', helperDataDir: '', helperPath: '',
          helperRunning: false, helperPayloadBytes: 0, lastExitCode: null, lastError: null, logTail: []
        })
      });

      // 1) 冷启动：保存的是英文，首帧中文必须在挂载后整体切换。
      localStorage.setItem('convenient-window-language', 'en-US');
      let component = mount(App, { target: document.body });
      flushSync(); await settle();
      assert.deepEqual(nav(), ['Center', 'Corners', 'Windows', 'Gestures', 'Style'], 'stored English must apply to the bottom tabs');
      // 页签用一词短标签，完整名称留在 title 里；长名称会挤爆抽屉打开后的窄场景。
      assert.ok(navTabs().every((tab) => !tab.text.includes(' ')), 'bottom tabs must use single-word labels: ' + JSON.stringify(navTabs()));
      assert.equal(navTabs().find((tab) => tab.text === 'Style')?.title, 'Appearance', 'the full feature name must stay in the tab title');
      assert.deepEqual(cjk(), [], 'English startup must not keep any Chinese text: ' + cjk().join(' / '));

      for (let index = 0; index < navButtons().length; index++) {
        navButtons()[index].click(); flushSync(); await settle();
        assert.deepEqual(cjk(), [], 'English drawer ' + index + ' must not contain Chinese: ' + cjk().join(' / '));
        navButtons()[index].click(); flushSync(); await settle();
      }
      openSettings(); await settle();
      assert.deepEqual(cjk(), [], 'English settings drawer must not contain Chinese: ' + cjk().join(' / '));
      assert.ok(composerLabels().includes('Corners'), 'feature entry labels must be English: ' + JSON.stringify(composerLabels()));
      openSettings(); await settle();

      // 2) 运行中切换：中文界面里打开设置并切到英文，页签和气泡必须立即跟随。
      await unmount(component); document.body.replaceChildren();
      localStorage.setItem('convenient-window-language', 'zh-CN');
      component = mount(App, { target: document.body });
      flushSync(); await settle();
      assert.deepEqual(nav(), ['运行中心', '触发角', '窗口增强', '鼠标手势', '窗口美化'], 'Chinese startup must render Chinese tabs');
      openSettings(); await settle();
      assert.ok(composerLabels().includes('触发角'), 'feature entry labels must start in Chinese: ' + JSON.stringify(composerLabels()));
      await chooseLanguage('en-US');
      assert.deepEqual(nav(), ['Center', 'Corners', 'Windows', 'Gestures', 'Style'], 'bottom tabs must follow a live language switch');
      assert.ok(composerLabels().includes('Corners'), 'feature entry labels must follow a live language switch: ' + JSON.stringify(composerLabels()));
      assert.deepEqual(cjk(), [], 'a live switch to English must not leave Chinese text: ' + cjk().join(' / '));
      await chooseLanguage('zh-CN');
      assert.deepEqual(nav(), ['运行中心', '触发角', '窗口增强', '鼠标手势', '窗口美化'], 'bottom tabs must switch back to Chinese');

      // 3) 英文错误路径：Rust/preload 的诊断仍是中文时，界面只能显示本地化通用提示。
      await unmount(component); document.body.replaceChildren();
      startFailure = '后台助手启动失败：模拟后端错误';
      localStorage.setItem('convenient-window-language', 'en-US');
      component = mount(App, { target: document.body });
      flushSync(); await settle();
      document.querySelector('[data-feature-id="power"]').click(); flushSync(); await settle();
      const startButton = document.querySelector('.power-actions .apply');
      assert.ok(startButton, 'the start action must be available');
      startButton.click(); await settle();
      assert.deepEqual(cjk(), [], 'English mode must localize a Chinese backend error: ' + cjk().join(' / '));
      assert.ok((document.body.textContent ?? '').includes('Operation failed'), 'backend errors must fall back to the localized technical error: ' + document.body.textContent);
      await unmount(component); document.body.replaceChildren();
      console.log('i18n runtime regression passed');
    `);
    await build({
      configFile: false, logLevel: "silent", plugins: [svelte()],
      build: {
        target: "esnext", minify: false, outDir: nodePath.join(directory, "bundle"),
        lib: { entry, formats: ["es"], fileName: () => "entry.mjs" },
        rollupOptions: { external: ["node:assert/strict"] }
      }
    });
    const runFile = nodePath.join(directory, "run.mjs");
    writeFileSync(runFile, `
      import { Window } from 'happy-dom';
      const window = new Window();
      Object.defineProperty(window.navigator, 'userAgent', { value: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)', configurable: true });
      class ResizeObserver {
        constructor(callback) { this.callback = callback; }
        observe(target) { this.callback([{ target, contentRect: target.getBoundingClientRect() }], this); }
        unobserve() {}
        disconnect() {}
      }
      window.ResizeObserver = ResizeObserver;
      for (const name of ['window', 'document', 'localStorage', 'Node', 'Text', 'Comment', 'Element', 'HTMLElement', 'HTMLInputElement', 'HTMLMediaElement', 'Event', 'PointerEvent', 'KeyboardEvent', 'MouseEvent', 'MutationObserver', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame']) {
        globalThis[name] = name === 'window' ? window : window[name];
      }
      globalThis.ResizeObserver = ResizeObserver;
      Object.defineProperty(globalThis, 'navigator', { value: window.navigator, configurable: true });
      const animate = window.Element.prototype.animate;
      window.Element.prototype.animate = function(...args) {
        const animation = animate.apply(this, args);
        animation.finished.catch(error => { if (error.name !== 'AbortError') throw error; });
        return animation;
      };
      try { await import('./bundle/entry.mjs'); }
      finally { await window.happyDOM.close(); }
    `);
    const output = execFileSync(process.execPath, [runFile], { encoding: "utf8", timeout: 30000, stdio: "pipe" });
    expect(output).toContain("i18n runtime regression passed");
  } finally {
    if (nodePath.dirname(nodePath.resolve(directory)) !== nodePath.resolve(import.meta.dirname)) throw new Error("Unexpected test directory");
    rmSync(directory, { recursive: true, force: true });
  }
}, 60000);
