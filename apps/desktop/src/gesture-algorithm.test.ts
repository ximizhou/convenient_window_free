import { describe, expect, it } from "vitest";
import { gestureSimilarity, resampleGesture } from "./gesture-algorithm";

describe("gesture algorithm", () => {
  it("resamples a stroke to the helper's fixed 64 point representation", () => {
    const sampled = resampleGesture([{ x: 0, y: 0 }, { x: 1, y: 1 }]);
    expect(sampled).toHaveLength(64);
    expect(sampled[0]).toEqual({ x: 0, y: 0 });
    expect(sampled.at(-1)).toEqual({ x: 1, y: 1 });
    expect(sampled[32].x).toBeCloseTo(32 / 63);
  });

  it("compares path geometry rather than the source event sampling rate", () => {
    const sparse = [{ x: 0, y: 0 }, { x: 0.5, y: 0.5 }, { x: 1, y: 1 }];
    const uneven = [{ x: 0, y: 0 }, { x: 0.01, y: 0.01 }, { x: 0.03, y: 0.03 }, { x: 1, y: 1 }];
    expect(gestureSimilarity(sparse, uneven)).toBeGreaterThan(0.999);
  });

  it("applies the helper's path-efficiency penalty to backtracking strokes", () => {
    const straight = [{ x: 0.5, y: 1 }, { x: 0.5, y: 0 }];
    const backtracking = [
      { x: 0.5, y: 1 },
      { x: 0.5, y: 0.45 },
      { x: 0.5, y: 0.8 },
      { x: 0.5, y: 0 }
    ];
    expect(gestureSimilarity(straight, backtracking)).toBeLessThan(0.88);
  });
});
