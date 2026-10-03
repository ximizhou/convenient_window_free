import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import nodePath from "node:path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { build } from "vite";
import { expect, it } from "vitest";

it("keeps monitor-specific timings through detection, profile migration, zone switches, and remount", async () => {
  const directory = mkdtempSync(nodePath.join(import.meta.dirname, ".hotzone-timing-test-"));
  try {
    const entry = nodePath.join(directory, "entry.ts");
    writeFileSync(entry, `
      import assert from 'node:assert/strict';
      import { mount, unmount, flushSync } from 'svelte';
      import App from '../App.svelte';
      import { configureHostBridge } from '../host-bridge';
      import { HelperClient } from '../helper-client';
      import { defaultSettings, normalizeSettings } from '../settings-store';
      let receive;
      let receiveStatus;
      HelperClient.prototype.onMessage = function(handler) { receive = handler; return () => {}; };
      HelperClient.prototype.onStatus = function(handler) { receiveStatus = handler; return () => {}; };
      let stored = normalizeSettings(defaultSettings);
      stored.hotzonesEnabled = true;
      let saves = 0;
      configureHostBridge({
        kind: 'desktop', getInitialSettings: () => structuredClone(stored),
        getHelperToken: () => null,
        getHelperState: () => ({ installed: false, development: true }),
        saveSettings: async value => { stored = JSON.parse(JSON.stringify(value)); saves++; }
      });
      const display = { id: 'monitor:actual', legacyId: 'display:0:0:2560:1440', primary: true,
        bounds: { left: 0, top: 0, right: 2560, bottom: 1440 }, workArea: { left: 0, top: 0, right: 2560, bottom: 1400 } };
      const open = () => { document.querySelectorAll('.mode-nav button')[1].click(); flushSync(); };
      const detect = () => { receive({ type: 'runtime.status', data: { displays: [display] } }); flushSync(); };
      const timing = () => [...document.querySelectorAll('.timing input')];
      const type = (node, value) => { node.value = value; node.dispatchEvent(new Event('input')); flushSync(); };
      const slot = (profile, zone = 'right', trigger = 'hover') => profile.hotzones.find(item => item.id === zone).actions.find(item => item.trigger === trigger);
      let component = mount(App, { target: document.body });
      flushSync(); open();
      assert.ok(timing()[0].closest('[inert]'), 'timing must wait for actual monitor identity');
      assert.equal(document.querySelector('.form-grid input').closest('[inert]'), null, 'global edge size remains editable');
      assert.equal(saves, 0);
      detect();
      assert.equal(timing()[0].closest('[inert]'), null);
      type(timing()[0], '500');
      type(timing()[1], '900');
      await Promise.resolve();
      assert.equal(stored.monitorProfiles.length, 1);
      assert.equal(stored.monitorProfiles[0].monitorId, display.id);
      assert.equal(slot(stored.monitorProfiles[0]).hoverDelayMs, 500);
      assert.equal(slot(stored.monitorProfiles[0]).cooldownMs, 900);
      assert.equal(slot({ hotzones: stored.hotzones }).hoverDelayMs, 350);
      document.querySelector('.zone-left').click(); flushSync();
      assert.deepEqual(timing().map(node => node.value), ['350', '700']);
      document.querySelector('.zone-right').click(); flushSync();
      assert.deepEqual(timing().map(node => node.value), ['500', '900']);
      document.querySelectorAll('.trigger-tabs button')[1].click(); flushSync();
      assert.deepEqual(timing().map(node => node.value), ['700']);
      type(timing()[0], '600');
      document.querySelectorAll('.trigger-tabs button')[0].click(); flushSync();
      assert.deepEqual(timing().map(node => node.value), ['500', '900']);
      assert.equal(slot(stored.monitorProfiles[0], 'right', 'left-click').cooldownMs, 600);
      timing()[1].focus();
      type(timing()[1], '3');
      receiveStatus('disconnected'); flushSync();
      const savesBeforeBlur = saves;
      timing()[1].dispatchEvent(new Event('blur')); flushSync();
      await Promise.resolve();
      assert.equal(saves, savesBeforeBlur, 'disconnect must not commit a focused timing draft');
      assert.equal(slot(stored.monitorProfiles[0]).cooldownMs, 900);
      await unmount(component);
      document.body.replaceChildren();
      // Exercise the legacy-ID migration on the next startup as well.
      stored.monitorProfiles[0].monitorId = display.legacyId;
      component = mount(App, { target: document.body });
      flushSync(); open(); detect();
      await Promise.resolve();
      assert.deepEqual(timing().map(node => node.value), ['500', '900']);
      assert.equal(stored.monitorProfiles[0].monitorId, display.id);
      document.querySelectorAll('.trigger-tabs button')[1].click(); flushSync();
      assert.deepEqual(timing().map(node => node.value), ['600']);
      await unmount(component);
      console.log('App monitor timing round trip passed');
    `);
    await build({
      configFile: false,
      logLevel: "silent",
      plugins: [svelte()],
      build: {
        target: "esnext", minify: false, outDir: nodePath.join(directory, "bundle"),
        lib: { entry, formats: ["es"], fileName: () => "entry.mjs" },
        rollupOptions: { external: ["node:assert/strict"] }
      }
    });
    writeFileSync(nodePath.join(directory, "run.mjs"), `
      import { Window } from 'happy-dom';
      const window = new Window();
      for (const name of ['window', 'document', 'localStorage', 'Node', 'Text', 'Comment', 'Element', 'HTMLElement', 'HTMLInputElement', 'HTMLMediaElement', 'Event', 'MutationObserver', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame']) {
        globalThis[name] = name === 'window' ? window : window[name];
      }
      Object.defineProperty(globalThis, 'navigator', { value: window.navigator, configurable: true });
      // Happy DOM rejects canceled animation promises during Svelte teardown.
      const animate = window.Element.prototype.animate;
      window.Element.prototype.animate = function(...args) {
        const animation = animate.apply(this, args);
        animation.finished.catch(error => { if (error.name !== 'AbortError') throw error; });
        return animation;
      };
      localStorage.setItem('convenient-window-language', 'en-US');
      await import('./bundle/entry.mjs');
      await window.happyDOM.close();
    `);
    const output = execFileSync(process.execPath, [nodePath.join(directory, "run.mjs")], { encoding: "utf8" });
    expect(output).toContain("App monitor timing round trip passed");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}, 20000);
