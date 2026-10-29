import { describe, expect, it } from "vitest";
import { filterStartupItems, readStartupItems, startupErrorKey, startupNameFromPath } from "./startup-manager";
import type { StartupItem, StartupResponse } from "./startup-manager";

const item: StartupItem = { id: "app", name: "Editor", command: '"C:\\Tools\\editor.exe" --tray', source: "userRun", enabled: true, editable: true, revision: "abc" };

describe("startup manager response boundaries", () => {
  it("accepts complete snapshots including unknown states and returns detached item objects", () => {
    const response: StartupResponse = { ok: true, version: 1, items: [item, { ...item, id: "unknown", enabled: null, editable: false }] };
    const parsed = readStartupItems(response);
    expect(parsed).toEqual(response.items);
    expect(parsed?.[0]).not.toBe(item);
  });

  it.each([
    { ok: true, version: 1 },
    { ok: false, version: 1, items: [item] },
    { ok: true, version: 2, items: [item] },
    { ok: true, version: 1, items: [item, item] },
    { ok: true, version: 1, items: [{ ...item, revision: "" }] },
    { ok: true, version: 1, items: [{ ...item, enabled: "true" }] },
    { ok: true, version: 1, items: [{ ...item, source: "service" }] }
  ])("rejects incomplete or ambiguous snapshots %#", response => {
    expect(readStartupItems(response as StartupResponse)).toBeNull();
  });

  it("filters unknown states only into All and searches names and commands case-insensitively", () => {
    const items = [item, { ...item, id: "off", name: "Notes", enabled: false }, { ...item, id: "unknown", enabled: null }];
    expect(filterStartupItems(items, "", "enabled").map(i => i.id)).toEqual(["app"]);
    expect(filterStartupItems(items, "", "disabled").map(i => i.id)).toEqual(["off"]);
    expect(filterStartupItems(items, " NOTES ", "all").map(i => i.id)).toEqual(["off"]);
    expect(filterStartupItems(items, "TOOLS", "all")).toHaveLength(3);
  });

  it("suggests an app name from the chosen executable without exposing directory names", () => {
    expect(startupNameFromPath("C:\\Users\\someone\\My Editor.exe")).toBe("My Editor");
    expect(startupNameFromPath("/tools/editor.EXE")).toBe("editor");
  });

  it("maps backend codes to stable UI messages and never returns raw backend text", () => {
    expect(startupErrorKey("ordinaryUserRequired")).toBe("ordinaryUser");
    expect(startupErrorKey("invalidName")).toBe("invalidName");
    expect(startupErrorKey("conflict")).toBe("changed");
    expect(startupErrorKey("readOnly")).toBe("accessDenied");
    expect(startupErrorKey("needsHelper")).toBe("needsHelper");
    expect(startupErrorKey("notFound")).toBe("missing");
    expect(startupErrorKey("unavailable")).toBe("unavailable");
    expect(startupErrorKey("拒绝访问 C:\\Users\\someone")).toBe("failed");
  });
});
