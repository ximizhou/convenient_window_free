import { afterEach, describe, expect, it, vi } from "vitest";
import {
  defaultSettings,
  MAX_GESTURE_TEMPLATES,
  MAX_SETTINGS_STORAGE_BYTES,
  normalizeSettings,
  saveSettings
} from "./settings-store";
import contractFixture from "../../../tests/fixtures/config-contract.json";
import type { AppSettings } from "./types";

const hostBridgeState = vi.hoisted(() => ({ current: null as null | { saveSettings(settings: unknown): Promise<void> } }));

vi.mock("./host-bridge", () => ({
  getOptionalHostBridge: () => hostBridgeState.current
}));

describe("normalizeSettings", () => {
  it("matches the shared frontend/helper configuration contract fixture", () => {
    const settings = normalizeSettings(contractFixture.input as Partial<AppSettings>);
    expect({
      schemaVersion: settings.schemaVersion,
      hotzonesEnabled: settings.hotzonesEnabled,
      edgeSize: settings.edgeSize,
      hoverDelayMs: settings.hoverDelayMs,
      pollIntervalMs: settings.pollIntervalMs,
      actionCooldownMs: settings.actionCooldownMs,
      pausedApps: settings.pausedApps,
      stripSize: settings.edgeHide.stripSize,
      edgeHidePreviewEnabled: settings.edgeHide.showPreview,
      showRestoreHint: settings.edgeHide.showRestoreHint,
      keepExpandedWhenForeground: settings.edgeHide.keepExpandedWhenForeground,
      triggerDistance: settings.edgeHide.triggerDistance,
      triggerRatio: settings.edgeHide.triggerRatio,
      collapseDelayMs: settings.edgeHide.collapseDelayMs,
      restoreDelayMs: settings.edgeHide.restoreDelayMs,
      minDistance: settings.mouseGestures.minDistance,
      sensitivity: settings.mouseGestures.sensitivity,
      gesturePausedApps: settings.mouseGestures.pausedApps,
      windowDragEnabled: settings.windowDrag.enabled,
      moveModifiers: settings.windowDrag.moveModifiers,
      resizeModifiers: settings.windowDrag.resizeModifiers,
      topmostPinEnabled: settings.topmostPin.enabled,
      ocrLanguage: settings.ocr.language,
      screenshotResult: settings.ocr.screenshotResult
    }).toEqual(contractFixture.expected);
  });

  it("fills missing sections from defaults", () => {
    const settings = normalizeSettings({
      enabled: false,
      hotzones: [
        {
          id: "top-left",
          enabled: true,
          actions: [{ trigger: "left-click", action: { kind: "shortcut", value: "Win+D" } }]
        }
      ]
    });

    expect(settings.enabled).toBe(false);
    expect(settings.hotzonesEnabled).toBe(true);
    expect(settings.hotzones).toHaveLength(8);
    expect(settings.hotzones[0]).toMatchObject({
      id: "top-left",
      enabled: true,
      actions: expect.arrayContaining([
        expect.objectContaining({ trigger: "left-click", action: { kind: "shortcut", value: "Win+D" } })
      ])
    });
    expect(settings.hotzones[1]).toMatchObject(defaultSettings.hotzones[1]);
    expect(settings.edgeHide.stripSize).toBe(defaultSettings.edgeHide.stripSize);
    expect(settings.edgeHide.showPreview).toBe(true);
    expect(settings.edgeHide.showRestoreHint).toBe(true);
    expect(settings.edgeHide.keepExpandedWhenForeground).toBe(true);
    expect(settings.edgeHide.distanceTriggerEnabled).toBe(true);
    expect(settings.edgeHide.ratioTriggerEnabled).toBe(true);
    expect(settings.edgeHide.triggerRatio).toBe(33);
    expect(settings.edgeHide.collapseDelayMs).toBe(300);
    expect(settings.edgeHide.restoreDelayMs).toBe(200);
    expect(settings.schemaVersion).toBe(7);
    expect(settings.mouseGestures.gestures).toHaveLength(5);
  });

  it("migrates legacy per-zone switches into the trigger-corner global switch", () => {
    const enabled = normalizeSettings({
      schemaVersion: 5,
      hotzones: [{ ...defaultSettings.hotzones[0], enabled: true }]
    });
    const disabled = normalizeSettings({
      schemaVersion: 5,
      hotzones: defaultSettings.hotzones
    });
    const explicit = normalizeSettings({
      schemaVersion: 6,
      hotzonesEnabled: false,
      hotzones: [{ ...defaultSettings.hotzones[0], enabled: true }]
    });

    expect(enabled.hotzonesEnabled).toBe(true);
    expect(disabled.hotzonesEnabled).toBe(false);
    expect(explicit.hotzonesEnabled).toBe(false);
  });

  it("migrates the legacy built-in circle default without replacing custom circle actions", () => {
    const legacy = normalizeSettings({
      schemaVersion: 4,
      mouseGestures: {
        ...defaultSettings.mouseGestures,
        gestures: [{
          ...defaultSettings.mouseGestures.gestures.find((gesture) => gesture.id === "gesture-circle")!,
          action: { kind: "show-desktop" }
        }]
      }
    });
    expect(legacy.mouseGestures.gestures.find((gesture) => gesture.id === "gesture-circle")?.action)
      .toEqual({ kind: "toggle-window-topmost" });

    const customized = normalizeSettings({
      schemaVersion: 4,
      mouseGestures: {
        ...defaultSettings.mouseGestures,
        gestures: [{
          ...defaultSettings.mouseGestures.gestures.find((gesture) => gesture.id === "gesture-circle")!,
          action: { kind: "shortcut", value: "Win+D" }
        }]
      }
    });
    expect(customized.mouseGestures.gestures.find((gesture) => gesture.id === "gesture-circle")?.action)
      .toEqual({ kind: "shortcut", value: "Win+D" });
  });

  it("returns an independent copy of defaults", () => {
    const settings = normalizeSettings(null);
    settings.hotzones[0].enabled = true;
    settings.edgeHide.edges.pop();
    settings.mouseGestures.gestures[0].samples[0][0].x = 0;

    expect(defaultSettings.hotzones[0].enabled).toBe(false);
    expect(defaultSettings.edgeHide.edges).toEqual(["left", "top", "right", "bottom"]);
    expect(defaultSettings.mouseGestures.gestures[0].samples[0][0].x).toBe(0.5);
  });

  it("normalizes gesture controls, samples and preserves built-ins", () => {
    const settings = normalizeSettings({
      mouseGestures: {
        ...defaultSettings.mouseGestures,
        triggerButton: "x2",
        minDistance: 999,
        sensitivity: 10,
        pausedApps: [" game.exe ", "game.exe"],
        gestures: [{
          id: "custom-z",
          name: " Z ",
          enabled: true,
          builtin: false,
          mode: "action",
          action: { kind: "shortcut", value: " Ctrl+Z " },
          samples: [[{ x: -1, y: 0 }, { x: 2, y: 1 }]]
        }]
      }
    });

    expect(settings.mouseGestures).toMatchObject({
      triggerButton: "x2",
      minDistance: 240,
      sensitivity: 35,
      pausedApps: ["game.exe"]
    });
    const custom = settings.mouseGestures.gestures.find((gesture) => gesture.id === "custom-z")!;
    expect(custom).toMatchObject({
      id: "custom-z",
      name: "Z",
      action: { kind: "shortcut", value: "Ctrl+Z" }
    });
    expect(custom.samples[0]).toHaveLength(64);
    expect(custom.samples[0][0]).toEqual({ x: 0, y: 0 });
    expect(custom.samples[0].at(-1)).toEqual({ x: 1, y: 1 });
    expect(settings.mouseGestures.gestures.some((gesture) => gesture.id === "gesture-up")).toBe(true);
  });

  it("keeps all built-ins and limits imported custom gestures to the remaining capacity", () => {
    const customGestures = Array.from({ length: MAX_GESTURE_TEMPLATES }, (_, index) => ({
      id: `custom-${index}`,
      name: `Custom ${index}`,
      enabled: true,
      builtin: false,
      mode: "action" as const,
      action: { kind: "none" as const },
      samples: [[{ x: 0, y: 0 }, { x: 1, y: 1 }]]
    }));
    const settings = normalizeSettings({
      mouseGestures: { ...defaultSettings.mouseGestures, gestures: customGestures }
    });

    expect(settings.mouseGestures.gestures).toHaveLength(MAX_GESTURE_TEMPLATES);
    expect(settings.mouseGestures.gestures.filter((gesture) => gesture.builtin)).toHaveLength(5);
    expect(settings.mouseGestures.gestures.filter((gesture) => !gesture.builtin)).toHaveLength(59);
    expect(settings.mouseGestures.gestures.slice(0, 5).map((gesture) => gesture.id))
      .toEqual(defaultSettings.mouseGestures.gestures.map((gesture) => gesture.id));
  });

  it("restores protected flags and mode for imported built-in ids", () => {
    const rectangle = defaultSettings.mouseGestures.gestures.find((gesture) => gesture.id === "gesture-rectangle")!;
    const settings = normalizeSettings({
      mouseGestures: {
        ...defaultSettings.mouseGestures,
        gestures: [{ ...rectangle, builtin: false, mode: "action" }]
      }
    });

    expect(settings.mouseGestures.gestures.find((gesture) => gesture.id === rectangle.id))
      .toMatchObject({ builtin: true, mode: "region-screenshot" });
  });

  it("clamps imported numeric values and removes invalid list entries", () => {
    const settings = normalizeSettings({
      edgeSize: -20,
      hoverDelayMs: 99_999,
      pollIntervalMs: Number.NaN,
      pausedApps: ["  explorer.exe  ", "", "explorer.exe"],
      edgeHide: {
        ...defaultSettings.edgeHide,
        showPreview: false,
        showRestoreHint: false,
        keepExpandedWhenForeground: false,
        stripSize: 999,
        triggerDistance: 0,
        triggerRatio: 999,
        edges: ["left", "left", "invalid" as never]
      }
    });

    expect(settings.edgeSize).toBe(2);
    expect(settings.hoverDelayMs).toBe(3000);
    expect(settings.pollIntervalMs).toBe(defaultSettings.pollIntervalMs);
    expect(settings.pausedApps).toEqual(["explorer.exe"]);
    expect(settings.edgeHide.stripSize).toBe(64);
    expect(settings.edgeHide.showPreview).toBe(false);
    expect(settings.edgeHide.showRestoreHint).toBe(false);
    expect(settings.edgeHide.keepExpandedWhenForeground).toBe(false);
    expect(settings.edgeHide.triggerDistance).toBe(4);
    expect(settings.edgeHide.triggerRatio).toBe(100);
    expect(settings.edgeHide.edges).toEqual(["left"]);
  });

  it("trims action parameters and removes blank values", () => {
    const settings = normalizeSettings({
      hotzones: [
        {
          ...defaultSettings.hotzones[0],
          actions: [{ trigger: "hover", action: { kind: "shortcut", value: "  Win+D  " } }]
        },
        {
          ...defaultSettings.hotzones[1],
          actions: [{ trigger: "hover", action: { kind: "open-command", value: "   " } }]
        }
      ]
    });

    expect(settings.hotzones[0].actions[0].action.value).toBe("Win+D");
    expect(settings.hotzones[1].actions[0].action.value).toBeUndefined();
  });

  it("keeps native smooth-volume actions", () => {
    const settings = normalizeSettings({
      hotzones: [{
        ...defaultSettings.hotzones[0],
        actions: [{ trigger: "wheel-up", action: { kind: "volume-adjust", value: "0.02" } }]
      }]
    });

    expect(settings.hotzones[0].actions.find((item) => item.trigger === "wheel-up")?.action)
      .toEqual({ kind: "volume-adjust", value: "0.02" });
  });

  it("keeps cooldown and hover delay independent for each trigger slot", () => {
    const settings = normalizeSettings({
      hotzones: [{
        ...defaultSettings.hotzones[0],
        actions: [
          { trigger: "hover", action: { kind: "show-desktop" }, cooldownMs: 900, hoverDelayMs: 480 },
          { trigger: "wheel-up", action: { kind: "volume-adjust", value: "0.02" }, cooldownMs: 24, hoverDelayMs: 0 }
        ]
      }]
    });
    const hover = settings.hotzones[0].actions.find((item) => item.trigger === "hover");
    const wheel = settings.hotzones[0].actions.find((item) => item.trigger === "wheel-up");

    expect(hover).toMatchObject({ cooldownMs: 900, hoverDelayMs: 480 });
    expect(wheel).toMatchObject({ cooldownMs: 24, hoverDelayMs: 0 });
  });

  it("keeps independent per-monitor profiles and all trigger slots", () => {
    const settings = normalizeSettings({
      monitorProfiles: [{
        monitorId: "display:-1920:0:0:1080",
        hotzones: [{
          id: "right",
          enabled: true,
          actions: [
            { trigger: "wheel-up", action: { kind: "shortcut", value: "VolumeUp" } },
            { trigger: "wheel-down", action: { kind: "shortcut", value: "VolumeDown" } }
          ]
        }]
      }]
    });

    expect(settings.monitorProfiles).toHaveLength(1);
    expect(settings.monitorProfiles[0].hotzones).toHaveLength(8);
    expect(settings.monitorProfiles[0].hotzones.find((zone) => zone.id === "right")?.actions)
      .toEqual(expect.arrayContaining([
        expect.objectContaining({ trigger: "wheel-up", action: { kind: "volume-adjust", value: "0.02" } }),
        expect.objectContaining({ trigger: "wheel-down", action: { kind: "volume-adjust", value: "-0.02" } })
      ]));
  });

  it("normalizes modifier variants and the new window/OCR settings", () => {
    const settings = normalizeSettings({
      hotzones: [{
        ...defaultSettings.hotzones[0],
        actions: [{
          trigger: "hover",
          action: { kind: "show-desktop" },
          modifierActions: [
            { modifiers: ["shift", "ctrl", "shift"], action: { kind: "shortcut", value: " Ctrl+C " } },
            { modifiers: ["ctrl", "shift"], action: { kind: "lock-screen" } },
            { modifiers: [], action: { kind: "lock-screen" } }
          ]
        }]
      }],
      windowDrag: {
        ...defaultSettings.windowDrag,
        enabled: true,
        moveModifiers: ["win", "alt", "win"],
        resizeModifiers: ["invalid" as never],
        moveButton: "x1"
      },
      topmostPin: { enabled: false },
      ocr: { language: "zh-Hans", screenshotResult: "pin-and-copy" }
    });

    expect(settings.hotzones[0].actions[0].modifierActions).toEqual([
      { modifiers: ["ctrl", "shift"], action: { kind: "shortcut", value: "Ctrl+C" } }
    ]);
    expect(settings.windowDrag).toMatchObject({
      enabled: true,
      moveModifiers: ["alt", "win"],
      resizeModifiers: [],
      moveButton: "x1"
    });
    expect(settings.topmostPin.enabled).toBe(false);
    expect(settings.ocr).toEqual({ language: "zh-Hans", screenshotResult: "pin-and-copy" });
  });

  it("keeps edge-hide directions independent for each monitor", () => {
    const settings = normalizeSettings({
      edgeHide: {
        ...defaultSettings.edgeHide,
        monitorProfiles: [
          { monitorId: "display:-1920:0:0:1080", edges: ["left"] },
          { monitorId: "display:0:0:1920:1080", edges: ["right", "bottom"] }
        ]
      }
    });

    expect(settings.edgeHide.monitorProfiles).toEqual([
      { monitorId: "display:-1920:0:0:1080", edges: ["left"] },
      { monitorId: "display:0:0:1920:1080", edges: ["right", "bottom"] }
    ]);
    settings.edgeHide.monitorProfiles[0].edges.push("top");
    expect(settings.edgeHide.monitorProfiles[1].edges).toEqual(["right", "bottom"]);
  });
});

describe("saveSettings", () => {
  afterEach(() => {
    hostBridgeState.current = null;
    vi.unstubAllGlobals();
  });

  it("rounds gesture coordinates before writing to desktop storage", () => {
    let stored: AppSettings | undefined;
    vi.stubGlobal("localStorage", {
      getItem: () => null,
      setItem: (_key: string, value: string) => { stored = JSON.parse(value) as AppSettings; }
    });
    const settings = normalizeSettings(defaultSettings);
    settings.mouseGestures.gestures[0].samples[0][0] = { x: 0.12345678, y: 0.87654321 };

    saveSettings(settings);

    expect(stored?.mouseGestures.gestures[0].samples[0][0]).toEqual({ x: 0.1235, y: 0.8765 });
    expect(settings.mouseGestures.gestures[0].samples[0][0]).toEqual({ x: 0.12345678, y: 0.87654321 });
  });

  it("surfaces desktop storage failures", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => null,
      setItem: () => { throw new Error("quota exceeded"); }
    });

    expect(() => saveSettings(normalizeSettings(defaultSettings))).toThrow("配置保存失败：quota exceeded");
  });

  it("rejects settings that exceed the storage safety budget", () => {
    const settings = normalizeSettings(defaultSettings);
    settings.pausedApps = ["x".repeat(MAX_SETTINGS_STORAGE_BYTES)];

    expect(() => saveSettings(settings)).toThrow("超过安全上限");
  });

  it("surfaces desktop host persistence failures and does not create a competing local copy", async () => {
    const setItem = vi.fn();
    vi.stubGlobal("localStorage", { getItem: () => null, setItem });
    hostBridgeState.current = {
      saveSettings: vi.fn().mockRejectedValue(new Error("disk full"))
    };

    await expect(saveSettings(normalizeSettings(defaultSettings))).rejects.toThrow("disk full");
    expect(setItem).not.toHaveBeenCalled();
  });
});
