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
