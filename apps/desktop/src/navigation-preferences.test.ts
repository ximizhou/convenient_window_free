// @vitest-environment happy-dom
import { beforeEach, describe, expect, it } from "vitest";
import { MAX_ACTIVE_NAVIGATION, defaultNavigationPreferences, loadNavigationPreferences, normalizeNavigationPreferences, saveNavigationPreferences } from "./navigation-preferences";

describe("navigation preferences", () => {
  beforeEach(() => {
    localStorage.clear();
    Reflect.deleteProperty(window, "utools");
  });

  it("keeps the runtime center visible while accepting custom order and visibility", () => {
    const value = normalizeNavigationPreferences({ order: ["window-layouts", "power", "power", "unknown"], visible: ["window-layouts"] });
    expect(value.order).toEqual(["window-layouts", "power", "hotzones", "edge-hide", "gestures", "beautification", "startup"]);
    expect(value.visible).toEqual(["power", "window-layouts"]);
  });

  it("trims migrated entries that exceed the active-row capacity", () => {
    const value = normalizeNavigationPreferences({ visible: ["window-layouts", "beautification", "gestures", "edge-hide", "hotzones", "power"] });
    expect(value.visible).toEqual(["power", "window-layouts", "beautification", "gestures", "edge-hide"]);
  });

  it("uses the default five bottom entries for a missing preference", () => {
    expect(normalizeNavigationPreferences(null)).toEqual(defaultNavigationPreferences());
  });

  it("keeps four configurable entries active and window layouts in the secondary row by default", () => {
    const value = defaultNavigationPreferences();
    expect(value.visible).toEqual(["power", "hotzones", "edge-hide", "gestures", "beautification"]);
    expect(value.order.filter((id) => !value.visible.includes(id))).toEqual(["window-layouts", "startup"]);
  });

  it("collapses a previously saved fifth entry into the spare row", () => {
    const value = normalizeNavigationPreferences({
      order: ["power", "hotzones", "edge-hide", "gestures", "beautification", "window-layouts"],
      visible: ["power", "hotzones", "edge-hide", "gestures", "beautification", "window-layouts"]
    });
    expect(value.visible).toEqual(["power", "hotzones", "edge-hide", "gestures", "beautification"]);
    expect(value.visible.length - 1).toBe(MAX_ACTIVE_NAVIGATION);
    expect(value.order).toContain("window-layouts");
  });

  it("persists through localStorage when the host database is unavailable", () => {
    const value = normalizeNavigationPreferences({ visible: ["power", "window-layouts"] });
    saveNavigationPreferences(value);
    expect(loadNavigationPreferences()).toEqual(value);
  });
});
