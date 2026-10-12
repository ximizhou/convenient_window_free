import { describe, expect, it } from "vitest";
import fixture from "../../../tests/fixtures/i18n-migration.json";
import errorCodes from "../../../tests/fixtures/runtime-errors.json";
import { normalizeSettings, defaultSettings } from "./settings-store";
import { gestureDisplayName, gestureLabels, gestureOverlayName } from "./gesture-names";
import { runtimeErrorKeys, runtimeErrorText } from "./runtime-error";
import { zh, en, translator } from "./i18n";
import type { AppSettings } from "./types";

describe("schema 9 and protocol 7 localization contract", () => {
  it("keeps overlay names short and follows reassigned actions", () => {
    const gesture = structuredClone(defaultSettings.mouseGestures.gestures[1]);
    expect(gestureOverlayName(gesture, translator("zh-CN"))).toBe("粘贴");
    expect(gestureOverlayName(gesture, translator("en-US"))).toBe("Paste");
    gesture.action = { kind: "shortcut", value: "Alt+F4" };
    expect(gestureOverlayName(gesture, translator("zh-CN"))).toBe("关闭窗口");
    gesture.action = { kind: "shortcut", value: "Ctrl+K" };
    expect(gestureOverlayName(gesture, translator("zh-CN"))).toBe("Ctrl+K");
    gesture.action = { kind: "__proto__" };
    expect(gestureOverlayName(gesture, translator("zh-CN"))).toBe("");
    gesture.name = "整理窗口";
    expect(gestureOverlayName(gesture, translator("zh-CN"))).toBe("整理窗口");
  });
  it("migrates the shared legacy fixture without changing user actions, samples or names", () => {
    const migrated = normalizeSettings(fixture.input as Partial<AppSettings>);
    expect(migrated.schemaVersion).toBe(9);
    for (const [id, name] of Object.entries(fixture.expectedNames)) {
      const gesture = migrated.mouseGestures.gestures.find(item => item.id === id)!;
      expect(gesture.name ?? null).toBe(name);
      expect(gesture.enabled).toBe(false);
      expect(gesture.action).toEqual({ kind: "shortcut", value: "Ctrl+K" });
      expect(gesture.modifierActions).toEqual([{ modifiers: ["alt"], action: { kind: "shortcut", value: "Ctrl+Q" } }]);
      expect(gesture.samples).toHaveLength(1);
      expect(gesture.samples[0][0]).toEqual({ x: 0, y: 0 });
      expect(gesture.samples[0].at(-1)).toEqual({ x: 0, y: 1 });
      expect(gesture.samples[0].every(point => point.x === 0 && point.y >= 0 && point.y <= 1)).toBe(true);
    }
    expect(migrated.mouseGestures.gestures.find(item => item.id === "custom")?.builtin).toBe(false);
    expect(migrated.mouseGestures.gestures.find(item => item.id === "gesture-rectangle")?.mode).toBe("region-screenshot");
    expect(normalizeSettings(JSON.parse(JSON.stringify(migrated)))).toEqual(migrated);
  });

  it("uses the ID for default names and retains explicit schema 8 overrides", () => {
    const stored = structuredClone(defaultSettings);
    stored.schemaVersion = 8;
    const builtin = stored.mouseGestures.gestures[0];
    expect(builtin.name).toBeUndefined();
    expect(gestureDisplayName(builtin, translator("en-US"))).toBe("Up · Copy");
    expect(gestureDisplayName(builtin, translator("zh-CN"))).toBe("向上 · 复制");
    builtin.name = "向上 · 复制";
    expect(normalizeSettings(stored).mouseGestures.gestures[0].name).toBe(builtin.name);
    expect(gestureDisplayName(builtin, translator("en-US"))).toBe(builtin.name);
    expect(gestureLabels(stored.mouseGestures.gestures, translator("en-US"))[builtin.id]).toBe(builtin.name);
    expect(JSON.stringify(defaultSettings)).not.toContain("向上");
  });

  it("rejects future configuration instead of silently downgrading it", () => {
    expect(() => normalizeSettings({ schemaVersion: 10 })).toThrow("Unsupported settings schema");
  });

  it("translates every wire error and never displays diagnostic prose", () => {
    expect(Object.keys(runtimeErrorKeys).sort()).toEqual([...errorCodes].sort());
    expect(Object.keys(en).sort()).toEqual(Object.keys(zh).sort());
    for (const code of [...errorCodes, "future_error", "__proto__"]) {
      for (const language of ["zh-CN", "en-US"] as const) {
        const text = runtimeErrorText({ code, details: "PRIVATE_DIAGNOSTIC" }, translator(language));
        expect(text).not.toContain("PRIVATE_DIAGNOSTIC");
        expect(text).not.toBe(code);
        expect(text.length).toBeGreaterThan(0);
      }
    }
    expect(runtimeErrorText(null, translator("en-US"))).toBe(en.errorRuntime);
  });
});
