import { describe, expect, it } from "vitest";
import { unavailableEdges } from "./monitor-topology";
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
