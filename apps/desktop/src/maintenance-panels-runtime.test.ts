import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import nodePath from "node:path";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { build } from "vite";
import { expect, it } from "vitest";

it("keeps diagnostic exports private and offline-profile confirmation responsive to language, topology and save failures", async () => {
  const directory = mkdtempSync(nodePath.join(import.meta.dirname, ".maintenance-panels-test-"));
  try {
    const entry = nodePath.join(directory, "entry.ts");
    writeFileSync(nodePath.join(directory, "Wrapper.svelte"), `
      <script lang="ts">
        import DiagnosticsPanel from '../DiagnosticsPanel.svelte';
        import OfflineMonitorProfiles from '../OfflineMonitorProfiles.svelte';
        import { buildDiagnosticReport } from '../diagnostics';
        import { defaultSettings } from '../settings-store';
        let english = false;
        let ready = false;
        let failReport = false;
        let failSave = false;
        let calls = 0;
        let nativeSave = undefined;
        let settings = structuredClone(defaultSettings);
        settings.monitorProfiles = ['online', 'offline-a', 'offline-b'].map(monitorId => ({monitorId, hotzones: structuredClone(defaultSettings.hotzones)}));
        settings.edgeHide.monitorProfiles = [{monitorId:'offline-a', edges:['left']}];
        let displays = [{id:'online',primary:true,bounds:{left:0,top:0,right:1920,bottom:1080},workArea:{left:0,top:0,right:1920,bottom:1040}}];
        export function setEnglish(value) { english = value; }
        export function setReady(value) { ready = value; }
        export function setReportFailure(value) { failReport = value; }
        export function setSaveFailure(value) { failSave = value; }
        export function setNativeSave(value) { nativeSave=value; }
        export function reconnect() { displays = [...displays,{...displays[0],id:'offline-a'}]; }
        export function inspect() { return {calls,settings}; }
        async function collect() {
          if (failReport) throw new Error('secret failure');
          return buildDiagnosticReport({host:'utools',appVersion:'0.6.9',rawDiagnostics:{helperPath:'private-path',lastError:'EACCES secret-private-path'},displays});
        }
        async function save(next) { calls++; if(failSave) return false; settings=next; return true; }
      </script>
      <DiagnosticsPanel {english} {collect} saveReport={nativeSave}/>
      <OfflineMonitorProfiles {settings} {displays} {ready} {english} onRemove={save}/>
    `);
    writeFileSync(entry, `
      import assert from 'node:assert/strict';
      import { mount, unmount, flushSync } from 'svelte';
      import Wrapper from './Wrapper.svelte';
      const settle = async () => { for(let i=0;i<8;i++){ await Promise.resolve(); flushSync(); } };
      const click = async selector => { const node=document.querySelector(selector); assert.ok(node,selector); node.click(); await settle(); };
      const instance=mount(Wrapper,{target:document.body}); flushSync(); await settle();
      assert.match(document.querySelector('.profile-empty').textContent,/连接后台助手/);
      assert.equal(document.querySelectorAll('.offline-profiles li').length,0);
      await click('.diagnostics-heading button');
      const report=document.querySelector('.diagnostics-panel textarea').value;
      assert.ok(!report.includes('private-path'));
      await click('.diagnostics-actions button');
      assert.equal(globalThis.copiedText,report);
      await click('.diagnostics-actions button:last-child');
      assert.equal(await globalThis.downloadBlob.text(),report);
      assert.equal(globalThis.downloadName,'convenient-window-diagnostics.json');
      assert.match(document.querySelector('.diagnostics-feedback').textContent,/已请求下载/);
      instance.setNativeSave(async()=>false); await settle(); await click('.diagnostics-actions button:last-child');
      assert.equal(document.querySelector('.diagnostics-feedback'),null,'cancel is not a successful save');
      instance.setNativeSave(async()=>{throw Error('secret native-save-path');}); await settle(); await click('.diagnostics-actions button:last-child');
      assert.match(document.querySelector('.diagnostics-feedback').textContent,/无法保存/);
      instance.setNativeSave(async json=>{assert.equal(json,report);return true;}); await settle(); await click('.diagnostics-actions button:last-child');
      assert.match(document.querySelector('.diagnostics-feedback').textContent,/报告已保存/);
      instance.setReady(true); await settle();
      assert.equal(document.querySelectorAll('.offline-profiles li').length,2);
      await click('.profile-row button');
      assert.ok(document.querySelector('.profile-confirm'));
      instance.setEnglish(true); await settle();
      assert.equal((document.body.textContent.match(/[\\u4e00-\\u9fff]+/g)||[]).length,0,'all live labels including confirmation and feedback switch');
      instance.reconnect(); await settle();
      assert.equal(document.querySelectorAll('.offline-profiles li').length,1);
      assert.equal(document.querySelector('.profile-confirm'),null,'reconnected screen confirmation is invalidated');
      assert.equal(instance.inspect().calls,0);
      instance.setSaveFailure(true); await click('.profile-row button'); await click('.profile-confirm .danger');
      assert.match(document.querySelector('.offline-profiles [role=status]').textContent,/not removed/);
      assert.equal(instance.inspect().settings.monitorProfiles.length,3,'failed save preserves the original state');
      instance.setSaveFailure(false); await click('.profile-confirm .danger');
      assert.equal(document.querySelectorAll('.offline-profiles li').length,0);
      assert.deepEqual(instance.inspect().settings.monitorProfiles.map(x=>x.monitorId),['online','offline-a']);
      instance.setReportFailure(true); await click('.diagnostics-heading button');
      assert.equal(document.querySelector('.diagnostics-panel textarea'),null,'failed refresh cannot expose a stale report');
      assert.match(document.querySelector('.diagnostics-feedback').textContent,/Could not generate/);
      instance.setEnglish(false); await settle();
      assert.match(document.querySelector('.diagnostics-feedback').textContent,/无法生成/);
      await unmount(instance);
      console.log('maintenance panels runtime passed');
    `);
    await build({ configFile: false, logLevel: "silent", plugins: [svelte()], build: { target: "esnext", minify: false, outDir: nodePath.join(directory, "bundle"), lib: { entry, formats: ["es"], fileName: () => "entry.mjs" }, rollupOptions: { external: ["node:assert/strict"] } } });
    const runFile = nodePath.join(directory, "run.mjs");
    writeFileSync(runFile, `
      import {Window} from 'happy-dom';
      const window=new Window();
      for(const name of ['window','document','Node','Text','Comment','Element','HTMLElement','HTMLInputElement','HTMLMediaElement','Event','MutationObserver']) globalThis[name]=name==='window'?window:window[name];
      Object.defineProperty(globalThis,'navigator',{value:window.navigator,configurable:true});
      Object.defineProperty(navigator,'clipboard',{value:{writeText:async text=>{globalThis.copiedText=text;}},configurable:true});
      URL.createObjectURL=blob=>{globalThis.downloadBlob=blob; return 'blob:test-report';};
      URL.revokeObjectURL=()=>{};
      window.HTMLAnchorElement.prototype.click=function(){globalThis.downloadName=this.download;};
      try {await import('./bundle/entry.mjs');} finally {await window.happyDOM.close();}
    `);
    expect(execFileSync(process.execPath, [runFile], { encoding: "utf8", timeout: 30000 })).toContain("maintenance panels runtime passed");
  } finally {
    if (nodePath.dirname(nodePath.resolve(directory)) !== nodePath.resolve(import.meta.dirname)) throw new Error("Unexpected test directory");
    rmSync(directory, { recursive: true, force: true });
  }
}, 60000);
