import type { GesturePoint } from "./types";

export const GESTURE_SAMPLE_POINTS = 64;

export function resampleGesture(
  points: GesturePoint[],
  count = GESTURE_SAMPLE_POINTS
): GesturePoint[] {
  if (count < 2 || points.length < 2 || points.some((point) => !Number.isFinite(point.x) || !Number.isFinite(point.y))) {
    return [];
  }
  const lengths = points.slice(1).map((point, index) => Math.hypot(
    point.x - points[index].x,
    point.y - points[index].y
  ));
  const total = lengths.reduce((sum, length) => sum + length, 0);
  if (total <= Number.EPSILON) return [];

  const output: GesturePoint[] = [];
  let segment = 0;
  let traversed = 0;
  for (let index = 0; index < count; index += 1) {
    const target = total * index / (count - 1);
    while (segment + 1 < points.length - 1 && traversed + lengths[segment] < target) {
      traversed += lengths[segment];
      segment += 1;
    }
    const segmentLength = Math.max(lengths[segment], Number.EPSILON);
    const ratio = Math.min(1, Math.max(0, (target - traversed) / segmentLength));
    output.push({
      x: points[segment].x + (points[segment + 1].x - points[segment].x) * ratio,
      y: points[segment].y + (points[segment + 1].y - points[segment].y) * ratio
    });
  }
  return output;
}

export function gestureSimilarity(left: GesturePoint[], right: GesturePoint[]): number {
  const a = normalizedGesture(left);
  const b = normalizedGesture(right);
  if (!a.length || !b.length) return 0;
  const distance = a.reduce(
    (sum, point, index) => sum + Math.hypot(point.x - b[index].x, point.y - b[index].y),
    0
  ) / a.length;
  const efficiencyPenalty = Math.abs(pathEfficiency(left) - pathEfficiency(right)) * 0.45;
  return Math.min(1, Math.max(0, 1 - distance / Math.SQRT2 - efficiencyPenalty));
}

function normalizedGesture(points: GesturePoint[]): GesturePoint[] {
  const sampled = resampleGesture(points);
  if (!sampled.length) return [];
  const minX = Math.min(...sampled.map((point) => point.x));
  const maxX = Math.max(...sampled.map((point) => point.x));
  const minY = Math.min(...sampled.map((point) => point.y));
  const maxY = Math.max(...sampled.map((point) => point.y));
  const scale = Math.max(maxX - minX, maxY - minY);
  if (!Number.isFinite(scale) || scale <= Number.EPSILON) return [];
  const centerX = sampled.reduce((sum, point) => sum + point.x, 0) / sampled.length;
  const centerY = sampled.reduce((sum, point) => sum + point.y, 0) / sampled.length;
  return sampled.map((point) => ({
    x: (point.x - centerX) / scale,
    y: (point.y - centerY) / scale
  }));
}

function pathEfficiency(points: GesturePoint[]): number {
  if (points.length < 2) return 0;
  const first = points[0];
  const last = points[points.length - 1];
  const displacement = Math.hypot(last.x - first.x, last.y - first.y);
  const length = points.slice(1).reduce(
    (sum, point, index) => sum + Math.hypot(point.x - points[index].x, point.y - points[index].y),
    0
  );
  return length <= Number.EPSILON ? 0 : displacement / length;
}
