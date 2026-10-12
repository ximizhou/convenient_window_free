import { migrateGestureName } from "./gesture-names";
import { normalizeHotzoneGeometry } from "./hotzone-geometry";
import type {
  AppSettings, GesturePoint, GestureTemplate, GestureTriggerButton, HotzoneAction,
  HotzoneId, HotzoneSetting, ModifierAction, ModifierKey, MouseButton, OcrLanguage,
  ScreenshotResultMode, TriggerAction, TriggerKind
} from "./types";
import { getOptionalHostBridge } from "./host-bridge";
import { resampleGesture } from "./gesture-algorithm";

export const SETTINGS_SCHEMA_VERSION = 9;
export const MAX_GESTURE_TEMPLATES = 64;
export const MAX_SETTINGS_STORAGE_BYTES = 900 * 1024;

const SETTINGS_KEY = "magic-corners.settings";
const TOPMOST_PIN_DEFAULTS_VERSION = 1;

const hotzoneIds: HotzoneId[] = [
  "top-left",
  "top",
  "top-right",
  "right",
  "bottom-right",
  "bottom",
  "bottom-left",
  "left"
];

const triggerKinds: TriggerKind[] = [
  "hover",
  "left-click",
  "right-click",
  "wheel-up",
  "wheel-down",
  "slide-forward",
  "slide-backward"
];

const modifierKeys: ModifierKey[] = ["ctrl", "alt", "shift", "win"];
const mouseButtons: MouseButton[] = ["left", "right", "middle", "x1", "x2"];
const ocrLanguages: OcrLanguage[] = ["auto", "zh-Hans", "en"];
const screenshotResultModes: ScreenshotResultMode[] = ["pin", "copy-text", "pin-and-copy"];

export const defaultSettings: AppSettings = {
  schemaVersion: SETTINGS_SCHEMA_VERSION,
  taskbarAppearance: {
    enabled: false,
    mode: "transparent",
    opacity: 58,
    tint: "#233A63",
    showBorder: false
  },
  enabled: true,
  hotzonesEnabled: true,
  showHotzoneHint: true,
  edgeSize: 8,
  hoverDelayMs: 350,
  pollIntervalMs: 33,
  actionCooldownMs: 700,
  pausedApps: [],
  hotzones: hotzoneIds.map((id) => ({
    id,
    enabled: false,
    actions: triggerKinds.map((trigger) => ({
      trigger,
      action: { kind: "none" },
      modifierActions: [],
      cooldownMs: 700,
      hoverDelayMs: 350
    }))
  })),
  monitorProfiles: [],
  edgeHide: {
    enabled: false,
    animationEnabled: true,
    animationDurationMs: 160,
    topTriggerDistance: 48,
    topHoverDelayMs: 600,
    showPreview: true,
    showRestoreHint: true,
    keepExpandedWhenForeground: false,
    edges: ["left", "top", "right", "bottom"],
    monitorProfiles: [],
    stripSize: 16,
    triggerDistance: 16,
    distanceTriggerEnabled: true,
    ratioTriggerEnabled: true,
    triggerRatio: 33,
    collapseDelayMs: 300,
    restoreDelayMs: 200,
    excludedApps: []
  },
  windowDrag: {
    enabled: false,
    updateHz: 120,
    moveModifiers: ["alt"],
    resizeModifiers: ["alt"],
    moveButton: "left",
    resizeButton: "right",
    pausedApps: []
  },
  topmostPin: {
    enabled: true,
    defaultsVersion: TOPMOST_PIN_DEFAULTS_VERSION
  },
  ocr: {
    language: "auto",
    screenshotResult: "pin",
    pinOffset: true
  },
  mouseGestures: {
    enabled: false,
    triggerButton: "right",
    minDistance: 36,
    sensitivity: 72,
    showTrail: true,
    fullscreenPause: true,
    pausedApps: [],
    gestures: [
      builtinGesture("gesture-up", { kind: "shortcut", value: "Ctrl+C" }, [[0.5, 0.9], [0.5, 0.1]]),
      builtinGesture("gesture-down", { kind: "shortcut", value: "Ctrl+V" }, [[0.5, 0.1], [0.5, 0.9]]),
      builtinGesture("gesture-l", { kind: "shortcut", value: "Alt+F4" }, [[0.2, 0.1], [0.2, 0.85], [0.85, 0.85]]),
      builtinGesture("gesture-circle", { kind: "toggle-window-topmost" }, circleSample()),
      {
        ...builtinGesture("gesture-rectangle", { kind: "none" }, [[0.15, 0.15], [0.85, 0.15], [0.85, 0.85], [0.15, 0.85], [0.15, 0.15]]),
        mode: "region-screenshot"
      }
    ]
  }
};

export function loadSettings(): AppSettings {
  const stored = loadStoredSettings();
  const settings = normalizeSettings(stored);
  // Old 0.6.x profiles did not expose this control. Restore the new default once,
  // then persist the marker with the next normal settings write.
  if (stored?.topmostPin?.defaultsVersion !== TOPMOST_PIN_DEFAULTS_VERSION) {
    settings.topmostPin.enabled = true;
  }
  return settings;
}

export function normalizeSettings(stored: Partial<AppSettings> | null | undefined): AppSettings {
  if (stored && typeof stored.schemaVersion === "number" && stored.schemaVersion > SETTINGS_SCHEMA_VERSION) {
    throw new Error("Unsupported settings schema version");
  }
  if (!stored) {
    return cloneSettings(defaultSettings);
  }

  const edgeHide = stored.edgeHide;
  const edgeSize = integerInRange(stored.edgeSize, defaultSettings.edgeSize, 2, 48);
  const sourceSchemaVersion = stored.schemaVersion ?? 0;
  const hoverDelayMs = integerInRange(stored.hoverDelayMs, defaultSettings.hoverDelayMs, 0, 3000);
  const actionCooldownMs = integerInRange(
    stored.actionCooldownMs,
    defaultSettings.actionCooldownMs,
    10,
    5000
  );
  return {
    schemaVersion: SETTINGS_SCHEMA_VERSION,
    enabled: booleanValue(stored.enabled, defaultSettings.enabled),
    hotzonesEnabled: typeof stored.hotzonesEnabled === "boolean"
      ? stored.hotzonesEnabled
      : sourceSchemaVersion < 6
        ? legacyHotzonesEnabled(stored)
        : defaultSettings.hotzonesEnabled,
    showHotzoneHint: booleanValue(stored.showHotzoneHint, defaultSettings.showHotzoneHint),
    edgeSize,
    hoverDelayMs,
    pollIntervalMs: integerInRange(stored.pollIntervalMs, defaultSettings.pollIntervalMs, 10, 250),
    actionCooldownMs,
    pausedApps: stringList(stored.pausedApps),
    edgeHide: {
      enabled: booleanValue(edgeHide?.enabled, defaultSettings.edgeHide.enabled),
      animationEnabled: booleanValue(edgeHide?.animationEnabled, defaultSettings.edgeHide.animationEnabled),
      animationDurationMs: integerInRange(edgeHide?.animationDurationMs, 160, 80, 800),
      topTriggerDistance: integerInRange(edgeHide?.topTriggerDistance, 48, 8, 128),
      topHoverDelayMs: integerInRange(edgeHide?.topHoverDelayMs, 600, 200, 2000),
      showPreview: booleanValue(edgeHide?.showPreview, defaultSettings.edgeHide.showPreview),
      showRestoreHint: booleanValue(
        edgeHide?.showRestoreHint,
        defaultSettings.edgeHide.showRestoreHint
      ),
      keepExpandedWhenForeground: booleanValue(
        edgeHide?.keepExpandedWhenForeground,
        defaultSettings.edgeHide.keepExpandedWhenForeground
      ),
      edges: edgeList(edgeHide?.edges),
      monitorProfiles: normalizeEdgeHideProfiles(edgeHide?.monitorProfiles),
      stripSize: integerInRange(edgeHide?.stripSize, defaultSettings.edgeHide.stripSize, 4, 64),
      triggerDistance: integerInRange(
        edgeHide?.triggerDistance,
        defaultSettings.edgeHide.triggerDistance,
        4,
        96
      ),
      distanceTriggerEnabled: booleanValue(
        edgeHide?.distanceTriggerEnabled,
        defaultSettings.edgeHide.distanceTriggerEnabled
      ),
      ratioTriggerEnabled: booleanValue(
        edgeHide?.ratioTriggerEnabled,
        defaultSettings.edgeHide.ratioTriggerEnabled
      ),
      triggerRatio: integerInRange(
        edgeHide?.triggerRatio,
        defaultSettings.edgeHide.triggerRatio,
        1,
        100
      ),
      collapseDelayMs: integerInRange(
        edgeHide?.collapseDelayMs,
        defaultSettings.edgeHide.collapseDelayMs,
        0,
        5000
      ),
      restoreDelayMs: integerInRange(
        edgeHide?.restoreDelayMs,
        defaultSettings.edgeHide.restoreDelayMs,
        0,
        5000
      ),
      excludedApps: stringList(edgeHide?.excludedApps)
    },
    mouseGestures: normalizeMouseGestures(stored.mouseGestures, sourceSchemaVersion),
    windowDrag: normalizeWindowDrag(stored.windowDrag),
    topmostPin: {
      enabled: booleanValue(stored.topmostPin?.enabled, defaultSettings.topmostPin.enabled),
      defaultsVersion: TOPMOST_PIN_DEFAULTS_VERSION
    },
    ocr: normalizeOcr(stored.ocr),
    taskbarAppearance: normalizeTaskbarAppearance(stored.taskbarAppearance),
    hotzones: normalizeHotzones(stored.hotzones, hoverDelayMs, actionCooldownMs, edgeSize),
    monitorProfiles: Array.isArray(stored.monitorProfiles)
      ? stored.monitorProfiles
          .filter((profile) => typeof profile?.monitorId === "string" && profile.monitorId.trim())
          .map((profile) => ({
            monitorId: profile.monitorId.trim(),
            hotzones: normalizeHotzones(profile.hotzones, hoverDelayMs, actionCooldownMs, edgeSize)
          }))
      : []
  };
}

function legacyHotzonesEnabled(stored: Partial<AppSettings>): boolean {
  const globalHotzones = Array.isArray(stored.hotzones) ? stored.hotzones : [];
  const profileHotzones = Array.isArray(stored.monitorProfiles)
    ? stored.monitorProfiles.flatMap((profile) => Array.isArray(profile?.hotzones) ? profile.hotzones : [])
    : [];
  return [...globalHotzones, ...profileHotzones].some((zone) => zone?.enabled === true);
}

function normalizeHotzones(
  value: unknown,
  hoverDelayMs = defaultSettings.hoverDelayMs,
  actionCooldownMs = defaultSettings.actionCooldownMs,
  edgeSize = defaultSettings.edgeSize
): HotzoneSetting[] {
  const savedList = Array.isArray(value) ? value : [];
  return defaultSettings.hotzones.map((fallback) => {
    const saved = savedList.find((item) => item?.id === fallback.id) as
      | (Partial<HotzoneSetting> & { trigger?: unknown; action?: unknown })
      | undefined;
    if (!saved) return cloneHotzone(fallback);
    const geometry = normalizeHotzoneGeometry(saved.geometry, fallback.id, edgeSize);
    const legacyTrigger = saved.trigger === "wheel" ? "wheel-up" : saved.trigger;
    const rawActions = Array.isArray(saved.actions)
      ? saved.actions
      : isTriggerKind(legacyTrigger)
        ? [{ trigger: legacyTrigger, action: saved.action }]
        : [];
    return {
      id: fallback.id,
      enabled: booleanValue(saved.enabled, fallback.enabled),
      ...(geometry ? { geometry } : {}),
      actions: triggerKinds.map((trigger) => {
        const raw = rawActions.find((item) => item?.trigger === trigger) as Partial<TriggerAction> | undefined;
        const action = normalizeAction(raw?.action);
        return {
          trigger,
          action,
          modifierActions: normalizeModifierActions(raw?.modifierActions),
          cooldownMs: integerInRange(
            raw?.cooldownMs,
            action.kind === "volume-adjust" || action.kind === "brightness-adjust"
              ? Math.min(actionCooldownMs, 32) : actionCooldownMs,
            10,
            5000
          ),
          hoverDelayMs: integerInRange(raw?.hoverDelayMs, hoverDelayMs, 0, 3000)
        };
      })
    };
  });
}

function isTriggerKind(value: unknown): value is TriggerKind {
  if (value === "wheel") return false;
  return typeof value === "string" && triggerKinds.includes(value as TriggerKind);
}

function normalizeAction(value: unknown): HotzoneAction {
  if (!value || typeof value !== "object") return { kind: "none" };
  const action = value as Partial<HotzoneAction>;
  const normalizedValue = optionalString(action.value);
  if (action.kind === "shortcut" && normalizedValue?.toLowerCase() === "volumeup") {
    return { kind: "volume-adjust", value: "0.02" };
  }
  if (action.kind === "shortcut" && normalizedValue?.toLowerCase() === "volumedown") {
    return { kind: "volume-adjust", value: "-0.02" };
  }
  const normalizedKind = typeof action.kind === "string" && action.kind.trim().length <= 80
    ? action.kind.trim()
    : "none";
  return {
    kind: normalizedKind || "none",
    value: normalizedValue
  };
}

function normalizeMouseGestures(value: unknown, sourceSchemaVersion = 5): AppSettings["mouseGestures"] {
  const raw = value && typeof value === "object"
    ? value as Partial<AppSettings["mouseGestures"]>
    : {};
  const triggerButtons: GestureTriggerButton[] = ["right", "middle", "x1", "x2"];
  const rawGestures = Array.isArray(raw.gestures) ? raw.gestures : [];
  const defaultsById = new Map(defaultSettings.mouseGestures.gestures.map((gesture) => [gesture.id, gesture]));
  const normalized: GestureTemplate[] = [];
  for (const item of rawGestures) {
    const gesture = normalizeGesture(item, defaultsById.get(typeof item?.id === "string" ? item.id.trim().slice(0, 80) : ""), sourceSchemaVersion);
    if (gesture && !normalized.some((existing) => existing.id === gesture.id)) normalized.push(gesture);
  }
  for (const fallback of defaultSettings.mouseGestures.gestures) {
    if (!normalized.some((gesture) => gesture.id === fallback.id)) normalized.push(cloneGesture(fallback));
  }
  const builtinIds = new Set(defaultSettings.mouseGestures.gestures.map((gesture) => gesture.id));
  const builtinGestures = defaultSettings.mouseGestures.gestures.map((fallback) =>
    normalized.find((gesture) => gesture.id === fallback.id) ?? cloneGesture(fallback)
  );
  if (sourceSchemaVersion < 5) {
    const circle = builtinGestures.find((gesture) => gesture.id === "gesture-circle");
    if (circle?.action.kind === "show-desktop") {
      circle.action = { kind: "toggle-window-topmost" };
    }
  }
  const customGestures = normalized
    .filter((gesture) => !builtinIds.has(gesture.id))
    .slice(0, MAX_GESTURE_TEMPLATES - builtinGestures.length);
  return {
    enabled: booleanValue(raw.enabled, defaultSettings.mouseGestures.enabled),
    triggerButton: triggerButtons.includes(raw.triggerButton as GestureTriggerButton)
      ? raw.triggerButton as GestureTriggerButton
      : defaultSettings.mouseGestures.triggerButton,
    minDistance: integerInRange(raw.minDistance, defaultSettings.mouseGestures.minDistance, 12, 240),
    sensitivity: integerInRange(raw.sensitivity, defaultSettings.mouseGestures.sensitivity, 35, 95),
    showTrail: booleanValue(raw.showTrail, defaultSettings.mouseGestures.showTrail),
    fullscreenPause: booleanValue(raw.fullscreenPause, defaultSettings.mouseGestures.fullscreenPause),
    pausedApps: stringList(raw.pausedApps),
    gestures: [...builtinGestures, ...customGestures]
  };
}

function normalizeGesture(value: unknown, fallback?: GestureTemplate, sourceSchemaVersion = 8): GestureTemplate | null {
  if (!value || typeof value !== "object") return fallback ? cloneGesture(fallback) : null;
  const raw = value as Partial<GestureTemplate>;
  const id = optionalString(raw.id) ?? fallback?.id;
  const name = id ? migrateGestureName(id, optionalString(raw.name), sourceSchemaVersion) : undefined;
  if (!id || (!fallback && !name)) return null;
  const samples = Array.isArray(raw.samples)
    ? raw.samples.map(normalizeGestureSample).filter((sample) => sample.length >= 2).slice(0, 8)
    : [];
  return {
    id: id.slice(0, 80),
    ...(name ? { name: Array.from(name).slice(0, 40).join("") } : {}),
    enabled: booleanValue(raw.enabled, fallback?.enabled ?? true),
    builtin: !!fallback,
    mode: fallback?.mode ?? (raw.mode === "region-screenshot" ? "region-screenshot" : "action"),
    action: normalizeAction(raw.action ?? fallback?.action),
    modifierActions: normalizeModifierActions(raw.modifierActions),
    samples: samples.length ? samples : fallback ? fallback.samples.map((sample) => sample.map((point) => ({ ...point }))) : []
  };
}

function normalizeGestureSample(value: unknown): GesturePoint[] {
  if (!Array.isArray(value)) return [];
  const points = value
    .filter((point) => point && typeof point === "object")
    .map((point) => point as Partial<GesturePoint>)
    .filter((point) => Number.isFinite(point.x) && Number.isFinite(point.y))
    .map((point) => ({ x: Math.min(1, Math.max(0, Number(point.x))), y: Math.min(1, Math.max(0, Number(point.y))) }));
  return resampleGesture(points);
}

function normalizeModifierActions(value: unknown): ModifierAction[] {
  if (!Array.isArray(value)) return [];
  const output: ModifierAction[] = [];
  for (const item of value) {
    if (!item || typeof item !== "object") continue;
    const raw = item as Partial<ModifierAction>;
    const modifiers = modifierList(raw.modifiers);
    if (!modifiers.length || output.some((existing) => sameModifiers(existing.modifiers, modifiers))) continue;
    output.push({ modifiers, action: normalizeAction(raw.action) });
    if (output.length >= 15) break;
  }
  return output;
}

function modifierList(value: unknown, fallback: ModifierKey[] = []): ModifierKey[] {
  if (!Array.isArray(value)) return [...fallback];
  const normalized = modifierKeys.filter((key) => value.includes(key));
  return normalized.length ? normalized : [...fallback];
}

function sameModifiers(left: ModifierKey[], right: ModifierKey[]): boolean {
  return left.length === right.length && left.every((key, index) => key === right[index]);
}

function normalizeWindowDrag(value: unknown): AppSettings["windowDrag"] {
  const raw = value && typeof value === "object" ? value as Partial<AppSettings["windowDrag"]> : {};
  const moveButton = mouseButtons.includes(raw.moveButton as MouseButton) ? raw.moveButton as MouseButton : defaultSettings.windowDrag.moveButton;
  const resizeButton = mouseButtons.includes(raw.resizeButton as MouseButton) ? raw.resizeButton as MouseButton : defaultSettings.windowDrag.resizeButton;
  return {
    enabled: booleanValue(raw.enabled, defaultSettings.windowDrag.enabled),
    updateHz: integerInRange(raw.updateHz, 120, 30, 240),
    moveModifiers: modifierList(raw.moveModifiers, defaultSettings.windowDrag.moveModifiers),
    resizeModifiers: modifierList(raw.resizeModifiers, defaultSettings.windowDrag.resizeModifiers),
    moveButton,
    resizeButton,
    pausedApps: stringList(raw.pausedApps)
  };
}

function normalizeTaskbarAppearance(value: unknown): AppSettings["taskbarAppearance"] {
  const raw = value && typeof value === "object" ? value as Partial<AppSettings["taskbarAppearance"]> : {};
  const modes = ["transparent", "acrylic", "solid"] as const;
  const mode = modes.includes(raw.mode as (typeof modes)[number])
    ? raw.mode as AppSettings["taskbarAppearance"]["mode"]
    : defaultSettings.taskbarAppearance.mode;
  return {
    enabled: booleanValue(raw.enabled, defaultSettings.taskbarAppearance.enabled),
    mode,
    opacity: integerInRange(raw.opacity, defaultSettings.taskbarAppearance.opacity, 0, 100),
    tint: typeof raw.tint === "string" && /^#[0-9a-f]{6}$/i.test(raw.tint.trim())
      ? raw.tint.trim().toUpperCase()
      : defaultSettings.taskbarAppearance.tint,
    showBorder: booleanValue(raw.showBorder, defaultSettings.taskbarAppearance.showBorder)
  };
}

function normalizeOcr(value: unknown): AppSettings["ocr"] {
  const raw = value && typeof value === "object" ? value as Partial<AppSettings["ocr"]> : {};
  return {
    language: ocrLanguages.includes(raw.language as OcrLanguage) ? raw.language as OcrLanguage : defaultSettings.ocr.language,
    screenshotResult: screenshotResultModes.includes(raw.screenshotResult as ScreenshotResultMode)
      ? raw.screenshotResult as ScreenshotResultMode
      : defaultSettings.ocr.screenshotResult,
    pinOffset: typeof raw.pinOffset === "boolean" ? raw.pinOffset : defaultSettings.ocr.pinOffset
  };
}

function integerInRange(value: unknown, fallback: number, min: number, max: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, Math.trunc(value)));
}

function booleanValue(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function optionalString(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const normalized = value.trim();
  return normalized || undefined;
}

function stringList(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return [
    ...new Set(
      value
        .filter((item): item is string => typeof item === "string")
        .map((item) => item.trim())
        .filter(Boolean)
    )
  ];
}

function edgeList(value: unknown): AppSettings["edgeHide"]["edges"] {
  if (!Array.isArray(value)) return [...defaultSettings.edgeHide.edges];
  const allowed = new Set(["left", "top", "right", "bottom"]);
  return [
    ...new Set(
      value.filter(
        (item): item is AppSettings["edgeHide"]["edges"][number] =>
          typeof item === "string" && allowed.has(item)
      )
    )
  ];
}

function normalizeEdgeHideProfiles(value: unknown): AppSettings["edgeHide"]["monitorProfiles"] {
  if (!Array.isArray(value)) return [];
  return value
    .filter((profile) => typeof profile?.monitorId === "string" && profile.monitorId.trim())
    .map((profile) => ({
      monitorId: profile.monitorId.trim(),
      edges: edgeList(profile.edges)
    }));
}

export function saveSettings(settings: AppSettings): Promise<void> {
  const compact = compactSettingsForStorage(settings);
  const serialized = JSON.stringify(compact);
  const bytes = new TextEncoder().encode(serialized).byteLength;
  if (bytes > MAX_SETTINGS_STORAGE_BYTES) {
    throw new Error(`配置大小 ${(bytes / 1024).toFixed(0)} KB 超过安全上限`);
  }

  const host = getOptionalHostBridge();
  if (host) {
    return host.saveSettings(compact);
  }

  try {
    localStorage.setItem(SETTINGS_KEY, serialized);
  } catch (error) {
    throw new Error(`配置保存失败：${error instanceof Error ? error.message : String(error)}`);
  }
  return Promise.resolve();
}

function compactSettingsForStorage(settings: AppSettings): AppSettings {
  const compact = cloneSettings(settings);
  for (const gesture of compact.mouseGestures.gestures) {
    for (const sample of gesture.samples) {
      for (const point of sample) {
        point.x = roundCoordinate(point.x);
        point.y = roundCoordinate(point.y);
      }
    }
  }
  return compact;
}

function roundCoordinate(value: number): number {
  return Math.round(value * 10_000) / 10_000;
}

function loadStoredSettings(): AppSettings | null {
  const initial = getOptionalHostBridge()?.getInitialSettings();
  if (initial && typeof initial === "object") return initial as AppSettings;

  const fallback = localStorage.getItem(SETTINGS_KEY);
  if (!fallback) {
    return null;
  }

  try {
    return JSON.parse(fallback) as AppSettings;
  } catch {
    return null;
  }
}

function cloneSettings(settings: AppSettings): AppSettings {
  return {
    ...settings,
    pausedApps: [...settings.pausedApps],
    hotzones: settings.hotzones.map(cloneHotzone),
    monitorProfiles: settings.monitorProfiles.map((profile) => ({
      monitorId: profile.monitorId,
      hotzones: profile.hotzones.map(cloneHotzone)
    })),
    edgeHide: {
      ...settings.edgeHide,
      edges: [...settings.edgeHide.edges],
      monitorProfiles: settings.edgeHide.monitorProfiles.map((profile) => ({
        monitorId: profile.monitorId,
        edges: [...profile.edges]
      })),
      excludedApps: [...settings.edgeHide.excludedApps]
    },
    windowDrag: {
      ...settings.windowDrag,
      moveModifiers: [...settings.windowDrag.moveModifiers],
      resizeModifiers: [...settings.windowDrag.resizeModifiers],
      pausedApps: [...settings.windowDrag.pausedApps]
    },
    topmostPin: { ...settings.topmostPin },
    ocr: { ...settings.ocr },
    mouseGestures: {
      ...settings.mouseGestures,
      pausedApps: [...settings.mouseGestures.pausedApps],
      gestures: settings.mouseGestures.gestures.map(cloneGesture)
    }
  };
}

function cloneGesture(gesture: GestureTemplate): GestureTemplate {
  return {
    ...gesture,
    action: { ...gesture.action },
    modifierActions: (gesture.modifierActions ?? []).map(cloneModifierAction),
    samples: gesture.samples.map((sample) => sample.map((point) => ({ ...point })))
  };
}

function builtinGesture(
  id: string,
  action: HotzoneAction,
  points: [number, number][]
): GestureTemplate {
  return {
    id,
    enabled: true,
    builtin: true,
    mode: "action",
    action,
    modifierActions: [],
    samples: [points.map(([x, y]) => ({ x, y }))]
  };
}

function circleSample(): [number, number][] {
  return Array.from({ length: 25 }, (_, index) => {
    const angle = (Math.PI * 2 * index) / 24 - Math.PI / 2;
    return [0.5 + Math.cos(angle) * 0.38, 0.5 + Math.sin(angle) * 0.38];
  });
}

function cloneHotzone(hotzone: AppSettings["hotzones"][number]): AppSettings["hotzones"][number] {
  return {
    ...hotzone,
    ...(hotzone.geometry ? { geometry: { ...hotzone.geometry } } : {}),
    actions: hotzone.actions.map((item) => ({
      trigger: item.trigger,
      action: { ...item.action },
      modifierActions: (item.modifierActions ?? []).map(cloneModifierAction),
      cooldownMs: item.cooldownMs,
      hoverDelayMs: item.hoverDelayMs
    }))
  };
}

function cloneModifierAction(item: ModifierAction): ModifierAction {
  return {
    modifiers: [...item.modifiers],
    action: { ...item.action }
  };
}
