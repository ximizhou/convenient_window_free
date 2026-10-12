import { afterEach, describe, expect, it, vi } from "vitest";
import {
  defaultSettings,
  loadSettings,
  MAX_GESTURE_TEMPLATES,
  MAX_SETTINGS_STORAGE_BYTES,
  normalizeSettings,
  saveSettings
} from "./settings-store";
import contractFixture from "../../../tests/fixtures/config-contract.json";
import type { AppSettings } from "./types";

const hostBridgeState = vi.hoisted(() => ({ current: null as null | { getInitialSettings?(): unknown; saveSettings(settings: unknown): Promise<void> } }));

vi.mock("./host-bridge", () => ({
  getOptionalHostBridge: () => hostBridgeState.current
}));

describe("normalizeSettings", () => {
  it("defaults the hotzone hint on for missing settings while migrating legacy settings to schema v9", () => {
    expect(defaultSettings.showHotzoneHint).toBe(true);
    expect(defaultSettings.schemaVersion).toBe(9);
    for (const input of [null, undefined, {}, { schemaVersion: 7 }, { schemaVersion: 8 }]) {
      expect(normalizeSettings(input)).toMatchObject({ schemaVersion: 9, showHotzoneHint: true });
    }
  });

  it.each([null, 0, 1, "false", "true", {}, []].map((value) => [value]))("defaults a non-boolean hotzone hint %j on", (showHotzoneHint) => {
    const settings = normalizeSettings({ showHotzoneHint } as unknown as Partial<AppSettings>);
    expect(settings.showHotzoneHint).toBe(true);
  });

  it.each([false, true])("preserves hotzone hint %s through normalization and JSON export/import", (showHotzoneHint) => {
    const settings = normalizeSettings({ showHotzoneHint, enabled: false, hotzonesEnabled: false });
    expect(settings).toMatchObject({ schemaVersion: 9, showHotzoneHint, enabled: false, hotzonesEnabled: false });
    expect(normalizeSettings(settings).showHotzoneHint).toBe(showHotzoneHint);
    const exported = JSON.stringify(normalizeSettings(settings), null, 2);
    expect(JSON.parse(exported).showHotzoneHint).toBe(showHotzoneHint);
    expect(normalizeSettings(JSON.parse(exported))).toMatchObject({ schemaVersion: 9, showHotzoneHint, enabled: false, hotzonesEnabled: false });
  });

  it("keeps taskbar transparency off for legacy and malformed configuration", () => {
    expect(defaultSettings.taskbarAppearance.enabled).toBe(false);
    expect(normalizeSettings({}).taskbarAppearance.enabled).toBe(false);
    expect(normalizeSettings({ taskbarAppearance: { enabled: "true" } } as unknown as Partial<AppSettings>).taskbarAppearance.enabled).toBe(false);
  });
  it("round-trips explicit taskbar opt-in and opt-out", () => {
    for (const enabled of [true, false]) {
      const normalized = normalizeSettings({ taskbarAppearance: { enabled } } as unknown as Partial<AppSettings>);
      expect(normalizeSettings(JSON.parse(JSON.stringify(normalized))).taskbarAppearance.enabled).toBe(enabled);
    }
  });
  it("preserves the original transparent appearance when upgrading a boolean-only prototype", () => {
    const settings = normalizeSettings({ taskbarAppearance: { enabled: true } } as unknown as Partial<AppSettings>);
    expect(settings.taskbarAppearance).toEqual({ enabled: true, mode: "transparent", opacity: 58, tint: "#233A63", showBorder: false });
  });
  it("normalizes taskbar material controls", () => {
    const normalized = normalizeSettings({
      taskbarAppearance: { enabled: true, mode: "unknown", opacity: 180, tint: "not-a-color", showBorder: "yes" }
    } as unknown as Partial<AppSettings>);
    expect(normalized.taskbarAppearance).toEqual({ enabled: true, mode: "transparent", opacity: 100, tint: "#233A63", showBorder: false });
    expect(normalizeSettings({ taskbarAppearance: { ...defaultSettings.taskbarAppearance, opacity: NaN } }).taskbarAppearance.opacity).toBe(58);
  });
  it("round-trips all taskbar materials, color, strength and border settings", () => {
    for (const mode of ["transparent", "acrylic", "solid"] as const) {
      const settings = normalizeSettings({ taskbarAppearance: { enabled: true, mode, opacity: 73, tint: " #eaf1fc ", showBorder: true } });
      expect(settings.taskbarAppearance).toEqual({ enabled: true, mode, opacity: 73, tint: "#EAF1FC", showBorder: true });
      expect(normalizeSettings(JSON.parse(JSON.stringify(settings))).taskbarAppearance).toEqual(settings.taskbarAppearance);
    }
  });
  it("matches the shared frontend/helper configuration contract fixture", () => {
    const settings = normalizeSettings(contractFixture.input as Partial<AppSettings>);
    expect({
      schemaVersion: settings.schemaVersion,
      hotzonesEnabled: settings.hotzonesEnabled,
      showHotzoneHint: settings.showHotzoneHint,
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
      windowDragPausedApps: settings.windowDrag.pausedApps,
      moveModifiers: settings.windowDrag.moveModifiers,
      resizeModifiers: settings.windowDrag.resizeModifiers,
      topmostPinEnabled: settings.topmostPin.enabled,
      ocrLanguage: settings.ocr.language,
      screenshotResult: settings.ocr.screenshotResult,
      pinOffset: settings.ocr.pinOffset,
      cornerGeometry: settings.hotzones.find(zone => zone.id === "top-left")?.geometry,
      edgeGeometry: settings.hotzones.find(zone => zone.id === "bottom")?.geometry
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
    expect(settings.edgeHide.keepExpandedWhenForeground).toBe(false);
    expect(settings.edgeHide.distanceTriggerEnabled).toBe(true);
    expect(settings.edgeHide.ratioTriggerEnabled).toBe(true);
    expect(settings.edgeHide.triggerRatio).toBe(33);
    expect(settings.edgeHide.collapseDelayMs).toBe(300);
    expect(settings.edgeHide.restoreDelayMs).toBe(200);
    expect(settings.schemaVersion).toBe(9);
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
      ocr: { language: "zh-Hans", screenshotResult: "pin-and-copy", pinOffset: false }
    });

    expect(settings.hotzones[0].actions[0].modifierActions).toEqual([
      { modifiers: ["ctrl", "shift"], action: { kind: "shortcut", value: "Ctrl+C" } }
    ]);
    expect(settings.windowDrag).toMatchObject({
      enabled: true,
      moveModifiers: ["alt", "win"],
      resizeModifiers: ["alt"],
      moveButton: "x1"
    });
    expect(settings.topmostPin.enabled).toBe(false);
    expect(settings.ocr).toEqual({ language: "zh-Hans", screenshotResult: "pin-and-copy", pinOffset: false });
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
  it("preserves either hotzone hint choice through desktop host storage and readback", async () => {
    let stored: AppSettings | null = null;
    const setItem = vi.fn();
    vi.stubGlobal("localStorage", { setItem });
    hostBridgeState.current = {
      getInitialSettings: () => stored,
      saveSettings: async (settings) => { stored = structuredClone(settings) as AppSettings; }
    };
    for (const showHotzoneHint of [false, true]) {
      const settings = normalizeSettings({ showHotzoneHint, enabled: false, hotzonesEnabled: false });
      await saveSettings(settings);
      expect(stored).toMatchObject({ schemaVersion: 9, showHotzoneHint });
      expect(loadSettings()).toMatchObject({ schemaVersion: 9, showHotzoneHint, enabled: false, hotzonesEnabled: false });
    }
    expect(setItem).not.toHaveBeenCalled();
  });

  it("preserves either hotzone hint choice through local fallback storage and readback", async () => {
    let stored: string | null = null;
    vi.stubGlobal("localStorage", {
      getItem: () => stored,
      setItem: (_key: string, value: string) => { stored = value; }
    });
    for (const showHotzoneHint of [false, true]) {
      const settings = normalizeSettings({ showHotzoneHint, enabled: false, hotzonesEnabled: false });
      await saveSettings(settings);
      expect(JSON.parse(stored!)).toMatchObject({ schemaVersion: 9, showHotzoneHint });
      expect(loadSettings()).toMatchObject({ schemaVersion: 9, showHotzoneHint, enabled: false, hotzonesEnabled: false });
    }
  });

  afterEach(() => {
    hostBridgeState.current = null;
    vi.unstubAllGlobals();
  });

  it("defaults pin offset on and preserves either choice after saving and reloading", async () => {
    expect(normalizeSettings({}).ocr.pinOffset).toBe(true);
    let stored = "";
    vi.stubGlobal("localStorage", {
      setItem: (_key: string, value: string) => { stored = value; }
    });
    for (const pinOffset of [false, true]) {
      const settings = normalizeSettings({ ocr: { ...defaultSettings.ocr, pinOffset } });
      await saveSettings(settings);
      expect(normalizeSettings(JSON.parse(stored)).ocr.pinOffset).toBe(pinOffset);
    }
  });

  it("deep clones stored global and per-monitor geometry without changing the editing draft", async () => {
    let stored: AppSettings | undefined;
    hostBridgeState.current = { saveSettings: async (value) => { stored = value as AppSettings; } };
    const settings = normalizeSettings(defaultSettings);
    settings.hotzones[0].geometry = { kind: "corner", width: 24, height: 48, linked: true };
    settings.hotzones[1].geometry = { kind: "edge", thickness: 16, lengthPercent: 80 };
    settings.monitorProfiles = [{ monitorId: "monitor:clone", hotzones: structuredClone(settings.hotzones) }];
    const before = structuredClone(settings);
    await saveSettings(settings);
    expect(stored).toMatchObject({ schemaVersion: 9, hotzones: before.hotzones, monitorProfiles: before.monitorProfiles });
    for (const [original, copied] of [
      [settings.hotzones, stored!.hotzones],
      [settings.monitorProfiles[0].hotzones, stored!.monitorProfiles[0].hotzones]
    ]) {
      expect(copied[0].geometry).not.toBe(original[0].geometry);
      expect(copied[1].geometry).not.toBe(original[1].geometry);
      const corner = copied[0].geometry!;
      if (corner.kind === "corner") corner.width = 128;
      const edge = copied[1].geometry!;
      if (edge.kind === "edge") edge.lengthPercent = 10;
    }
    expect(settings).toEqual(before);
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



describe("edge hide animation preference", () => {
  it("defaults on and preserves explicit off across normalization and old imports", () => {
    expect(defaultSettings.edgeHide.animationEnabled).toBe(true);
    expect(normalizeSettings({ schemaVersion: 7 }).edgeHide.animationEnabled).toBe(true);
    const settings = structuredClone(defaultSettings);
    settings.edgeHide.animationEnabled = false;
    const imported = normalizeSettings(JSON.parse(JSON.stringify(settings)));
    expect(imported.edgeHide.animationEnabled).toBe(false);
    expect(normalizeSettings(imported).edgeHide).toEqual(imported.edgeHide);
    (settings.edgeHide as unknown as { animationEnabled: unknown }).animationEnabled = "off";
    expect(normalizeSettings(settings).edgeHide.animationEnabled).toBe(true);
  });
});


it("keeps Alt required for missing and empty drag bindings, matching the helper contract", () => {
  expect(normalizeSettings({}).windowDrag.moveModifiers).toEqual(["alt"]);
  const settings = structuredClone(defaultSettings);
  settings.windowDrag.moveModifiers = [];
  settings.windowDrag.resizeModifiers = [];
  const normalized = normalizeSettings(settings);
  expect(normalized.windowDrag.moveModifiers).toEqual(["alt"]);
  expect(normalized.windowDrag.resizeModifiers).toEqual(["alt"]);
});

describe("schema 9 hotzone geometry", () => {
  it.each(Array.from({ length: 9 }, (_, schemaVersion) => schemaVersion))(
    "migrates schema %s without materializing geometry or changing legacy sizes", (schemaVersion) => {
      const settings = normalizeSettings({ schemaVersion, edgeSize: 24,
        hotzones: defaultSettings.hotzones,
        monitorProfiles: [{ monitorId: "monitor:legacy", hotzones: defaultSettings.hotzones }]
      });
      expect(settings.schemaVersion).toBe(9);
      expect(settings.edgeSize).toBe(24);
      for (const zone of [...settings.hotzones, ...settings.monitorProfiles[0].hotzones]) {
        expect(Object.hasOwn(zone, "geometry")).toBe(false);
      }
      expect(normalizeSettings({ schemaVersion, edgeSize: 999 }).edgeSize).toBe(48);
      expect(normalizeSettings({ schemaVersion, edgeSize: -1 }).edgeSize).toBe(2);
      expect(normalizeSettings(JSON.parse(JSON.stringify(settings)))).toMatchObject({
        schemaVersion: 9, edgeSize: settings.edgeSize, hotzones: settings.hotzones, monitorProfiles: settings.monitorProfiles
      });
    }
  );

  it("retains disabled/non-action geometry and deep copies both global and per-monitor values", () => {
    const source = structuredClone(defaultSettings);
    source.hotzones[0].geometry = { kind: "corner", width: 24, height: 48, linked: true };
    source.hotzones[1].geometry = { kind: "edge", thickness: 16, lengthPercent: 100 };
    source.monitorProfiles = [
      { monitorId: "monitor:a", hotzones: source.hotzones },
      { monitorId: "monitor:b", hotzones: source.hotzones }
    ];
    const before = structuredClone(source);
    const settings = normalizeSettings(source);
    expect(settings.hotzones).toEqual(before.hotzones);
    expect(settings.monitorProfiles).toEqual(before.monitorProfiles);
    expect(settings.hotzones[0].geometry).not.toBe(source.hotzones[0].geometry);
    expect(settings.hotzones[1].geometry).not.toBe(source.hotzones[1].geometry);
    for (const profile of settings.monitorProfiles) {
      expect(profile.hotzones[0].geometry).not.toBe(source.hotzones[0].geometry);
      expect(profile.hotzones[0].geometry).not.toBe(settings.hotzones[0].geometry);
      expect(profile.hotzones[1].geometry).not.toBe(settings.hotzones[1].geometry);
      expect(profile.hotzones[0].actions).not.toBe(source.hotzones[0].actions);
    }
    const corner = settings.hotzones[0].geometry!;
    if (corner.kind === "corner") corner.width = 64;
    expect(settings.monitorProfiles[0].hotzones[0].geometry).toEqual(before.hotzones[0].geometry);
    expect(source).toEqual(before);
    const exported = JSON.stringify(settings);
    expect(normalizeSettings(JSON.parse(exported))).toMatchObject({
      schemaVersion: 9, hotzones: settings.hotzones, monitorProfiles: settings.monitorProfiles
    });
    expect(settings.hotzones[0].actions).toEqual(source.hotzones[0].actions);
    expect(settings.hotzones[0].enabled).toBe(false);
    expect(settings.hotzones[0].actions.every(slot => slot.action.kind === "none")).toBe(true);
    expect(Object.hasOwn(settings.hotzones[2], "geometry")).toBe(false);
  });

  it("uses normalized edgeSize for partial geometry on all monitors and never forces linked squares", () => {
    const source = {
      schemaVersion: 8, edgeSize: 999,
      hotzones: [
        { id: "top-left", geometry: { kind: "corner", width: 24, height: 96, linked: true } },
        { id: "top-right", geometry: { kind: "corner" } },
        { id: "bottom", geometry: { kind: "edge" } }
      ],
      monitorProfiles: [{ monitorId: "monitor:partial", hotzones: [
        { id: "bottom-left", geometry: { kind: "corner", width: 21.6, height: 200, linked: false } },
        { id: "right", geometry: { kind: "edge", lengthPercent: 10.6 } }
      ] }]
    } as unknown as Partial<AppSettings>;
    const settings = normalizeSettings(source);
    expect(settings.hotzones[0].geometry).toEqual({ kind: "corner", width: 24, height: 96, linked: true });
    expect(settings.hotzones.find(zone => zone.id === "top-right")?.geometry).toEqual({ kind: "corner", width: 48, height: 48, linked: true });
    expect(settings.hotzones.find(zone => zone.id === "bottom")?.geometry).toEqual({ kind: "edge", thickness: 48, lengthPercent: 40 });
    expect(settings.monitorProfiles[0].hotzones.find(zone => zone.id === "bottom-left")?.geometry).toEqual({ kind: "corner", width: 22, height: 128, linked: false });
    expect(settings.monitorProfiles[0].hotzones.find(zone => zone.id === "right")?.geometry).toEqual({ kind: "edge", thickness: 48, lengthPercent: 11 });
  });

  it("omits invalid and mismatched geometry without dropping actions", () => {
    for (const geometry of [null, 0, "corner", [], {}, { kind: "edge", thickness: 24, lengthPercent: 100 }]) {
      const hotzones = [{ ...defaultSettings.hotzones[0], enabled: true, geometry,
        actions: [{ trigger: "left-click", action: { kind: "shortcut", value: "Ctrl+K" } }]
      }];
      const settings = normalizeSettings({ hotzones,
        monitorProfiles: [{ monitorId: "monitor:invalid", hotzones }]
      } as unknown as Partial<AppSettings>);
      for (const zone of [settings.hotzones[0], settings.monitorProfiles[0].hotzones[0]]) {
        expect(Object.hasOwn(zone, "geometry")).toBe(false);
        expect(zone.enabled).toBe(true);
        expect(zone.actions.find(slot => slot.trigger === "left-click")?.action).toEqual({ kind: "shortcut", value: "Ctrl+K" });
      }
    }
  });

  it("rejects future schemas without rewriting the geometry-bearing input", () => {
    const future = structuredClone(defaultSettings);
    future.schemaVersion = 10;
    future.hotzones[0].geometry = { kind: "corner", width: 24, height: 48, linked: true };
    const before = structuredClone(future);
    expect(() => normalizeSettings(future)).toThrow("Unsupported settings schema");
    expect(future).toEqual(before);
  });
});


describe("0.6.5 window motion settings", () => {
  it("preserves the old animation speed and supplies safe optional defaults", () => {
    const settings = normalizeSettings({ schemaVersion: 9 });
    expect(settings.edgeHide.animationDurationMs).toBe(160);
    expect(settings.edgeHide.topTriggerDistance).toBe(48);
    expect(settings.edgeHide.topHoverDelayMs).toBe(600);
    expect(settings.windowDrag.updateHz).toBe(120);
  });
  it("normalizes custom values and preserves them through import/export", () => {
    const settings = normalizeSettings({ ...defaultSettings,
      edgeHide: { ...defaultSettings.edgeHide, animationDurationMs: 400, topTriggerDistance: 80, topHoverDelayMs: 1000 },
      windowDrag: { ...defaultSettings.windowDrag, updateHz: 60 } });
    const imported = normalizeSettings(JSON.parse(JSON.stringify(settings)));
    expect(imported.edgeHide.animationDurationMs).toBe(400);
    expect(imported.edgeHide.topTriggerDistance).toBe(80);
    expect(imported.edgeHide.topHoverDelayMs).toBe(1000);
    expect(imported.edgeHide.triggerDistance).toBe(defaultSettings.edgeHide.triggerDistance);
    expect(imported.windowDrag.updateHz).toBe(60);
  });
  it("bounds extreme values and rejects nonnumeric inputs", () => {
    const settings = normalizeSettings({ edgeHide: { ...defaultSettings.edgeHide, animationDurationMs: 9999, topTriggerDistance: 0, topHoverDelayMs: 1 }, windowDrag: { ...defaultSettings.windowDrag, updateHz: 9999 } });
    expect(settings.edgeHide.animationDurationMs).toBe(800);
    expect(settings.edgeHide.topTriggerDistance).toBe(8);
    expect(settings.edgeHide.topHoverDelayMs).toBe(200);
    expect(settings.windowDrag.updateHz).toBe(240);
    const malformed = JSON.parse('{"edgeHide":{"animationDurationMs":"slow"},"windowDrag":{"updateHz":"fast"}}');
    const invalid = normalizeSettings(malformed);
    expect(invalid.edgeHide.animationDurationMs).toBe(160);
    expect(invalid.windowDrag.updateHz).toBe(120);
  });
});
