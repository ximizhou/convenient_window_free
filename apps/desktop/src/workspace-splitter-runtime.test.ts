import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import nodePath from "node:path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { build } from "vite";
import { expect, it } from "vitest";

it("keeps pane resizing local, bounded, cancellable and persistent per feature", async () => {
  const directory = mkdtempSync(nodePath.join(import.meta.dirname, ".workspace-splitter-test-"));
  try {
    writeFileSync(nodePath.join(directory, "Harness.svelte"), `
      <script>
        import WorkspaceSplitter from '../WorkspaceSplitter.svelte';
        export let workspace;
        let mode = 'hotzones';
        export function switchMode(value) { mode = value; }
      </script>
      <WorkspaceSplitter {workspace} {mode} label="Preview width" resetLabel="Double-click or Home to reset" />
    `);
    const entry = nodePath.join(directory, "entry.ts");
    writeFileSync(entry, `
      import assert from 'node:assert/strict';
      import { mount, unmount, flushSync } from 'svelte';
      import Harness from './Harness.svelte';

      const key = 'convenient-window-pane-widths';
      const property = '--user-drawer-width';
      const workspace = document.createElement('div');
      const drawer = document.createElement('aside');
      drawer.className = 'drawer';
      workspace.append(drawer); document.body.append(workspace);
      let width = 1000;
      workspace.getBoundingClientRect = () => ({ width, left: 0, right: width, top: 0, bottom: 600, height: 600 });
      drawer.getBoundingClientRect = () => ({ width: Number.parseFloat(workspace.style.getPropertyValue(property)) || 360 });
      const applied = () => workspace.style.getPropertyValue(property);
      const stored = () => JSON.parse(localStorage.getItem(key) || '{}');
      const settle = async () => {
        for (let i = 0; i < 5; i++) { await Promise.resolve(); flushSync(); globalThis.flushFrames(); }
        flushSync();
      };
      const keypress = (key) => { document.querySelector('[role="separator"]').dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })); flushSync(); };
      const resize = async (next) => { width = next; window.innerWidth = next; window.dispatchEvent(new Event('resize')); await settle(); };
      let component = mount(Harness, { target: workspace, props: { workspace } });
      await settle();
      let handle = document.querySelector('[role="separator"]');
      assert.equal(handle.hidden, false);
      assert.equal(applied(), '', 'first use must preserve the existing CSS default');
      assert.equal(localStorage.getItem(key), null, 'mounting must not create a width preference');
      assert.equal(handle.getAttribute('aria-valuenow'), '640');
      keypress('ArrowLeft'); await settle();
      assert.equal(applied(), '376px');
      assert.equal(stored().hotzones, 376);
      assert.equal(handle.getAttribute('aria-valuenow'), '624', 'left arrow moves the separator left');
      keypress('Home'); await settle();
      assert.equal(applied(), '', 'Home restores the CSS default');
      assert.equal(stored().hotzones, undefined);

      const captures = new Set();
      const capture = () => {
        handle = document.querySelector('[role="separator"]');
        handle.setPointerCapture = id => captures.add(id);
        handle.hasPointerCapture = id => captures.has(id);
        handle.releasePointerCapture = id => { captures.delete(id); handle.dispatchEvent(new PointerEvent('lostpointercapture', { pointerId: id })); };
      };
      capture();
      const pointer = (target, type, x, id = 7) => target.dispatchEvent(new PointerEvent(type, { pointerId: id, isPrimary: true, button: 0, clientX: x, bubbles: true, cancelable: true }));
      pointer(handle, 'pointerdown', 640);
      assert.equal(workspace.classList.contains('resizing'), true);
      pointer(window, 'pointermove', 600); pointer(window, 'pointermove', 580); pointer(window, 'pointermove', 560);
      assert.equal(applied(), '', 'pointer moves are coalesced until the animation frame');
      assert.equal(globalThis.pendingFrames(), 1);
      await settle();
      assert.equal(applied(), '440px');
      assert.equal(stored().hotzones, undefined, 'intermediate frames do not write preferences');
      pointer(window, 'pointerup', 540); await settle();
      assert.equal(applied(), '460px', 'pointerup flushes the final position even without a frame');
      assert.equal(stored().hotzones, 460);
      assert.equal(workspace.classList.contains('resizing'), false);
      assert.equal(captures.size, 0);

      pointer(handle, 'pointerdown', 540); pointer(window, 'pointermove', 400); await settle();
      assert.equal(applied(), '600px');
      pointer(handle, 'lostpointercapture', 400); await settle();
      assert.equal(applied(), '460px', 'lost capture cancels a preview instead of committing it');
      assert.equal(stored().hotzones, 460);
      assert.equal(workspace.classList.contains('resizing'), false);

      for (const interruption of ['pointercancel', 'blur', 'Escape']) {
        pointer(handle, 'pointerdown', 540); pointer(window, 'pointermove', 440); await settle();
        assert.equal(applied(), '560px');
        if (interruption === 'pointercancel') pointer(window, interruption, 440);
        else if (interruption === 'blur') window.dispatchEvent(new Event('blur'));
        else keypress('Escape');
        await settle();
        assert.equal(applied(), '460px', interruption + ' must restore the last committed width');
        assert.equal(stored().hotzones, 460);
        assert.equal(workspace.classList.contains('resizing'), false);
      }

      component.switchMode('gestures'); await settle();
      keypress('ArrowRight'); await settle();
      assert.equal(applied(), '560px', 'gestures keep their usable editor minimum');
      assert.equal(stored().gestures, 560);
      assert.equal(stored().hotzones, 460, 'each feature keeps its own width');
      component.switchMode('startup'); await settle();
      keypress('ArrowRight'); await settle();
      assert.equal(applied(), '400px', 'startup entries retain their minimum width');
      component.switchMode('edge-hide'); await settle();
      keypress('ArrowLeft'); await settle();
      component.switchMode('hotzones'); await settle();
      assert.equal(applied(), '460px');
      component.switchMode('edge-hide'); await settle();
      assert.equal(applied(), '376px');

      component.switchMode('hotzones'); await settle();
      capture();
      pointer(handle, 'pointerdown', 540); pointer(window, 'pointerup', -200); await settle();
      assert.equal(applied(), '820px', 'the preview keeps at least 180px');
      assert.equal(stored().hotzones, 820);
      await resize(800);
      assert.equal(applied(), '620px');
      assert.equal(stored().hotzones, 820, 'window resize never overwrites the preferred width');
      await resize(700);
      assert.equal(handle.hidden, true);
      assert.equal(applied(), '', 'narrow viewports retain the original full-width drawer');
      await resize(1000);
      assert.equal(applied(), '820px', 'restoring the window restores the saved width');
      component.switchMode('gestures'); await settle();
      await resize(720);
      assert.equal(handle.hidden, true, 'hide when gesture editor and preview cannot both fit');
      assert.equal(applied(), '');
      await resize(1000);

      capture();
      pointer(handle, 'pointerdown', 440); pointer(window, 'pointermove', 300); await settle();
      component.switchMode('startup'); await settle();
      assert.equal(applied(), '400px', 'changing feature cancels the previous drag');
      assert.equal(workspace.classList.contains('resizing'), false);
      assert.equal(stored().gestures, 560);
      pointer(window, 'pointerup', 200); await settle();
      assert.equal(stored().startup, 400, 'a late pointerup cannot write another feature');

      handle.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })); await settle();
      assert.equal(applied(), '', 'double click resets only the current feature');
      assert.equal(stored().startup, undefined);
      assert.equal(stored().hotzones, 820);
      component.switchMode('hotzones'); await settle();
      capture();
      pointer(handle, 'pointerdown', 180); pointer(window, 'pointermove', 280);
      await unmount(component); await settle();
      assert.equal(applied(), '');
      assert.equal(workspace.classList.contains('resizing'), false);
      assert.equal(globalThis.pendingFrames(), 0);
      pointer(window, 'pointerup', 400); await settle();
      assert.equal(stored().hotzones, 820, 'unmount removes drag listeners and cancels pending work');

      localStorage.setItem(key, JSON.stringify({ hotzones: 510, gestures: -5, power: '400', unknown: 600, more: null }));
      component = mount(Harness, { target: workspace, props: { workspace } }); await settle();
      assert.equal(applied(), '510px', 'saved widths survive remount');
      keypress('ArrowRight'); await settle();
      assert.deepEqual(stored(), { hotzones: 494 }, 'ignore unknown modes and invalid persisted values');
      for (let i = 0; i < 20; i++) keypress('ArrowRight');
      await settle();
      assert.equal(applied(), '310px', 'regular panes retain their minimum width');
      assert.equal(document.querySelector('[role="separator"]').getAttribute('aria-valuenow'), '690');
      await unmount(component); await settle();
      console.log('workspace splitter runtime regression passed');
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
      window.innerWidth = 1000;
      const frames = new Map(); let nextFrame = 0;
      window.requestAnimationFrame = callback => { const id = ++nextFrame; frames.set(id, callback); return id; };
      window.cancelAnimationFrame = id => frames.delete(id);
      globalThis.flushFrames = () => { const batch = [...frames]; frames.clear(); for (const [, callback] of batch) callback(0); };
      globalThis.pendingFrames = () => frames.size;
      class ResizeObserver { observe() {} disconnect() {} }
      window.ResizeObserver = ResizeObserver;
      for (const name of ['window', 'document', 'localStorage', 'Node', 'Text', 'Comment', 'Element', 'HTMLElement', 'HTMLInputElement', 'HTMLMediaElement', 'Event', 'PointerEvent', 'KeyboardEvent', 'MouseEvent', 'MutationObserver', 'ResizeObserver', 'getComputedStyle', 'requestAnimationFrame', 'cancelAnimationFrame']) {
        globalThis[name] = name === 'window' ? window : window[name];
      }
      Object.defineProperty(globalThis, 'navigator', { value: window.navigator, configurable: true });
      try { await import('./bundle/entry.mjs'); }
      finally { await window.happyDOM.close(); }
    `);
    const output = execFileSync(process.execPath, [runFile], { encoding: "utf8", timeout: 30000, stdio: "pipe" });
    expect(output).toContain("workspace splitter runtime regression passed");
  } finally {
    if (nodePath.dirname(nodePath.resolve(directory)) !== nodePath.resolve(import.meta.dirname)) throw new Error("Unexpected test directory");
    rmSync(directory, { recursive: true, force: true });
  }
}, 60000);
