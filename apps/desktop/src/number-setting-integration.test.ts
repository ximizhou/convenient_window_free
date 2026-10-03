import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import nodePath from "node:path";
import { compile } from "svelte/compiler";
import { expect, it } from "vitest";

it("mounts the Svelte action with ranges, saves each current value, and switches draft context", () => {
  const directory = mkdtempSync(nodePath.join(import.meta.dirname, ".number-setting-test-"));
  try {
    const source = `<script>
      import { numberSetting } from '../number-setting.ts';
      export let onSave;
      let value = 700;
      let field = 'first';
      let unrelated = 0;
      let bound = 300;
    </script>
    <input class="number" use:numberSetting={{key: field, value, onChange: (next) => { value = next; onSave(next); }}} min="10" max="5000" type="number" />
    <button class="other" on:click={() => { unrelated++; value = value; }}>{unrelated}</button>
    <button class="switch" on:click={() => { field = 'second'; }}>Switch</button>
    <button class="reset" on:click={() => { value = 350; }}>Reset</button>
    <input class="bound" type="number" bind:value={bound} on:input={() => onSave(bound)} />`;
    writeFileSync(nodePath.join(directory, "Fixture.mjs"), compile(source, { generate: "client" }).js.code);
    writeFileSync(nodePath.join(directory, "run.mjs"), `
      import assert from 'node:assert/strict';
      import { Window } from 'happy-dom';
      const window = new Window();
      for (const name of ['window', 'document', 'Node', 'Text', 'Comment', 'Element', 'HTMLElement', 'HTMLInputElement', 'HTMLMediaElement', 'Event', 'MutationObserver', 'getComputedStyle']) {
        globalThis[name] = name === 'window' ? window : window[name];
      }
      const { mount, unmount, flushSync } = await import('svelte');
      const { default: Fixture } = await import('./Fixture.mjs');
      const values = [];
      const component = mount(Fixture, { target: document.body, props: { onSave: value => values.push(value) } });
      flushSync();
      const input = document.querySelector('.number');
      assert.equal(input.title, '10 ~ 5000');
      input.focus();
      const type = text => { input.value = text; input.dispatchEvent(new Event('input')); flushSync(); };
      type('5');
      assert.equal(input.value, '5');
      assert.deepEqual(values, []);
      document.querySelector('.other').click();
      flushSync();
      assert.equal(input.value, '5');
      type(input.value + '0');
      type(input.value + '0');
      assert.equal(input.value, '500');
      assert.deepEqual(values, [50, 500]);
      type('5');
      document.querySelector('.switch').click();
      flushSync();
      assert.equal(input.value, '500');
      input.blur();
      assert.deepEqual(values, [50, 500]);
      document.querySelector('.reset').click();
      flushSync();
      assert.equal(input.value, '350');
      const bound = document.querySelector('.bound');
      bound.value = '500';
      bound.dispatchEvent(new Event('input'));
      flushSync();
      assert.deepEqual(values, [50, 500, 500]);
      await unmount(component);
      await window.happyDOM.close();
      console.log('Svelte numeric integration passed');
    `);
    const output = execFileSync(process.execPath, ["--conditions=browser", nodePath.join(directory, "run.mjs")], { encoding: "utf8" });
    expect(output).toContain("Svelte numeric integration passed");
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
