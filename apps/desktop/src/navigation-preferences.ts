import { loadHostValue, saveHostValue } from "./host-storage";

export const NAVIGATION_KEY = "convenient-window-navigation";

export const NAVIGATION_FEATURES = [
  "power",
  "hotzones",
  "edge-hide",
  "gestures",
  "beautification",
  "window-layouts",
  "startup"
] as const;

/** 第一行（底部页签）最多显示 4 个功能入口；“运行中心”固定可见，不占用这 4 个名额。 */
export const MAX_ACTIVE_NAVIGATION = 4;

export type NavigationFeatureId = typeof NAVIGATION_FEATURES[number];

export interface NavigationPreferences {
  order: NavigationFeatureId[];
  visible: NavigationFeatureId[];
}

const DEFAULT_ORDER: NavigationFeatureId[] = [...NAVIGATION_FEATURES];
const DEFAULT_VISIBLE: NavigationFeatureId[] = [
  "power",
  "hotzones",
  "edge-hide",
  "gestures",
  "beautification"
];

export function defaultNavigationPreferences(): NavigationPreferences {
  return { order: [...DEFAULT_ORDER], visible: [...DEFAULT_VISIBLE] };
}

export function normalizeNavigationPreferences(value: unknown): NavigationPreferences {
  const source = value && typeof value === "object" ? value as Record<string, unknown> : {};
  const order = normalizeList(source.order, DEFAULT_ORDER, true);
  const rawVisible = normalizeList(source.visible, DEFAULT_VISIBLE, false);
  // 旧版本允许显示 5 个功能入口；这里按新规则收敛到 4 个，多出的入口留在备用行。
  const visible: NavigationFeatureId[] = ["power"];
  for (const id of rawVisible) {
    if (id === "power") continue;
    if (visible.length - 1 >= MAX_ACTIVE_NAVIGATION) break;
    visible.push(id);
  }
  return { order, visible };
}

export function loadNavigationPreferences(): NavigationPreferences {
  return normalizeNavigationPreferences(loadHostValue<unknown>(NAVIGATION_KEY));
}

export function saveNavigationPreferences(value: NavigationPreferences): void {
  saveHostValue(NAVIGATION_KEY, normalizeNavigationPreferences(value));
}

function normalizeList(value: unknown, fallback: NavigationFeatureId[], appendMissing: boolean): NavigationFeatureId[] {
  const source = Array.isArray(value) ? value : fallback;
  const result: NavigationFeatureId[] = [];
  for (const item of source) {
    if (typeof item !== "string" || !isNavigationFeatureId(item)) continue;
    if (!result.includes(item)) result.push(item);
  }
  if (appendMissing) for (const item of fallback) if (!result.includes(item)) result.push(item);
  return result;
}

function isNavigationFeatureId(value: string): value is NavigationFeatureId {
  return (NAVIGATION_FEATURES as readonly string[]).includes(value);
}
