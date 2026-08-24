import { readFileSync } from "node:fs";
import nodePath from "node:path";
import { describe, expect, it } from "vitest";

const source = readFileSync(nodePath.join(import.meta.dirname, "App.svelte"), "utf8");
const styles = readFileSync(nodePath.join(import.meta.dirname, "styles.css"), "utf8");
const monitorStage = readFileSync(nodePath.join(import.meta.dirname, "MonitorStage.svelte"), "utf8");

describe("window enhancement UI wiring", () => {
  it("uses the unified hotzone master-toggle copy and disabled settings body", () => {
    expect(source).toContain("触发角总开关");
    expect(source).toContain("把屏幕边角变成快捷操作入口");
    expect(source).toContain("触发角设置");
    expect(source).toContain('class="feature-settings-body" class:off={!settings.hotzonesEnabled} inert={!settings.hotzonesEnabled}');
    expect(source).toContain('class:app-disabled={!settings.enabled}');
    expect(styles).toContain(".app-disabled .feature-settings-body");
  });

  it("keeps configured hotzone markers visible in the monitor preview", () => {
    expect(source).toContain("hotzonesEnabled={settings.hotzonesEnabled}");
  });

  it("keeps the selected monitor and edge controls above companion content", () => {
    expect(source).toContain("edgeHideEnabled={settings.edgeHide.enabled}");
    expect(monitorStage).toContain("class:hotzone-layer={display.id === selectedDisplayId && mode === \"hotzones\"}");
    expect(monitorStage).toContain(".physical-monitor.hotzone-layer{z-index:20}");
    expect(monitorStage).toContain(".zone{position:absolute;z-index:30");
    expect(monitorStage).toContain("aria-pressed={edgeHideEdges.includes(edge as Edge)}");
    expect(monitorStage).toContain("pointer-events:auto");
  });

  it("keeps only the focused edge-hide tutorial", () => {
    expect(source).toContain('showFeatureTutorial("edge-hide")');
    expect(source).toContain('id="edge-hide-tutorial"');
    expect(source).not.toContain('showFeatureTutorial("window-drag")');
    expect(source).not.toContain('showFeatureTutorial("topmost-pin")');
    expect(source).not.toContain('id="window-drag-tutorial"');
    expect(source).not.toContain('id="topmost-pin-tutorial"');
  });

  it("animates one window through drag, collapse, hover and restore", () => {
    expect(source).toContain("拖到屏幕外边缘并松开，窗口自动收起；移到露出区域即可恢复。");
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
