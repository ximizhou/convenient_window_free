import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings, normalizeSettings } from "./settings-store";
import * as gestureAlgorithm from "./gesture-algorithm";
import { prepareSettingsUpdate } from "./settings-sync";

afterEach(() => vi.restoreAllMocks());

it("reuses unchanged samples across edits while invalidating in-place and replacement changes", () => {
  const resample = vi.spyOn(gestureAlgorithm, "resampleGesture");
  let draft = structuredClone(defaultSettings);
  draft.mouseGestures.gestures[0].samples = [[{ x: 0, y: 0 }, { x: 0.8, y: 0.5 }, { x: 1, y: 1 }]];
  const first = prepareSettingsUpdate(draft);
  draft = first.editable;
  resample.mockClear();
  for (let index = 0; index < 20; index++) {
    draft.mouseGestures.sensitivity = 35 + index;
    const next = prepareSettingsUpdate(draft);
    expect(next.normalized.mouseGestures.sensitivity).toBe(35 + index);
    expect(next.normalized.mouseGestures.gestures[0].samples).toEqual(first.normalized.mouseGestures.gestures[0].samples);
    draft = next.editable;
  }
  expect(resample.mock.calls.length).toBe(0);

  const sample = draft.mouseGestures.gestures[0].samples[0];
  sample[1].x = 0.1;
  const changed = prepareSettingsUpdate(draft);
  expect(resample).toHaveBeenCalledTimes(1);
  expect(changed.normalized).toEqual(normalizeSettings(structuredClone(draft)));
  expect(changed.normalized.mouseGestures.gestures[0].samples).not.toEqual(first.normalized.mouseGestures.gestures[0].samples);

  resample.mockClear();
  sample.push({ x: 0.2, y: 0.4 });
  prepareSettingsUpdate(draft);
  expect(resample).toHaveBeenCalledTimes(1);
  resample.mockClear();
  draft.mouseGestures.gestures[0].samples = [[{ x: 1, y: 0 }, { x: 0, y: 1 }]];
  const replaced = prepareSettingsUpdate(draft);
  expect(resample).toHaveBeenCalledTimes(1);
  expect(replaced.normalized).toEqual(normalizeSettings(structuredClone(draft)));
});

it("isolates cached samples from callers and revalidates mutated imported coordinates", () => {
  const draft = structuredClone(defaultSettings);
  const sample = draft.mouseGestures.gestures[0].samples[0];
  const expected = prepareSettingsUpdate(draft).normalized;
  const returned = prepareSettingsUpdate(draft).normalized;
  returned.mouseGestures.gestures[0].samples[0][0].x = 99;
  expect(prepareSettingsUpdate(draft).normalized).toEqual(expected);
  for (const x of [NaN, Infinity, -2, 2]) {
    sample[0].x = x;
    expect(prepareSettingsUpdate(draft).normalized).toEqual(normalizeSettings(structuredClone(draft)));
  }
});

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
