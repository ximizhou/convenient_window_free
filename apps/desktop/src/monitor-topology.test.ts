import { describe, expect, it } from "vitest";
import { sameDisplaySnapshot, unavailableEdges } from "./monitor-topology";
import type { DisplayInfo } from "./types";

function display(id: string, left: number, top: number, right: number, bottom: number): DisplayInfo {
  return {
    id,
    primary: id === "primary",
    bounds: { left, top, right, bottom },
    workArea: { left, top, right, bottom }
  };
}

describe("unavailableEdges", () => {
  it("marks the full seam between side-by-side displays as unavailable", () => {
    const left = display("left", -1920, 0, 0, 1080);
    const primary = display("primary", 0, 0, 1920, 1080);

    expect(unavailableEdges(left, [left, primary])).toEqual(["right"]);
    expect(unavailableEdges(primary, [left, primary])).toEqual(["left"]);
  });

  it("keeps a partially exposed edge configurable", () => {
    const primary = display("primary", 0, 0, 1920, 1080);
    const shortLeft = display("left", -1280, 300, 0, 1024);

    expect(unavailableEdges(primary, [primary, shortLeft])).not.toContain("left");
  });
});


describe("runtime display snapshot reuse", () => {
  const one: DisplayInfo = { id: "one", legacyId: "old-one", primary: true,
    bounds: { left: -1920, top: 0, right: 0, bottom: 1080 },
    workArea: { left: -1920, top: 0, right: 0, bottom: 1040 } };
  it("reuses unchanged decoded status snapshots, without a JSON traversal", () => {
    expect(sameDisplaySnapshot([one], structuredClone([one]))).toBe(true);
    expect(sameDisplaySnapshot([], [])).toBe(true);
  });
  it("does not suppress topology, primary, legacy identity, or work-area changes", () => {
    for (const changed of [
      { ...one, id: "two" }, { ...one, legacyId: undefined }, { ...one, primary: false },
      { ...one, bounds: { ...one.bounds, left: -1280 } },
      { ...one, workArea: { ...one.workArea, bottom: 1000 } }
    ]) expect(sameDisplaySnapshot([one], [changed])).toBe(false);
    expect(sameDisplaySnapshot([one], [])).toBe(false);
    expect(sameDisplaySnapshot([one, { ...one, id: "two" }], [{ ...one, id: "two" }, one])).toBe(false);
  });
});
