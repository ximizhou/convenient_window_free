import { describe, expect, it } from "vitest";
import { defaultSettings } from "./settings-store";
import { prepareSettingsUpdate } from "./settings-sync";

describe("prepareSettingsUpdate", () => {
  it("normalizes the persisted snapshot without rewriting in-progress input", () => {
    const settings = structuredClone(defaultSettings);
    settings.hotzones[0].actions[0].action = { kind: "open-command", value: "cmd " };
    settings.mouseGestures.gestures[0].name = "向上 ";
    settings.edgeHide.stripSize = 1;
    settings.pollIntervalMs = 3;

    const prepared = prepareSettingsUpdate(settings);

    expect(prepared.editable).not.toBe(settings);
    expect(prepared.editable.hotzones[0].actions[0].action.value).toBe("cmd ");
    expect(prepared.editable.mouseGestures.gestures[0].name).toBe("向上 ");
    expect(prepared.editable.edgeHide.stripSize).toBe(1);
    expect(prepared.editable.pollIntervalMs).toBe(3);
    expect(prepared.normalized.hotzones[0].actions[0].action.value).toBe("cmd");
    expect(prepared.normalized.mouseGestures.gestures[0].name).toBe("向上");
    expect(prepared.normalized.edgeHide.stripSize).toBe(4);
    expect(prepared.normalized.pollIntervalMs).toBe(10);
  });
});
