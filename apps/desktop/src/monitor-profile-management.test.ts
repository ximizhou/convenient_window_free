import { describe, expect, it } from "vitest";
import { defaultSettings } from "./settings-store";
import { canManageMonitorProfiles, listOfflineMonitorProfiles, removeOfflineMonitorProfile } from "./monitor-profile-management";
import type { AppSettings, DisplayInfo } from "./types";

const online = { id: "online", legacyId: "old-online", primary: true, bounds: { left: 0, top: 0, right: 1920, bottom: 1080 }, workArea: { left: 0, top: 0, right: 1920, bottom: 1040 } } satisfies DisplayInfo;
function settings(): AppSettings {
  const result = structuredClone(defaultSettings);
  result.monitorProfiles = ["online", "old-online", "offline", "hotzones-only"].map((monitorId) => ({ monitorId, hotzones: structuredClone(result.hotzones) }));
  result.edgeHide.monitorProfiles = ["online", "old-online", "offline", "edges-only"].map((monitorId) => ({ monitorId, edges: ["left", "right"] }));
  return result;
}

describe("offline display profile management", () => {
  it("joins both profile lists and protects online stable and legacy identities", () => {
    const profiles = listOfflineMonitorProfiles(settings(), [online], true);
    expect(profiles.map((profile) => profile.monitorId)).toEqual(["offline", "hotzones-only", "edges-only"]);
    expect(profiles[0]).toMatchObject({ hotzoneCount: 8, edgeCount: 2, hasHotzones: true, hasEdgeHide: true });
    expect(profiles[1]).toMatchObject({ hasHotzones: true, hasEdgeHide: false });
    expect(profiles[2]).toMatchObject({ hasHotzones: false, hasEdgeHide: true });
  });

  it.each([[false, [online]], [true, []], [true, [{ ...online, id: "" }]] ] as const)("requires real, known topology before identifying anything as offline", (ready, displays) => {
    expect(canManageMonitorProfiles(displays, ready)).toBe(false);
    expect(listOfflineMonitorProfiles(settings(), displays, ready)).toEqual([]);
    expect(removeOfflineMonitorProfile(settings(), displays, ready, "offline")).toBeNull();
  });

  it("removes exactly the selected screen from both lists without mutating the original or unrelated settings", () => {
    const original = settings();
    const before = structuredClone(original);
    const next = removeOfflineMonitorProfile(original, [online], true, "offline")!;
    expect(original).toEqual(before);
    expect(next.monitorProfiles.map((item) => item.monitorId)).toEqual(["online", "old-online", "hotzones-only"]);
    expect(next.edgeHide.monitorProfiles.map((item) => item.monitorId)).toEqual(["online", "old-online", "edges-only"]);
    expect(next.hotzones).toEqual(original.hotzones);
    expect(next.mouseGestures).toEqual(original.mouseGestures);
    expect(next.edgeHide.edges).toEqual(original.edgeHide.edges);
  });

  it("rechecks a reconnected screen or disconnected helper at confirmation time", () => {
    const state = settings();
    expect(listOfflineMonitorProfiles(state, [online], true).some((item) => item.monitorId === "offline")).toBe(true);
    expect(removeOfflineMonitorProfile(state, [online, { ...online, id: "offline" }], true, "offline")).toBeNull();
    expect(removeOfflineMonitorProfile(state, [online, { ...online, id: "new-stable", legacyId: "offline" }], true, "offline")).toBeNull();
    expect(removeOfflineMonitorProfile(state, [online], false, "offline")).toBeNull();
    expect(removeOfflineMonitorProfile(state, [online], true, "unknown")).toBeNull();
  });

  it("keeps profiles present in only one list manageable and removes duplicate entries for the selected ID", () => {
    const original = settings();
    original.monitorProfiles.push(structuredClone(original.monitorProfiles[2]));
    const next = removeOfflineMonitorProfile(original, [online], true, "offline")!;
    expect(next.monitorProfiles.some((profile) => profile.monitorId === "offline")).toBe(false);
    expect(removeOfflineMonitorProfile(next, [online], true, "edges-only")!.edgeHide.monitorProfiles).toHaveLength(2);
  });
});
