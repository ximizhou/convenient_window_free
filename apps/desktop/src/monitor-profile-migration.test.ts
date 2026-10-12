import { describe, expect, it } from "vitest";
import { defaultSettings, normalizeSettings } from "./settings-store";
import { migrateMonitorProfileIds } from "./monitor-profile-migration";

describe("monitor profile migration", () => {
  it("moves legacy geometry profiles to a stable device id", () => {
    const settings = normalizeSettings({
      monitorProfiles: [{
        monitorId: "display:0:0:1920:1080",
        hotzones: defaultSettings.hotzones
      }],
      edgeHide: {
        ...defaultSettings.edgeHide,
        monitorProfiles: [{ monitorId: "display:0:0:1920:1080", edges: ["left"] }]
      }
    });

    expect(migrateMonitorProfileIds(settings, [{
      id: "monitor:device-a",
      legacyId: "display:0:0:1920:1080",
      primary: true,
      bounds: { left: 0, top: 0, right: 1920, bottom: 1080 },
      workArea: { left: 0, top: 0, right: 1920, bottom: 1040 }
    }])).toBe(true);
    expect(settings.monitorProfiles[0].monitorId).toBe("monitor:device-a");
    expect(settings.edgeHide.monitorProfiles[0]).toEqual({ monitorId: "monitor:device-a", edges: ["left"] });
  });

  it("keeps an existing stable profile and removes its stale legacy duplicate", () => {
    const settings = normalizeSettings({
      monitorProfiles: [
        { monitorId: "monitor:device-a", hotzones: defaultSettings.hotzones },
        { monitorId: "display:0:0:1920:1080", hotzones: defaultSettings.hotzones }
      ]
    });
    const display = {
      id: "monitor:device-a", legacyId: "display:0:0:1920:1080", primary: true,
      bounds: { left: 0, top: 0, right: 1920, bottom: 1080 },
      workArea: { left: 0, top: 0, right: 1920, bottom: 1040 }
    };

    expect(migrateMonitorProfileIds(settings, [display])).toBe(true);
    expect(settings.monitorProfiles.map((profile) => profile.monitorId)).toEqual(["monitor:device-a"]);
  });
});

it("retains configured geometry and actions when a monitor profile gets its stable id", () => {
  const settings = normalizeSettings(defaultSettings);
  const zones = structuredClone(settings.hotzones);
  zones[0].geometry = { kind: "corner", width: 24, height: 48, linked: true };
  zones[1].geometry = { kind: "edge", thickness: 16, lengthPercent: 100 };
  zones[0].actions[0].action = { kind: "shortcut", value: "Ctrl+K" };
  settings.monitorProfiles = [{ monitorId: "display:-1920:-1080:0:0", hotzones: zones }];
  const before = structuredClone(zones);
  expect(migrateMonitorProfileIds(settings, [{
    id: "monitor:geometry", legacyId: "display:-1920:-1080:0:0", primary: false,
    bounds: { left: -1920, top: -1080, right: 0, bottom: 0 },
    workArea: { left: -1920, top: -1080, right: 0, bottom: 0 }
  }])).toBe(true);
  expect(settings.monitorProfiles[0]).toEqual({ monitorId: "monitor:geometry", hotzones: before });
  expect(normalizeSettings(JSON.parse(JSON.stringify(settings))).monitorProfiles).toEqual(settings.monitorProfiles);
});
