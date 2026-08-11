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
