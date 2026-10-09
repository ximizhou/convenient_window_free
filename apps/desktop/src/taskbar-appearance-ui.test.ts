import { describe, expect, it } from "vitest";
import { readFileSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import nodePath from "node:path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { build } from "vite";
import { zh, en } from "./i18n";
const source = readFileSync(new URL("./TaskbarAppearance.svelte", import.meta.url), "utf8");
describe("taskbar recovery controls", () => {
  it("allows re-enable while a previous restore is pending", () => {
    expect(source).toContain("disabled={!canApply || retryPending || (busy && appearance.enabled) || applied}");
    expect(source).not.toContain("disabled={!canApply || busy || applied}");
  });
  it("keeps cancellation available while connecting or recovering", () => {
    expect(source).toContain('status?.state === "recovering"');
    expect(source).toContain('!retryPending && !status?.terminal && status?.state !== "error"');
    expect(source).toContain("onChange({ enabled: false })");
    expect(source).toContain("applyAppearance");
    expect(source).toContain("restoreAppearance");
  });
  it("reports recovery honestly and puts raw error codes in optional details", () => {
    expect(zh.beautyRecovering).toContain("自动恢复");
    expect(en.beautyRecovering).toContain("automatically");
    expect(source).toContain('current.state === "recovering"');
    expect(source).toContain('<details class="beauty-diagnostics">');
    expect(source).toContain("t.beautyDiagnostics");
    expect(source).toContain('current.errorCode === "0x80070666"');
  });
  it("does not present terminal failures as automatic recovery and keeps retry available", () => {
    expect(source).toContain('status?.retryable === true');
    expect(source).toContain('current.terminal');
    expect(source).toContain('return "beautyRetryExhausted"');
    expect(source).toContain('|| canRetry');
    expect(zh.beautyRetryExhausted).toContain("手动重试");
    expect(en.beautyRetryExhausted).toContain("Retry manually");
    expect(zh.beautyConflict).not.toContain("自动继续");
    expect(en.beautyConflict).not.toContain("automatically");
  });
});


it("renders terminal failures and retries only after restore acknowledgement", async () => {
  const directory = mkdtempSync(nodePath.join(import.meta.dirname, ".taskbar-ui-"));
  try {
    const entry = nodePath.join(directory, "entry.ts");
    writeFileSync(nodePath.join(directory, "Harness.svelte"), `
      <script lang="ts">
        import TaskbarAppearance from '../TaskbarAppearance.svelte';
        export let onChange;
        let props = { english: true, connected: true, masterEnabled: true,
          appearance: { enabled: true, mode: 'transparent', opacity: 58, tint: '#233A63', showBorder: false },
          status: { state: 'error', available: false, terminal: true, retryable: true,
            errorCode: '0x80070102', materials: ['transparent', 'acrylic', 'solid'] } };
        export function update(patch) { props = { ...props, ...patch }; }
      </script>
      <TaskbarAppearance {...props} {onChange} />
    `);
    writeFileSync(entry, `
      import assert from 'node:assert/strict';
      import { mount, unmount, flushSync } from 'svelte';
      import Harness from './Harness.svelte';
      import { en, zh } from '../i18n';
      const changes = [];
      const component = mount(Harness, { target: document.body, props: { onChange: value => changes.push(value) } });
      const update = patch => { component.update(patch); flushSync(); };
      const appearance = enabled => ({ enabled, mode: 'transparent', opacity: 58, tint: '#233A63', showBorder: false });
      const status = (state, extra = {}) => ({ state, available: false, terminal: true, retryable: true, materials: ['transparent','acrylic','solid'], ...extra });
      const apply = () => document.querySelector('.beauty-actions .apply');
      const restore = () => document.querySelector('.beauty-actions .quiet');
      const text = () => document.querySelector('.beauty-status').textContent;
      flushSync();
      assert.equal(apply().disabled, false, 'retryable error must not be blocked by available=false');
      assert.ok(text().includes(en.beautyRetryExhausted));
      assert.ok(!text().includes(en.beautyRecovering));
      assert.ok(document.querySelector('.beauty-diagnostics').textContent.includes('0x80070102'));
      apply().click(); flushSync();
      assert.deepEqual(changes, [{ enabled: false }], 'retry first requests disable/restoration');
      assert.equal(apply().disabled, true, 'block duplicate retries while awaiting restoration');
      assert.equal(restore().disabled, false, 'cancel remains available during retry');
      update({ appearance: appearance(false), status: status('restoring', { terminal: false }) });
      assert.equal(changes.length, 1, 'restoring is not an acknowledgement');
      update({ status: status('inactive', { terminal: false, available: true, retryable: false }) });
      assert.deepEqual(changes, [{ enabled: false }, { enabled: true }], 'enable only after native reports inactive');
      update({ appearance: appearance(true), status: status('unsupported') });
      assert.equal(apply().disabled, false, 'unsupported retains explicit retry');
      assert.ok(text().includes(en.beautyUnsupported));
      apply().click(); flushSync();
      update({ appearance: appearance(false) });
      restore().click(); flushSync();
      const afterCancel = changes.length;
      update({ status: status('inactive', { terminal: false, available: true }) });
      assert.equal(changes.length, afterCancel, 'restore cancels automatic re-enable');
      update({ appearance: appearance(true), status: status('conflict') });
      assert.ok(text().includes(en.beautyConflict));
      assert.ok(!text().includes('automatically'));
      assert.equal(apply().disabled, false);
      update({ status: status('error', { errorCode: '0x80070666' }) });
      assert.ok(text().includes(en.beautyVersionMismatch), 'resident version guidance must be retained');
      update({ english: false, status: status('error') });
      assert.ok(text().includes(zh.beautyRetryExhausted));
      // Legacy helpers omit terminal/retryable: errors still allow explicit retry.
      update({ english: true, status: { state: 'error', available: false, materials: ['transparent','acrylic','solid'] } });
      assert.equal(apply().disabled, false);
      assert.ok(text().includes(en.beautyError));
      update({ status: status('recovering', { terminal: false }) });
      assert.equal(apply().disabled, true, 'active automatic recovery is busy');
      assert.equal(restore().disabled, false);
      update({ status: status('error'), connected: false });
      assert.equal(apply().disabled, true);
      update({ connected: true, masterEnabled: false });
      assert.equal(apply().disabled, true);
      await unmount(component);
      console.log('Taskbar terminal retry DOM passed');
    `);
    await build({
      configFile: false, logLevel: "silent", plugins: [svelte()],
      build: { target: "esnext", minify: false, outDir: nodePath.join(directory, "bundle"),
        lib: { entry, formats: ["es"], fileName: () => "entry.mjs" },
        rollupOptions: { external: ["node:assert/strict"] } }
    });
    writeFileSync(nodePath.join(directory, "run.mjs"), `
      import { Window } from 'happy-dom';
      const window = new Window({ width: 640, height: 600 });
      for (const name of ['window','document','Node','Text','Comment','Element','HTMLElement','HTMLInputElement','HTMLMediaElement','Event','MutationObserver','getComputedStyle','requestAnimationFrame','cancelAnimationFrame']) {
        globalThis[name] = name === 'window' ? window : window[name];
      }
      Object.defineProperty(globalThis, 'navigator', { value: window.navigator, configurable: true });
      try { await import('./bundle/entry.mjs'); }
      finally { await window.happyDOM.close(); }
    `);
    const output = execFileSync(process.execPath, [nodePath.join(directory, "run.mjs")], { encoding: "utf8", timeout: 30000, stdio: "pipe" });
    expect(output).toContain("Taskbar terminal retry DOM passed");
  } finally {
    if (nodePath.dirname(nodePath.resolve(directory)) !== nodePath.resolve(import.meta.dirname)) throw new Error("Unexpected test directory");
    rmSync(directory, { recursive: true, force: true });
  }
}, 60000);
