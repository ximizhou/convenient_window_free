import { resampleGesture } from "./gesture-algorithm";
import type { GesturePoint } from "./types";

// Editing a slider keeps the recorded sample arrays alive. Weak keys bound the
// cache to those arrays; coordinate snapshots also detect in-place edits.
const samples = new WeakMap<object, { source: GesturePoint[]; normalized: GesturePoint[] }>();

export function normalizeGestureSample(value: unknown): GesturePoint[] {
  if (!Array.isArray(value)) return [];
  const cached = samples.get(value);
  if (cached && sameCoordinates(value, cached.source)) return copyPoints(cached.normalized);

  const source = value
    .filter((point) => point && typeof point === "object")
    .map((point) => point as Partial<GesturePoint>)
    .filter((point) => Number.isFinite(point.x) && Number.isFinite(point.y))
    .map((point) => ({ x: Number(point.x), y: Number(point.y) }));
  const normalized = resampleGesture(source.map((point) => ({
    x: Math.min(1, Math.max(0, point.x)),
    y: Math.min(1, Math.max(0, point.y))
  })));
  // Malformed imports still take the full validation path. Never retain the
  // caller's points or return the cached output by reference.
  if (source.length === value.length) samples.set(value, { source, normalized });
  else samples.delete(value);
  return copyPoints(normalized);
}

function sameCoordinates(value: unknown[], source: GesturePoint[]): boolean {
  if (value.length !== source.length) return false;
  for (let index = 0; index < source.length; index++) {
    const point = value[index] as Partial<GesturePoint> | null | undefined;
    if (!point || typeof point !== "object" || point.x !== source[index].x || point.y !== source[index].y) return false;
  }
  return true;
}

function copyPoints(points: GesturePoint[]): GesturePoint[] {
  return points.map((point) => ({ ...point }));
}
