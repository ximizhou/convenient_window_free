import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import nodePath from "node:path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { build } from "vite";
import { expect, it } from "vitest";

it("keeps startup mutations explicit, confirmed, localized and isolated from stale responses", async () => {
  const directory = mkdtempSync(nodePath.join(import.meta.dirname, ".startup-manager-test-"));
  try {
    writeFileSync(nodePath.join(directory, "Harness.svelte"), `<script>
      import StartupManager from '../StartupManager.svelte';
      export let request; export let pickExecutable;
      let english = true;
    </script>
    <button id="language" onclick={() => { english = !english; }}>Language</button>
    <StartupManager {english} {request} {pickExecutable} />`);
    const entry = nodePath.join(directory, "entry.ts");
    writeFileSync(entry, `
      import assert from 'node:assert/strict';
      import { mount, unmount, flushSync } from 'svelte';
      import Harness from './Harness.svelte';
      const settle = async () => { for (let i = 0; i < 8; i++) { await Promise.resolve(); flushSync(); } };
      const seed = { id: 'editor', name: 'Editor', command: 'editor.exe --tray', source: 'userRun', enabled: true, editable: true, revision: 'rev-1' };
      let items = [seed, { ...seed, id: 'notes', name: 'Notes', enabled: false, revision: 'notes-1' },
        { ...seed, id: 'system', name: 'System app', source: 'machineRun', editable: false, reason: 'systemItem' },
        { ...seed, id: 'unknown', name: 'Unknown app', enabled: null, editable: false, reason: 'unknownState' }];
      const calls = [];
      let finishMutation;
      let picked = null;
      let pickCalls = 0;
      const snapshot = () => ({ ok: true, version: 1, items: structuredClone(items), warnings: [] });
      const request = input => {
        calls.push(structuredClone(input));
        if (input.action === 'list') return Promise.resolve(snapshot());
        return new Promise(resolve => { finishMutation = resolve; });
      };
      const pickExecutable = async () => { pickCalls++; return picked; };
      const button = name => [...document.querySelectorAll('button')].find(node => node.textContent.trim() === name);
      const row = id => document.querySelector('[data-startup-id="' + id + '"]');
      const toggle = id => row(id).querySelector('[role="switch"]');
      const click = async node => { assert.ok(node, 'expected UI control'); node.click(); flushSync(); await settle(); };
      const search = async value => { const input = document.querySelector('input[type="search"]'); input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); flushSync(); await settle(); };
      let component = mount(Harness, { target: document.body, props: { request, pickExecutable } });
      flushSync(); await settle();
      assert.equal(document.querySelectorAll('.startup-item').length, 4);
      assert.equal(toggle('system').disabled, true, 'system entries are read only');
      assert.equal(toggle('unknown').disabled, true, 'unknown states are never guessed');
      await click(button('Disabled'));
      assert.equal(document.querySelectorAll('.startup-item').length, 1);
      assert.ok(row('notes'));
      await click(button('All')); await search('EDITOR');
      // Search includes executable commands, shared by these fixture entries.
      assert.equal(document.querySelectorAll('.startup-item').length, 4);
      await search('Notes'); assert.equal(document.querySelectorAll('.startup-item').length, 1);
      await search('');
      await click(toggle('editor'));
      assert.deepEqual(calls.at(-1), { action: 'setEnabled', id: 'editor', revision: 'rev-1', enabled: false });
      assert.equal(toggle('editor').getAttribute('aria-checked'), 'true', 'do not optimistically claim the app is disabled');
      const callCount = calls.length;
      toggle('editor').dispatchEvent(new MouseEvent('click', { bubbles: true })); await settle();
      window.dispatchEvent(new Event('focus')); await settle();
      assert.equal(calls.length, callCount, 'repeated clicks and focus cannot overlap a mutation');
      finishMutation({ ok: false, version: 1, items: [], error: { code: 'conflict', message: '注册表内容已改变' } }); await settle();
      assert.equal(toggle('editor').getAttribute('aria-checked'), 'true');
      assert.ok(document.querySelector('[role="alert"]').textContent.includes('Another app changed'));
      assert.equal(/[\\u4e00-\\u9fff]/.test(document.querySelector('.startup-manager').textContent), false, 'backend errors never leak untranslated text');
      await click(toggle('editor'));
      items = items.map(item => item.id === 'editor' ? { ...item, enabled: false, revision: 'rev-2' } : item);
      finishMutation(snapshot()); await settle();
      assert.equal(toggle('editor').getAttribute('aria-checked'), 'false');
      await click(toggle('editor'));
      assert.equal(calls.at(-1).revision, 'rev-2', 'the next mutation uses the confirmed revision');
      finishMutation({ ok: true, version: 1 }); await settle();
      assert.equal(toggle('editor').getAttribute('aria-checked'), 'false', 'an incomplete success response cannot change state');
      assert.ok(document.querySelector('[role="alert"]'));
      const beforePicker = calls.length;
      await click(button('＋Add app'));
      assert.equal(pickCalls, 1); assert.equal(calls.length, beforePicker, 'canceling the picker does not add anything');
      picked = 'C:/Apps/My Editor.exe';
      await click(button('＋Add app'));
      assert.ok(document.querySelector('.startup-draft'));
      assert.equal(calls.length, beforePicker, 'choosing a file still needs explicit confirmation');
      assert.equal(document.querySelector('.startup-draft input').value, 'My Editor');
      const name = document.querySelector('.startup-draft input');
      name.value = 'My app'; name.dispatchEvent(new Event('input', { bubbles: true })); flushSync();
      document.querySelector('.startup-draft').dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle();
      assert.deepEqual(calls.at(-1), { action: 'add', executablePath: 'C:/Apps/My Editor.exe', name: 'My app' });
      const pendingCalls = calls.length;
      document.querySelector('.startup-draft').dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })); await settle();
      assert.equal(calls.length, pendingCalls);
      items.push({ ...seed, id: 'added', name: 'My app', command: 'C:/Apps/My Editor.exe', revision: 'added-1' });
      finishMutation(snapshot()); await settle();
      assert.equal(document.querySelector('.startup-draft'), null);
      assert.ok(row('added'));
      await click(document.querySelector('#language'));
      assert.ok(document.querySelector('.startup-heading').textContent.includes('登录时启动'));
      await click(document.querySelector('#language'));
      assert.equal(/[\\u4e00-\\u9fff]/.test(document.querySelector('.startup-manager').textContent), false, 'labels and status translate on a live switch');
      items = items.filter(item => item.id !== 'added');
      window.dispatchEvent(new Event('focus')); await settle();
      assert.equal(row('added'), null, 'focus refreshes external changes');
      await unmount(component); document.body.replaceChildren();
      const callsAfterUnmount = calls.length; window.dispatchEvent(new Event('focus')); await settle();
      assert.equal(calls.length, callsAfterUnmount, 'unmounted panels do not keep monitoring');

      let finishOldList;
      component = mount(Harness, { target: document.body, props: { request: () => new Promise(resolve => { finishOldList = resolve; }), pickExecutable } });
      flushSync(); await settle(); await unmount(component); document.body.replaceChildren();
      component = mount(Harness, { target: document.body, props: { request: async () => ({ ok: true, version: 1, items: [{ ...seed, id: 'new', name: 'New snapshot' }] }), pickExecutable } });
      flushSync(); await settle(); finishOldList(snapshot()); await settle();
      assert.ok(row('new')); assert.equal(row('editor'), null, 'stale responses cannot replace a reopened panel');
      await unmount(component); document.body.replaceChildren();
      component = mount(Harness, { target: document.body, props: { request: async () => ({ ok: false, version: 1, error: { code: 'needsHelper', message: '后台助手未启动' } }), pickExecutable } });
      flushSync(); await settle();
      assert.ok(document.querySelector('[role="alert"]').textContent.includes('Install or update the helper'));
      assert.equal(button('＋Add app').disabled, true);
      await unmount(component);
      console.log('startup manager UI regression passed');
    `);
    await build({ root: nodePath.dirname(import.meta.dirname), configFile: false, logLevel: "silent", plugins: [svelte()], build: {
      target: "esnext", minify: false, outDir: nodePath.join(directory, "bundle"),
      lib: { entry, formats: ["es"], fileName: () => "entry.mjs" }, rollupOptions: { external: ["node:assert/strict"] }
    } });
    writeFileSync(nodePath.join(directory, "run.mjs"), `
      import { Window } from 'happy-dom';
      const window = new Window();
      for (const name of ['window', 'document', 'localStorage', 'Node', 'Text', 'Comment', 'Element', 'HTMLElement', 'HTMLInputElement', 'HTMLMediaElement', 'Event', 'PointerEvent', 'KeyboardEvent', 'MouseEvent', 'MutationObserver', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame']) globalThis[name] = name === 'window' ? window : window[name];
      Object.defineProperty(globalThis, 'navigator', { value: window.navigator, configurable: true });
      try { await import('./bundle/entry.mjs'); } finally { await window.happyDOM.close(); }
    `);
    expect(execFileSync(process.execPath, [nodePath.join(directory, "run.mjs")], { encoding: "utf8", timeout: 30000, stdio: "pipe" })).toContain("startup manager UI regression passed");
  } finally {
    if (nodePath.dirname(nodePath.resolve(directory)) !== nodePath.resolve(import.meta.dirname)) throw new Error("Unexpected test directory");
    rmSync(directory, { recursive: true, force: true });
  }
}, 60000);
