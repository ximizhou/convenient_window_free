import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import nodePath from "node:path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { build } from "vite";
import { expect, it } from "vitest";

it("edits geometry through the real App, isolates areas and displays, and round-trips without changing actions", async () => {
  const directory = mkdtempSync(nodePath.join(import.meta.dirname, ".hotzone-geometry-ui-test-"));
  try {
    const entry = nodePath.join(directory, "entry.ts");
    writeFileSync(entry, `
      import assert from 'node:assert/strict';
      import { mount, unmount, flushSync } from 'svelte';
      import App from '../App.svelte';
      import { HelperClient } from '../helper-client';
      import { defaultSettings, normalizeSettings } from '../settings-store';
      import { configureHostBridge } from '../host-bridge';
      let receive, receiveStatus, saves = 0, stopCalls = 0, exportedJson = '';
      const sent = [];
      HelperClient.prototype.onMessage = function(handler) { receive = handler; return () => {}; };
      HelperClient.prototype.onStatus = function(handler) { receiveStatus = handler; return () => {}; };
      HelperClient.prototype.connect = function() { receiveStatus('connecting'); };
      HelperClient.prototype.disconnect = function() { receiveStatus('disconnected'); };
      HelperClient.prototype.stop = function() { stopCalls++; receiveStatus('disconnected'); return true; };
      HelperClient.prototype.sendConfig = function(value) { sent.push(structuredClone(value)); return true; };
      let stored = normalizeSettings(defaultSettings);
      stored.enabled = true;
      stored.hotzonesEnabled = true;
      stored.showHotzoneHint = false;
      for (const zone of stored.hotzones) for (const slot of zone.actions) { slot.action = { kind: 'none' }; slot.modifierActions = []; }
      stored.hotzones.find(zone => zone.id === 'top-left').geometry = { kind: 'corner', width: 24, height: 12, linked: true };
      const initialActions = normalizeSettings(stored).hotzones.map(zone => structuredClone(zone.actions));
      configureHostBridge({ kind: 'desktop', getInitialSettings: () => structuredClone(stored), getHelperToken: () => null,
        getHelperState: () => ({ installed: true, development: true, version: '0.6.4' }), startHelper: async () => ({ ok: true }),
        saveSettings: async value => { stored = structuredClone(value); saves++; },
        exportSettings: async value => { exportedJson = JSON.stringify(value); return true; }, importSettings: async () => JSON.parse(exportedJson) });
      const a = { id: 'monitor:real-a', primary: true, bounds: { left: -2560, top: -200, right: 0, bottom: 1240 }, workArea: { left: -2560, top: -200, right: 0, bottom: 1200 } };
      const b = { id: 'monitor:real-b', primary: false, bounds: { left: 0, top: 0, right: 1920, bottom: 1080 }, workArea: { left: 0, top: 0, right: 1920, bottom: 1040 } };
      const settle = async () => { for (let i = 0; i < 10; i++) { await Promise.resolve(); flushSync(); } };
      const open = () => { if (!document.querySelector('.hotzone-geometry')) document.querySelectorAll('.mode-nav button')[1].click(); flushSync(); };
      const detect = () => { receive({ type: 'runtime.status', data: { displays: [a, b] } }); flushSync(); };
      const ready = () => { receiveStatus('connected'); receive({ type: 'helper.ready', data: { version: '0.6.4', protocolVersion: 7, schemaVersion: 9 } }); detect(); };
      const number = label => document.querySelector('.hotzone-geometry input[aria-label="' + label + '"]');
      const type = async (node, value) => { node.value = String(value); node.dispatchEvent(new Event('input', { bubbles: true })); await settle(); };
      const pick = zone => {
        const selector = document.querySelector('.geometry-zone-select');
        assert.equal(selector.options.length, 8);
        assert.equal(selector.getAttribute('aria-label'), 'Select current area');
        selector.dispatchEvent(new Event('pointerdown', { bubbles: true }));
        if (document.activeElement instanceof HTMLInputElement) document.activeElement.blur();
        selector.value = zone; selector.dispatchEvent(new Event('change', { bubbles: true })); flushSync();
        assert.equal(selector.value, zone);
      };
      const screen = index => {
        const button = document.querySelectorAll('.screen')[index];
        button.dispatchEvent(new Event('pointerdown', { bubbles: true }));
        if (document.activeElement instanceof HTMLInputElement) document.activeElement.blur();
        button.click(); flushSync();
      };
      const geometry = (monitor, zone) => stored.monitorProfiles.find(profile => profile.monitorId === monitor)?.hotzones.find(item => item.id === zone)?.geometry;
      const footprint = () => document.querySelector('.hotzone-footprint');
      const preset = value => document.querySelectorAll('.geometry-presets button')[[20, 40, 100].indexOf(value)];
      let component = mount(App, { target: document.body }); flushSync(); await settle(); open();
      assert.ok(number('Thickness').closest('[inert]'), 'no geometry profile before real display detection');
      const savesBefore = saves;
      await type(number('Thickness'), 20);
      assert.equal(saves, savesBefore);
      assert.equal(stored.monitorProfiles.length, 0);
      assert.equal(footprint(), null);
      ready(); await settle();
      assert.equal(number('Thickness').value, '8');
      assert.equal(number('Length').value, '40');
      assert.equal(footprint().dataset.rect, '-8,232,0,808');
      await type(number('Thickness'), 24);
      assert.deepEqual(geometry(a.id, 'right'), { kind: 'edge', thickness: 24, lengthPercent: 40 });
      await type(number('Length'), 70);
      assert.equal(footprint().dataset.rect, '-24,16,0,1024');
      await type(document.querySelector('.geometry-slider'), 60);
      assert.equal(number('Length').value, '60');
      assert.equal(footprint().dataset.rect, '-24,88,0,952');
      for (const value of [20, 40, 100]) {
        preset(value).click(); await settle();
        assert.equal(geometry(a.id, 'right').lengthPercent, value);
        assert.equal(number('Length').value, String(value));
      }
      assert.equal(footprint().style.height, '100%');
      assert.ok(document.querySelector('.geometry-priority').textContent.includes('Corner areas take priority'));
      assert.equal(document.querySelectorAll('.zone').length, 8, 'full edge must leave all zone selectors available');
      pick('left');
      assert.equal(number('Length').value, '40');
      assert.equal(geometry(a.id, 'left'), undefined);
      await type(number('Thickness'), 16);
      pick('right');
      assert.equal(number('Thickness').value, '24');
      const beforeZoneDraft = saves;
      number('Thickness').focus(); await type(number('Thickness'), '1');
      pick('left');
      assert.equal(number('Thickness').value, '16', 'narrow area selector cancels the old draft');
      assert.equal(saves, beforeZoneDraft);
      pick('right');
      assert.equal(number('Thickness').value, '24');
      const beforeDraft = saves;
      number('Thickness').focus(); await type(number('Thickness'), '1');
      assert.equal(saves, beforeDraft, 'unfinished numbers are not persisted');
      screen(1);
      assert.equal(number('Thickness').value, '8', 'switching monitor cancels the draft');
      assert.equal(number('Length').value, '40');
      preset(20).click(); await settle();
      assert.deepEqual(geometry(b.id, 'right'), { kind: 'edge', thickness: 8, lengthPercent: 20 });
      assert.equal(geometry(a.id, 'right').lengthPercent, 100);
      assert.equal(footprint().dataset.rect, '1912,432,1920,648');
      screen(0);
      assert.equal(number('Length').value, '100');
      pick('top-left');
      assert.equal(number('Width').value, '24'); assert.equal(number('Height').value, '12');
      assert.equal(document.querySelector('.geometry-link').getAttribute('aria-pressed'), 'true');
      await type(number('Width'), 48);
      assert.deepEqual(geometry(a.id, 'top-left'), { kind: 'corner', width: 48, height: 24, linked: true });
      assert.equal(number('Height').value, '24', 'linked mode keeps the current rectangle ratio, not a square');
      document.querySelector('.geometry-link').click(); await settle();
      await type(number('Height'), 64);
      assert.deepEqual(geometry(a.id, 'top-left'), { kind: 'corner', width: 48, height: 64, linked: false });
      assert.equal(footprint().dataset.rect, '-2560,-200,-2512,-136');
      document.querySelector('.geometry-link').click(); await settle();
      await type(number('Width'), 96);
      assert.equal(number('Height').value, '128');
      assert.deepEqual(geometry(a.id, 'top-left'), { kind: 'corner', width: 96, height: 128, linked: true });
      assert.deepEqual(stored.hotzones.find(zone => zone.id === 'top-left').geometry, { kind: 'corner', width: 24, height: 12, linked: true });
      screen(1); pick('top-left');
      assert.equal(number('Width').value, '24'); assert.equal(number('Height').value, '12');
      await type(number('Width'), 36);
      assert.equal(number('Height').value, '18');
      assert.equal(geometry(a.id, 'top-left').width, 96);
      screen(0);
      document.querySelector('.geometry-reset').click(); await settle();
      assert.equal(geometry(a.id, 'top-left'), undefined, 'reset deletes only the selected override');
      assert.equal(number('Width').value, '8'); assert.equal(number('Height').value, '8', 'reset uses the legacy fallback, not the inherited override');
      assert.equal(geometry(b.id, 'top-left').width, 36);
      pick('right'); document.querySelector('.geometry-reset').click(); await settle();
      assert.equal(geometry(a.id, 'right'), undefined);
      assert.equal(number('Length').value, '40');
      assert.equal(document.querySelector('.geometry-priority'), null);
      assert.equal(footprint().dataset.rect, '-8,232,0,808');
      assert.equal(geometry(a.id, 'left').thickness, 16);
      assert.equal(geometry(b.id, 'right').lengthPercent, 20);
      assert.equal(stored.showHotzoneHint, false);
      for (const profile of stored.monitorProfiles) assert.deepEqual(profile.hotzones.map(zone => zone.actions), initialActions, 'saving geometry cannot invent a configured action');
      assert.equal(document.querySelectorAll('.zone em').length, 0, 'a no-action zone must not gain a configured marker');
      document.querySelector('.settings-toggle').click(); flushSync();
      document.querySelectorAll('.config-actions .quiet')[0].click(); await settle();
      const exported = JSON.parse(exportedJson);
      assert.equal(exported.schemaVersion, 9);
      assert.equal(exported.showHotzoneHint, false);
      open(); screen(1); pick('right'); preset(100).click(); await settle();
      assert.equal(geometry(b.id, 'right').lengthPercent, 100);
      document.querySelector('.settings-toggle').click(); flushSync();
      document.querySelectorAll('.config-actions .quiet')[1].click(); await settle();
      assert.equal(geometry(b.id, 'right').lengthPercent, 20, 'the real App import restores per-monitor geometry');
      assert.equal(stored.showHotzoneHint, false);
      await unmount(component); document.body.replaceChildren();
      component = mount(App, { target: document.body }); flushSync(); await settle(); open(); ready(); screen(1); pick('right');
      assert.equal(number('Length').value, '20');
      assert.equal(footprint().dataset.rect, '1912,432,1920,648', 'the actual MonitorStage synchronizes after remount');
      pick('top-left');
      assert.equal(number('Width').value, '36'); assert.equal(number('Height').value, '18');
      number('Width').focus(); await type(number('Width'), '1');
      const priorDisconnect = saves;
      receiveStatus('disconnected'); flushSync();
      number('Width').dispatchEvent(new Event('blur')); await settle();
      assert.equal(saves, priorDisconnect, 'disconnect cancels invalid numeric drafts');
      ready(); screen(1); pick('top-left');
      assert.equal(number('Width').value, '36');
      await unmount(component); document.body.replaceChildren();
      component = mount(App, { target: document.body }); flushSync(); await settle(); open();
      const beforeOld = sent.length;
      receiveStatus('connected'); receive({ type: 'helper.ready', data: { version: '0.6.4', protocolVersion: 7, schemaVersion: 8 } });
      detect(); await settle();
      assert.ok(stopCalls > 0, 'an old-schema helper must be stopped');
      assert.ok(document.body.textContent.includes('Update the helper'), document.body.textContent);
      assert.equal(footprint(), null, 'late displays from an incompatible helper cannot make the editor ready');
      assert.ok(document.querySelector('.geometry-field input').closest('[inert]'));
      assert.equal(sent.length, beforeOld, 'old-schema readiness cannot apply settings');
      await unmount(component);
      console.log('Hotzone geometry UI round trip passed');
    `);
    await build({
      configFile: false, logLevel: "silent", plugins: [svelte()],
      build: { target: "esnext", minify: false, outDir: nodePath.join(directory, "bundle"),
        lib: { entry, formats: ["es"], fileName: () => "entry.mjs" },
        rollupOptions: { external: ["node:assert/strict"] }
      }
    });
    writeFileSync(nodePath.join(directory, "run.mjs"), `
      import { Window } from 'happy-dom';
      const window = new Window({ width: 640, height: 600 });
      for (const name of ['window', 'document', 'localStorage', 'Node', 'Text', 'Comment', 'Element', 'HTMLElement', 'HTMLInputElement', 'HTMLMediaElement', 'Event', 'KeyboardEvent', 'MutationObserver', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame']) {
        globalThis[name] = name === 'window' ? window : window[name];
      }
      Object.defineProperty(globalThis, 'navigator', { value: window.navigator, configurable: true });
      const animate = window.Element.prototype.animate;
      window.Element.prototype.animate = function(...args) {
        const animation = animate.apply(this, args);
        animation.finished.catch(error => { if (error.name !== 'AbortError') throw error; });
        return animation;
      };
      localStorage.setItem('convenient-window-language', 'en-US');
      try { await import('./bundle/entry.mjs'); }
      finally { await window.happyDOM.close(); }
    `);
    const output = execFileSync(process.execPath, [nodePath.join(directory, "run.mjs")], { encoding: "utf8", timeout: 30000, stdio: "pipe" });
    expect(output).toContain("Hotzone geometry UI round trip passed");
  } finally {
    if (nodePath.dirname(nodePath.resolve(directory)) !== nodePath.resolve(import.meta.dirname)) throw new Error("Unexpected test directory");
    rmSync(directory, { recursive: true, force: true });
  }
}, 60000);


it("keeps compiled geometry inputs mounted across readiness while cancelling drafts at area/display boundaries", async () => {
  const directory = mkdtempSync(nodePath.join(import.meta.dirname, ".hotzone-geometry-dom-test-"));
  try {
    const entry = nodePath.join(directory, "entry.ts");
    writeFileSync(nodePath.join(directory, "Harness.svelte"), `
      <script lang="ts">
        import Editor from '../HotzoneGeometryEditor.svelte';
        import type { DisplayInfo, HotzoneSetting, HotzoneGeometry } from '../types';
        export let onChange: (geometry: HotzoneGeometry) => void;
        let state = {
          ready: true, monitorId: 'display-a', edgeSize: 8, language: 'en-US' as const,
          display: { id: 'display-a', primary: true, bounds: { left: 0, top: 0, right: 1920, bottom: 1080 }, workArea: { left: 0, top: 0, right: 1920, bottom: 1040 } } as DisplayInfo | undefined,
          zone: { id: 'top-left', actions: [], geometry: { kind: 'corner', width: 24, height: 12, linked: true } } as HotzoneSetting
        };
        export function update(patch: Partial<typeof state>): void { state = { ...state, ...patch }; }
      </script>
      <Editor {...state} {onChange} onReset={() => {}} onSelectZone={() => {}} />
    `);
    writeFileSync(entry, `
      import assert from 'node:assert/strict';
      import { mount, unmount, flushSync } from 'svelte';
      import Harness from './Harness.svelte';
      const changes = [];
      const component = mount(Harness, { target: document.body, props: { onChange: value => changes.push({ ...value }) } });
      const update = patch => { component.update(patch); flushSync(); };
      const input = name => document.querySelector('input[aria-label="' + name + '"]');
      const typeDraft = (node, value) => {
        node.focus(); node.value = value; node.dispatchEvent(new Event('input', { bubbles: true })); flushSync();
        assert.equal(node.getAttribute('aria-invalid'), 'true');
      };
      const blur = node => { node.dispatchEvent(new Event('blur')); flushSync(); };
      const corner = (id, width = 24, height = 12) => ({ id, actions: [], geometry: { kind: 'corner', width, height, linked: true } });
      const edge = (id, thickness = 8, lengthPercent = 40) => ({ id, actions: [], geometry: { kind: 'edge', thickness, lengthPercent } });
      const displayA = { id: 'display-a', primary: true, bounds: { left: 0, top: 0, right: 1920, bottom: 1080 }, workArea: { left: 0, top: 0, right: 1920, bottom: 1040 } };
      const displayB = { ...displayA, id: 'display-b', primary: false };
      flushSync();
      const section = document.querySelector('.hotzone-geometry');
      const width = input('Width'), height = input('Height');
      assert.ok(width && height);
      const mutations = [];
      const observer = new MutationObserver(records => mutations.push(...records));
      observer.observe(section, { childList: true, subtree: true });
      for (const [node, saved] of [[width, '24'], [height, '12']]) {
        typeDraft(node, '1');
        update({ ready: false, display: undefined });
        assert.ok(input('Width') === width, 'ready=false must preserve the corner width DOM node');
        assert.ok(input('Height') === height, 'ready=false must preserve the corner height DOM node');
        assert.ok(section.hasAttribute('inert'), 'readiness still disables the editor');
        assert.equal(node.value, saved, 'disconnect cancels the unfinished draft on the same node');
        assert.equal(node.getAttribute('aria-invalid'), 'false');
        blur(node);
        update({ ready: true, display: displayA });
        assert.ok(input('Width') === width, 'ready=true must preserve the corner width DOM node');
        assert.ok(input('Height') === height, 'ready=true must preserve the corner height DOM node');
        assert.equal(section.hasAttribute('inert'), false);
        assert.equal(node.value, saved);
        blur(node);
        assert.equal(changes.length, 0, 'late blur must not clamp an old draft into settings');
      }
      typeDraft(width, '1');
      update({ edgeSize: 16 });
      assert.equal(width.value, '1', 'an unrelated update must keep the unfinished draft');
      assert.ok(input('Width') === width, 'the input DOM node must stay mounted');
      update({ zone: corner('top-right') });
      assert.ok(input('Width') === width, 'same-kind zone changes need not recreate controls');
      assert.equal(width.value, '24', 'equal saved values still cancel a prior-zone draft');
      blur(width);
      typeDraft(height, '');
      update({ zone: corner('bottom-left', 48, 36) });
      assert.ok(input('Height') === height, 'the input DOM node must stay mounted');
      assert.equal(height.value, '36', 'a new area supplies its own committed value');
      blur(height);
      typeDraft(width, '1');
      update({ monitorId: 'display-b', display: displayB });
      assert.ok(input('Width') === width, 'the input DOM node must stay mounted');
      assert.equal(width.value, '48', 'equal-valued monitor switches still cancel drafts');
      blur(width);
      typeDraft(width, '1');
      update({ monitorId: 'display-a', display: displayA, zone: corner('top-left') });
      assert.equal(width.value, '24');
      blur(width);
      assert.equal(changes.length, 0, 'area/display changes cannot write an unfinished draft');
      mutations.push(...observer.takeRecords());
      observer.disconnect();
      assert.equal(mutations.some(record => [...record.removedNodes].some(node => node === width || node === height || node.contains?.(width) || node.contains?.(height))), false, 'corner inputs are never removed at readiness/context boundaries');
      update({ zone: edge('right') });
      const thickness = input('Thickness'), length = input('Length');
      const slider = document.querySelector('.geometry-slider');
      for (const [node, saved] of [[thickness, '8'], [length, '40']]) {
        typeDraft(node, '1');
        update({ ready: false, display: undefined });
        assert.ok(input('Thickness') === thickness, 'readiness must preserve the edge thickness node');
        assert.ok(input('Length') === length, 'readiness must preserve the edge length node');
        assert.ok(document.querySelector('.geometry-slider') === slider, 'readiness must preserve the slider node');
        assert.equal(node.value, saved);
        blur(node);
        update({ ready: true, display: displayA });
        assert.ok(input('Thickness') === thickness, 'the input DOM node must stay mounted');
        assert.ok(input('Length') === length, 'the input DOM node must stay mounted');
        blur(node);
      }
      typeDraft(thickness, '1');
      update({ zone: edge('left') });
      assert.ok(input('Thickness') === thickness, 'the input DOM node must stay mounted');
      assert.equal(thickness.value, '8');
      blur(thickness);
      typeDraft(length, '');
      update({ monitorId: 'display-b', display: displayB, zone: edge('left', 16, 60) });
      assert.ok(input('Length') === length, 'the input DOM node must stay mounted');
      assert.equal(length.value, '60');
      blur(length);
      assert.equal(changes.length, 0);
      // Cancellation must not accidentally disable valid edits on the retained controls.
      thickness.value = '20'; thickness.dispatchEvent(new Event('input', { bubbles: true })); flushSync();
      assert.deepEqual(changes, [{ kind: 'edge', thickness: 20, lengthPercent: 60 }]);
      await unmount(component);
      console.log('Geometry retained-DOM and draft boundaries passed');
    `);
    await build({
      configFile: false, logLevel: "silent", plugins: [svelte()],
      build: { target: "esnext", minify: false, outDir: nodePath.join(directory, "bundle"),
        lib: { entry, formats: ["es"], fileName: () => "entry.mjs" },
        rollupOptions: { external: ["node:assert/strict"] }
      }
    });
    writeFileSync(nodePath.join(directory, "run.mjs"), `
      import { Window } from 'happy-dom';
      const window = new Window({ width: 359, height: 544 });
      for (const name of ['window', 'document', 'Node', 'Text', 'Comment', 'Element', 'HTMLElement', 'HTMLInputElement', 'HTMLMediaElement', 'Event', 'KeyboardEvent', 'MutationObserver', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame']) {
        globalThis[name] = name === 'window' ? window : window[name];
      }
      Object.defineProperty(globalThis, 'navigator', { value: window.navigator, configurable: true });
      try { await import('./bundle/entry.mjs'); }
      finally { await window.happyDOM.close(); }
    `);
    const output = execFileSync(process.execPath, [nodePath.join(directory, "run.mjs")], { encoding: "utf8", timeout: 30000, stdio: "pipe" });
    expect(output).toContain("Geometry retained-DOM and draft boundaries passed");
  } finally {
    if (nodePath.dirname(nodePath.resolve(directory)) !== nodePath.resolve(import.meta.dirname)) throw new Error("Unexpected test directory");
    rmSync(directory, { recursive: true, force: true });
  }
}, 60000);
