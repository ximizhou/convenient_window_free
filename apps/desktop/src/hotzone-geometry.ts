import type { DisplayInfo, HotzoneGeometry, HotzoneId, HotzoneSetting } from "./types";

type CornerGeometry = Extract<HotzoneGeometry, { kind: "corner" }>;

export function isCornerHotzone(id: HotzoneId): boolean {
  return id === "top-left" || id === "top-right" || id === "bottom-left" || id === "bottom-right";
}

export function normalizeHotzoneGeometry(
  value: unknown,
  id: HotzoneId,
  edgeSize: number
): HotzoneGeometry | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const geometry = value as Record<string, unknown>;
  const legacySize = integerInRange(edgeSize, 8, 2, 48);
  if (isCornerHotzone(id)) {
    if (geometry.kind !== "corner") return undefined;
    return {
      kind: "corner",
      width: integerInRange(geometry.width, legacySize, 2, 128),
      height: integerInRange(geometry.height, legacySize, 2, 128),
      linked: typeof geometry.linked === "boolean" ? geometry.linked : true
    };
  }
  if (geometry.kind !== "edge") return undefined;
  return {
    kind: "edge",
    thickness: integerInRange(geometry.thickness, legacySize, 2, 48),
    lengthPercent: integerInRange(geometry.lengthPercent, 40, 10, 100)
  };
}

export function effectiveHotzoneGeometry(
  zone: Pick<HotzoneSetting, "id" | "geometry">,
  edgeSize: number
): HotzoneGeometry {
  return normalizeHotzoneGeometry(zone.geometry, zone.id, edgeSize)
    ?? normalizeHotzoneGeometry({ kind: isCornerHotzone(zone.id) ? "corner" : "edge" }, zone.id, edgeSize)!;
}

export function resizeCornerGeometry(
  geometry: CornerGeometry,
  field: "width" | "height",
  value: number
): CornerGeometry {
  const width = integerInRange(geometry.width, 8, 2, 128);
  const height = integerInRange(geometry.height, 8, 2, 128);
  const current = field === "width" ? width : height;
  const requested = typeof value === "number" && Number.isFinite(value) ? value : current;
  if (!geometry.linked) {
    return { ...geometry, width, height, [field]: integerInRange(requested, current, 2, 128) };
  }
  // Clamp one shared scale, not two independent dimensions, to retain the current ratio.
  const minScale = Math.max(2 / width, 2 / height);
  const maxScale = Math.min(128 / width, 128 / height);
  const scale = Math.min(maxScale, Math.max(minScale, requested / current));
  return { ...geometry, width: Math.round(width * scale), height: Math.round(height * scale) };
}

export function hotzonePreviewRect(
  id: HotzoneId,
  bounds: DisplayInfo["bounds"],
  edgeSize: number,
  geometry?: HotzoneGeometry
): DisplayInfo["bounds"] {
  const effective = effectiveHotzoneGeometry({ id, geometry }, edgeSize);
  const screenWidth = Math.max(0, bounds.right - bounds.left);
  const screenHeight = Math.max(0, bounds.bottom - bounds.top);
  if (effective.kind === "corner") {
    const width = Math.min(screenWidth, effective.width);
    const height = Math.min(screenHeight, effective.height);
    const left = id === "top-right" || id === "bottom-right" ? bounds.right - width : bounds.left;
    const top = id === "bottom-left" || id === "bottom-right" ? bounds.bottom - height : bounds.top;
    return { left, top, right: left + width, bottom: top + height };
  }
  const horizontal = id === "top" || id === "bottom";
  const dimension = horizontal ? screenWidth : screenHeight;
  const length = Math.min(dimension, Math.max(1, Math.round(dimension * effective.lengthPercent / 100)));
  const thickness = Math.min(horizontal ? screenHeight : screenWidth, effective.thickness);
  const offset = Math.floor((dimension - length) / 2);
  if (horizontal) {
    const left = bounds.left + offset;
    const top = id === "bottom" ? bounds.bottom - thickness : bounds.top;
    return { left, top, right: left + length, bottom: top + thickness };
  }
  const left = id === "right" ? bounds.right - thickness : bounds.left;
  const top = bounds.top + offset;
  return { left, top, right: left + thickness, bottom: top + length };
}

function integerInRange(value: unknown, fallback: number, min: number, max: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, Math.round(value)));
}
