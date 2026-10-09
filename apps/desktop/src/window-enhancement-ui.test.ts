import { readFileSync } from "node:fs";
import nodePath from "node:path";
import { describe, expect, it } from "vitest";
import * as i18n from "./i18n";

const source = readFileSync(nodePath.join(import.meta.dirname, "App.svelte"), "utf8");
const styles = readFileSync(nodePath.join(import.meta.dirname, "styles.css"), "utf8");
const monitorStage = readFileSync(nodePath.join(import.meta.dirname, "MonitorStage.svelte"), "utf8");

describe("window enhancement UI wiring", () => {
  it("keeps one dictionary for both hosts with matching keys and a system-language default", () => {
    // 两套前端各持一份逐字节一致的 src/i18n.ts；这里校验宿主侧契约，
    // 跨宿主的哈希一致由私有仓库 scripts/check-i18n-parity.mjs 负责。
    expect(i18n.LANGUAGE_KEY).toBe("convenient-window-language");
    expect(Object.keys(i18n.en).sort()).toEqual(Object.keys(i18n.zh).sort());
    expect(i18n.zh.power).toBe("运行中心");
    expect(i18n.en.power).toBe("Center");
    expect(i18n.translator("en-US")("edgeHide")).toBe("Windows");
    expect(i18n.format(i18n.zh.sampleCount, { count: 3 })).toBe("3 个样本");
    // 没有保存过选择时跟随系统：zh* 中文，其余英文；已保存的选择优先。
    expect(i18n.resolveInitialLanguage(null, "zh-Hans")).toBe("zh-CN");
    expect(i18n.resolveInitialLanguage(undefined, "en-GB")).toBe("en-US");
    expect(i18n.resolveInitialLanguage("zh-CN", "en-US")).toBe("zh-CN");
    expect(source).toContain('$: ui = translator(language);');
    expect(source).toContain("resolveInitialLanguage(stored, typeof navigator === \"undefined\" ? undefined : navigator.language)");
    expect(source).toContain('value="en-US"');
    // 语言相关的界面文案必须走字典，不再散落中文字面量。
    expect(source).toContain('{ui("language")}');
    expect(source).toContain("English");
  });

  it("uses the unified hotzone master-toggle copy and disabled settings body", () => {
    expect(source).toContain('ui("hotzoneMaster")');
    expect(source).toContain('ui("hotzoneTitle")');
    expect(source).toContain('ui("hotzoneSettings")');
    expect(i18n.zh.hotzoneMaster).toBe("触发角总开关");
    expect(i18n.zh.hotzoneTitle).toBe("把屏幕边角变成快捷操作入口");
    expect(i18n.zh.hotzoneSettings).toBe("触发角设置");
    expect(i18n.en.hotzoneMaster).toBe("Corner switch");
    expect(source).toContain('class="feature-settings-body" class:off={!settings.hotzonesEnabled} inert={!settings.hotzonesEnabled}');
    expect(source).toContain('class:app-disabled={!settings.enabled}');
    expect(styles).toContain(".app-disabled .feature-settings-body");
  });

  it("keeps configured hotzone markers visible in the monitor preview", () => {
    expect(source).toContain("hotzonesEnabled={settings.hotzonesEnabled}");
    expect(source).toContain("displayReady");
    expect(source).toContain('ui("checkConnection")');
  });

  it("keeps the selected monitor and edge controls above companion content", () => {
    expect(source).toContain("edgeHideEnabled={settings.edgeHide.enabled}");
    expect(monitorStage).toContain("class:hotzone-layer={display.id === selectedDisplayId && mode === \"hotzones\"}");
    expect(monitorStage).toContain(".physical-monitor.hotzone-layer{z-index:20}");
    expect(monitorStage).toContain(".zone{position:absolute;z-index:30");
    expect(monitorStage).toContain("aria-pressed={edgeHideEdges.includes(edge as Edge)}");
    expect(monitorStage).toContain("pointer-events:auto");
    expect(monitorStage).toContain(".window-demo button.configured:not(.active){outline:");
    expect(monitorStage).not.toContain(".window-demo button::before");
  });

  it("updates edge controls from a reactive immutable monitor profile", () => {
    expect(monitorStage).toContain("on:click={() => onToggleEdge(edge as Edge)}");
    expect(source).toContain("currentDisplayEdgeHideEdges = settings.edgeHide.monitorProfiles.find(");
    expect(source).toContain("edgeHideEdges={currentDisplayEdgeHideEdges}");
    expect(source).not.toContain("edgeHideEdges={currentEdgeHideEdges()}");
    expect(source).toContain("settings = {");
    expect(source).toContain("monitorProfiles: profile");
  });

  it("labels fallback display data as a preview and only marks live data online", () => {
    expect(source).toContain("let displayReady = false");
    expect(source).toContain("let helperError = \"\"");
    expect(source).toContain('ui("checkConnection")');
    expect(source).toContain("class:ready={displayReady}");
    expect(source).toContain("displayReady={displayReady}");
    expect(styles).toContain(".display-picker i { width: 7px; height: 7px; border-radius: 50%; background: var(--faint);");
    expect(styles).toContain(".display-picker i.ready");
    expect(monitorStage).toContain("class:preview-monitor={!displayReady}");
    expect(monitorStage).toContain("等待后台助手提供显示器信息");
  });

  it("keeps disabled settings readable and preserves a persistent error rail", () => {
    expect(styles).toContain(".feature-settings-body.off { cursor: not-allowed; opacity: 1;");
    expect(styles).toContain(".app-disabled .feature-settings-body { opacity: 1;");
    expect(source).toContain("class:error={Boolean(helperError)}");
    expect(source).toContain('if (status === "connected") lastMessage =');
    expect(source).toMatch(/function markHelperReady\(\): void \{\r?\n    helperError = ""/);
    expect(source).toContain('aria-live="polite"');
    expect(styles).toContain(".status-rail.error");
  });

  it("keeps only the focused edge-hide tutorial", () => {
    expect(source).toContain('showFeatureTutorial("edge-hide")');
    expect(source).toContain('id="edge-hide-tutorial"');
    expect(source).not.toContain('showFeatureTutorial("window-drag")');
    expect(source).not.toContain('showFeatureTutorial("topmost-pin")');
    expect(source).not.toContain('id="window-drag-tutorial"');
    expect(source).not.toContain('id="topmost-pin-tutorial"');
  });

  it("exposes the restore outline toggle", () => {
    expect(source).toContain('ui("expansionOutline")');
    expect(source).toContain('ui("expansionOutlineDescription")');
    expect(i18n.zh.expansionOutlineDescription).toBe("关闭只隐藏窗口收纳后的淡白轮廓，边缘恢复仍可触发");
    expect(i18n.en.expansionOutline).toBe("Show expand outline");
    expect(source).toContain("bind:checked={settings.edgeHide.showRestoreHint}");
  });

  it("animates one window through drag, collapse, hover and restore", () => {
    expect(source).toContain('ui("edgeHideTutorialBody")');
    expect(i18n.zh.edgeHideTutorialBody).toBe("拖到屏幕外边缘并松开，窗口自动收起；移到露出区域即可恢复。");
    expect(source.match(/class="tutorial-edge-window"/g)).toHaveLength(1);
    expect(source).toContain('class="tutorial-cursor"');
    expect(styles).toContain("animation: tutorial-edge-window-cycle");
    expect(styles).toContain("animation: tutorial-cursor-cycle");
    expect(styles).toContain("@keyframes tutorial-edge-window-cycle");
    expect(styles).toContain("@keyframes tutorial-cursor-cycle");
    expect(styles).toContain(".feature-help { position: relative;");
    expect(styles).toContain("right: -112px;");
    expect(styles).toContain("top: calc(100% + 6px);");
    expect(styles).toContain(".feature-help-button::before");
    expect(styles).toContain('content: "?";');
    expect(styles).toContain("inset: 4px;");
    expect(styles).toContain("border-radius: 50%;");
  });

  it("auto-saves mouse gesture edits without a manual apply button", () => {
    for (const handler of ["createGesture", "recordGesture", "renameGesture", "duplicateGesture", "deleteGesture", "clearGestureSamples", "toggleGestureEnabled", "setGestureActionPreset", "setGestureActionValue"]) {
      const handlerSource = source.match(new RegExp(`function ${handler}\\([\\s\\S]*?\\n  }`))?.[0];
      expect(handlerSource, `${handler} should persist its edit`).toContain("persist(");
    }
    expect(source).not.toContain("保存并应用鼠标增强");
    expect(source).not.toContain('class="apply gesture-apply"');
    expect(styles).not.toContain(".gesture-apply");
  });
});


it("auto-saves the localized edge animation switch without changing delays", () => {
  expect(source).toContain('bind:checked={settings.edgeHide.animationEnabled} on:change={() => persist()}');
  expect(source).toContain('aria-label={ui("edgeAnimation")}');
  expect(source).toContain('settings.edgeHide.collapseDelayMs');
  expect(source).toContain('settings.edgeHide.restoreDelayMs');
});


describe("per-area geometry UI boundaries", () => {
  it("replaces the global size input without adding a feature tab", () => {
    expect(source).toContain('import HotzoneGeometryEditor from "./HotzoneGeometryEditor.svelte"');
    expect(source).toContain('onChange={setHotzoneGeometry}');
    expect(source).toContain('onReset={resetHotzoneGeometry}');
    expect(source).toContain('ready={displayReady}');
    expect(source).toContain('delete zone.geometry');
    expect(source).toContain('geometry: { ...zone.geometry }');
    expect(source).not.toContain('value: settings.edgeSize, onChange:');
    expect(source).toContain('isSupportedHelperSchema(data?.schemaVersion)');
    expect(i18n.zh.geometryScope).toContain("不改变动作");
    expect(i18n.en.geometryLinked).toBe("Keep the current aspect ratio");
    expect(styles).toContain('.geometry-field input { display: block; width: 100%; min-width: 0;');
    expect(monitorStage).toContain('pointer-events:none');
    expect(monitorStage).toContain('hotzonePreviewRect(selectedZone, selectedDisplay.bounds, edgeSize, selectedGeometry)');
  });
});


describe("drawer scroll layout contract", () => {
  const bodyRules = [...styles.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
    .filter(([, selectors]) => selectors.trim().endsWith(".drawer-body"))
    .map(([, selectors, declarations]) => ({
      selector: selectors.trim(),
      declarations: Object.fromEntries(declarations.split(";").filter(part => part.includes(":")).map(part => {
        const separator = part.indexOf(":");
        return [part.slice(0, separator).trim(), part.slice(separator + 1).trim()];
      }))
    }));

  it("keeps long cards in normal block flow and does not hide horizontal overflow", () => {
    const base = bodyRules.find(rule => rule.selector === ".drawer-body")?.declarations;
    expect(base).toBeDefined();
    expect(base?.display).toBe("block");
    expect(base?.["min-height"]).toBe("0");
    expect(base?.["overflow-y"]).toBe("auto");
    expect(base?.["overflow-x"]).toBe("auto");
    expect(base?.["scrollbar-gutter"]).toBe("stable");
    for (const rule of bodyRules) {
      expect(rule.declarations.display, rule.selector).not.toBe("flex");
      for (const key of ["overflow", "overflow-x", "overflow-y"]) {
        expect(rule.declarations[key] ?? "", rule.selector + " " + key).not.toMatch(/hidden|clip/);
      }
    }
  });

  it("retains the bottom scroll clearance in narrow and gesture drawer overrides", () => {
    expect(bodyRules.length).toBeGreaterThanOrEqual(4);
    for (const rule of bodyRules) {
      // A later shorthand must not silently replace the base bottom clearance.
      if (rule.declarations.padding) {
        expect(rule.declarations.padding, rule.selector).toMatch(/calc\(28px \+ env\(safe-area-inset-bottom\)\)/);
      }
      expect(rule.declarations["padding-bottom"], rule.selector).toBeUndefined();
    }
  });
});
