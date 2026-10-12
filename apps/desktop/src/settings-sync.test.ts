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

it("isolates normalized geometry from the active editor while preserving per-monitor dimensions", () => {
  const settings = structuredClone(defaultSettings);
  settings.hotzones[0].geometry = { kind: "corner", width: 24, height: 48, linked: true };
  settings.hotzones[1].geometry = { kind: "edge", thickness: 16, lengthPercent: 80 };
  settings.monitorProfiles = [{ monitorId: "monitor:sync", hotzones: structuredClone(settings.hotzones) }];
  const before = structuredClone(settings);
  const prepared = prepareSettingsUpdate(settings);
  expect(prepared.normalized.hotzones).toEqual(before.hotzones);
  expect(prepared.normalized.monitorProfiles).toEqual(before.monitorProfiles);
  expect(prepared.editable).toEqual(before);
  expect(prepared.normalized.hotzones[0].geometry).not.toBe(settings.hotzones[0].geometry);
  expect(prepared.normalized.monitorProfiles[0].hotzones[1].geometry).not.toBe(settings.monitorProfiles[0].hotzones[1].geometry);
  const corner = prepared.normalized.hotzones[0].geometry!;
  if (corner.kind === "corner") corner.width = 64;
  expect(settings).toEqual(before);
  expect(prepared.editable).toEqual(before);
  expect(prepared.normalized.monitorProfiles[0].hotzones[0].geometry).toEqual(before.hotzones[0].geometry);
});
