import { describe, expect, it } from "vitest";
import { normalizeSettings, defaultSettings } from "./settings-store";
import { settingsForHelper } from "./runtime-settings";

function withUnknownAction() {
  const input = structuredClone(defaultSettings) as unknown as Record<string, any>;
  input.hotzones[0].actions[0].action = { kind: "utools-redirect", value: "legacy-code" };
  input.mouseGestures.gestures[0].action = { kind: "future-host-action", value: "opaque" };
  return input;
}

describe("desktop runtime settings", () => {
  it("preserves brightness actions through storage and helper sanitization", () => {
    const input = structuredClone(defaultSettings);
    const up = { kind: "brightness-adjust", value: "0.05" };
    const down = { kind: "brightness-adjust", value: "-0.05" };
    input.hotzones[0].actions = [
      { trigger: "wheel-up", action: up, modifierActions: [{ modifiers: ["ctrl"], action: down }] },
      { trigger: "wheel-down", action: down, cooldownMs: 80 }
    ];
    input.monitorProfiles = [{ monitorId: "monitor:test", hotzones: structuredClone(input.hotzones) }];
    input.mouseGestures.gestures[0].action = up;
    input.mouseGestures.gestures[0].modifierActions = [{ modifiers: ["shift"], action: down }];

    const stored = normalizeSettings(JSON.parse(JSON.stringify(input)));
    const runtime = settingsForHelper(stored);
    for (const zone of [runtime.hotzones[0], runtime.monitorProfiles[0].hotzones[0]]) {
      const wheelUp = zone.actions.find((slot) => slot.trigger === "wheel-up")!;
      const wheelDown = zone.actions.find((slot) => slot.trigger === "wheel-down")!;
      expect(wheelUp.action).toEqual(up);
      expect(wheelUp.cooldownMs).toBe(32);
      expect(wheelUp.modifierActions?.[0].action).toEqual(down);
      expect(wheelDown.action).toEqual(down);
      expect(wheelDown.cooldownMs).toBe(80);
    }
    expect(runtime.mouseGestures.gestures[0].action).toEqual(up);
    expect(runtime.mouseGestures.gestures[0].modifierActions?.[0].action).toEqual(down);
  });

  it("preserves unknown actions in stored schema v7 settings", () => {
    const stored = normalizeSettings(withUnknownAction());

    expect(stored.hotzones[0].actions[0].action).toEqual({ kind: "utools-redirect", value: "legacy-code" });
    expect(stored.mouseGestures.gestures[0].action).toEqual({ kind: "future-host-action", value: "opaque" });
  });

  it("hides unsupported host actions from the shared helper without mutating storage", () => {
    const stored = normalizeSettings(withUnknownAction());
    const runtime = settingsForHelper(stored);

    expect(runtime.hotzones[0].actions[0].action).toEqual({ kind: "none" });
    expect(runtime.mouseGestures.gestures[0].action).toEqual({ kind: "none" });
    expect(stored.hotzones[0].actions[0].action.kind).toBe("utools-redirect");
  });
});
