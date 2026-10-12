import { describe, expect, it } from "vitest";
import fixture from "../../../tests/fixtures/hotzone-geometry.json";
import {
  effectiveHotzoneGeometry, hotzonePreviewRect, isCornerHotzone,
  normalizeHotzoneGeometry, resizeCornerGeometry
} from "./hotzone-geometry";
import type { HotzoneGeometry, HotzoneId } from "./types";

type CornerGeometry = Extract<HotzoneGeometry, { kind: "corner" }>;
const ids: HotzoneId[] = ["top-left", "top", "top-right", "right", "bottom-right", "bottom", "bottom-left", "left"];

describe("shared helper/frontend geometry contract", () => {
  it.each(fixture.normalizations)("normalizes $name", ({ id, edgeSize, input, expected }) => {
    expect(normalizeHotzoneGeometry(input, id as HotzoneId, edgeSize) ?? null).toEqual(expected);
  });

  it.each(fixture.rectangles)("previews $name", ({ id, bounds, edgeSize, expected, ...sample }) => {
    const geometry = "geometry" in sample ? sample.geometry as HotzoneGeometry : undefined;
    expect(hotzonePreviewRect(id as HotzoneId, bounds, edgeSize, geometry)).toEqual(expected);
  });
});

describe("hotzone geometry normalization", () => {
  it.each(ids)("classifies %s without treating edges as corners", (id) => {
    expect(isCornerHotzone(id)).toBe(id.includes("-"));
  });

  it.each([undefined, null, false, 7, "corner", [], ["corner"], {}, { kind: "unknown" }].map(value => [value]))(
    "rejects malformed geometry %j", (value) => {
      expect(normalizeHotzoneGeometry(value, "top-left", 24)).toBeUndefined();
      expect(normalizeHotzoneGeometry(value, "bottom", 24)).toBeUndefined();
    }
  );

  it("fills only explicit geometry from the clamped legacy size and leaves linked rectangles intact", () => {
    expect(normalizeHotzoneGeometry({ kind: "corner" }, "top-left", 24)).toEqual({ kind: "corner", width: 24, height: 24, linked: true });
    expect(normalizeHotzoneGeometry({ kind: "edge" }, "left", 24)).toEqual({ kind: "edge", thickness: 24, lengthPercent: 40 });
    expect(normalizeHotzoneGeometry({ kind: "corner" }, "top-right", 999)).toEqual({ kind: "corner", width: 48, height: 48, linked: true });
    expect(normalizeHotzoneGeometry({ kind: "edge" }, "right", -1)).toEqual({ kind: "edge", thickness: 2, lengthPercent: 40 });
    expect(normalizeHotzoneGeometry({ kind: "corner", width: 21.6, height: 65.4 }, "bottom-left", 8)).toEqual({ kind: "corner", width: 22, height: 65, linked: true });
    expect(normalizeHotzoneGeometry({ kind: "edge", thickness: 10.6, lengthPercent: 49.6 }, "bottom", 8)).toEqual({ kind: "edge", thickness: 11, lengthPercent: 50 });
  });

  it.each([null, "24", NaN, Infinity, -Infinity, {}, []].map(value => [value]))(
    "defaults non-finite/non-numeric fields %j without coercion", (value) => {
      expect(normalizeHotzoneGeometry({ kind: "corner", width: value, height: value, linked: "false" }, "top-left", 24)).toEqual({ kind: "corner", width: 24, height: 24, linked: true });
      expect(normalizeHotzoneGeometry({ kind: "edge", thickness: value, lengthPercent: value }, "top", 24)).toEqual({ kind: "edge", thickness: 24, lengthPercent: 40 });
    }
  );

  it("calculates fallback without mutating or materializing the source", () => {
    const zone = Object.freeze({ id: "bottom-left" as const });
    expect(effectiveHotzoneGeometry(zone, 32)).toEqual({ kind: "corner", width: 32, height: 32, linked: true });
    expect(Object.hasOwn(zone, "geometry")).toBe(false);
    const geometry = Object.freeze({ kind: "corner" as const, width: 24, height: 48, linked: true });
    const effective = effectiveHotzoneGeometry({ id: "top-right", geometry }, 8);
    expect(effective).toEqual(geometry);
    expect(effective).not.toBe(geometry);
    expect(effectiveHotzoneGeometry({ id: "top", geometry }, 24)).toEqual({ kind: "edge", thickness: 24, lengthPercent: 40 });
  });
});

describe("linked corner resizing", () => {
  it.each([
    ["width", 48, 48, 96], ["height", 24, 12, 24],
    ["width", 999, 64, 128], ["height", 999, 64, 128],
    ["width", 0, 2, 4], ["height", -50, 2, 4]
  ] as const)("jointly resizes %s to %s", (field, value, width, height) => {
    const geometry = Object.freeze({ kind: "corner" as const, width: 24, height: 48, linked: true });
    expect(resizeCornerGeometry(geometry, field, value)).toEqual({ kind: "corner", width, height, linked: true });
    expect(geometry).toEqual({ kind: "corner", width: 24, height: 48, linked: true });
  });

  it("resizes unlinked fields independently and retains dimensions when linking a rectangle", () => {
    const rectangle: CornerGeometry = { kind: "corner", width: 24, height: 48, linked: false };
    expect(resizeCornerGeometry(rectangle, "width", 999)).toEqual({ ...rectangle, width: 128 });
    expect(resizeCornerGeometry(rectangle, "height", 0)).toEqual({ ...rectangle, height: 2 });
    const linked = { ...rectangle, linked: true };
    expect(resizeCornerGeometry(linked, "width", linked.width)).toEqual(linked);
    expect(resizeCornerGeometry(linked, "height", linked.height)).toEqual(linked);
  });

  it("rounds both dimensions only after scaling and ignores non-finite resize input", () => {
    const geometry: CornerGeometry = { kind: "corner", width: 21, height: 32, linked: true };
    expect(resizeCornerGeometry(geometry, "width", 27)).toEqual({ ...geometry, width: 27, height: 41 });
    for (const value of [NaN, Infinity, -Infinity]) {
      expect(resizeCornerGeometry(geometry, "width", value)).toEqual(geometry);
    }
  });

  it("keeps both axes integral, bounded and proportional even at extreme ratios", () => {
    for (const width of [2, 3, 21, 64, 127, 128]) {
      for (const height of [2, 3, 32, 64, 127, 128]) {
        const geometry: CornerGeometry = { kind: "corner", width, height, linked: true };
        for (const field of ["width", "height"] as const) {
          for (const value of [-100, 0, 2, 3, 9.5, 63, 128, 1000]) {
            const resized = resizeCornerGeometry(geometry, field, value);
            expect(Number.isInteger(resized.width) && Number.isInteger(resized.height)).toBe(true);
            expect(resized.width).toBeGreaterThanOrEqual(2);
            expect(resized.width).toBeLessThanOrEqual(128);
            expect(resized.height).toBeGreaterThanOrEqual(2);
            expect(resized.height).toBeLessThanOrEqual(128);
            // At most half a pixel of rounding error on either axis of the shared scale.
            expect(Math.abs(resized.width * height - resized.height * width)).toBeLessThanOrEqual((width + height) / 2);
          }
        }
      }
    }
  });
});

describe("absolute preview rectangles", () => {
  const bounds = { left: -100, top: -200, right: 0, bottom: 0 };

  it.each([
    ["top-left", { left: -100, top: -200, right: -80, bottom: -170 }],
    ["top-right", { left: -20, top: -200, right: 0, bottom: -170 }],
    ["bottom-left", { left: -100, top: -30, right: -80, bottom: 0 }],
    ["bottom-right", { left: -20, top: -30, right: 0, bottom: 0 }]
  ] as const)("anchors corner %s on signed coordinates", (id, expected) => {
    expect(hotzonePreviewRect(id, bounds, 8, { kind: "corner", width: 20, height: 30, linked: true })).toEqual(expected);
  });

  it.each([
    ["top", { left: -75, top: -200, right: -25, bottom: -190 }],
    ["bottom", { left: -75, top: -10, right: -25, bottom: 0 }],
    ["left", { left: -100, top: -150, right: -90, bottom: -50 }],
    ["right", { left: -10, top: -150, right: 0, bottom: -50 }]
  ] as const)("centers edge %s", (id, expected) => {
    expect(hotzonePreviewRect(id, bounds, 8, { kind: "edge", thickness: 10, lengthPercent: 50 })).toEqual(expected);
  });

  it("uses legacy square/centered 40% and rejects mismatched overrides", () => {
    expect(hotzonePreviewRect("top-left", bounds, 24)).toEqual({ left: -100, top: -200, right: -76, bottom: -176 });
    expect(hotzonePreviewRect("top", bounds, 24)).toEqual({ left: -70, top: -200, right: -30, bottom: -176 });
    expect(hotzonePreviewRect("top", bounds, 24, { kind: "corner", width: 128, height: 128, linked: false })).toEqual(hotzonePreviewRect("top", bounds, 24));
  });

  it("clamps thickness and length to single-pixel displays and supports full edges", () => {
    const tiny = { left: -1, top: -1, right: 0, bottom: 0 };
    for (const id of ids) expect(hotzonePreviewRect(id, tiny, 48)).toEqual(tiny);
    expect(hotzonePreviewRect("left", bounds, 8, { kind: "edge", thickness: 99, lengthPercent: 100 })).toEqual({ left: -100, top: -200, right: -52, bottom: 0 });
  });
});
