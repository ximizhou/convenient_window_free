export type HotzoneId =
  | "top-left"
  | "top"
  | "top-right"
  | "right"
  | "bottom-right"
  | "bottom"
  | "bottom-left"
  | "left";

export type TriggerKind =
  | "hover"
  | "left-click"
  | "right-click"
  | "wheel-up"
  | "wheel-down"
  | "slide-forward"
  | "slide-backward";

export type KnownActionKind =
  | "none"
  | "shortcut"
  | "show-desktop"
  | "toggle-window-topmost"
  | "lock-screen"
  | "volume-adjust"
  | "open-command";

export type ActionKind = KnownActionKind | (string & {});

export type Edge = "left" | "top" | "right" | "bottom";
export type ModifierKey = "ctrl" | "alt" | "shift" | "win";
export type MouseButton = "left" | "right" | "middle" | "x1" | "x2";
export type OcrLanguage = "auto" | "zh-Hans" | "en";
export type ScreenshotResultMode = "pin" | "copy-text" | "pin-and-copy";

export interface HotzoneAction {
  kind: ActionKind;
  value?: string;
}

export interface ModifierAction {
  modifiers: ModifierKey[];
  action: HotzoneAction;
}

export interface HotzoneSetting {
  id: HotzoneId;
  enabled: boolean;
  actions: TriggerAction[];
}

export interface TriggerAction {
  trigger: TriggerKind;
  action: HotzoneAction;
  modifierActions?: ModifierAction[];
  cooldownMs?: number;
  hoverDelayMs?: number;
}

export interface MonitorProfile {
  monitorId: string;
  hotzones: HotzoneSetting[];
}

export interface DisplayInfo {
  id: string;
  legacyId?: string;
  primary: boolean;
  bounds: { left: number; top: number; right: number; bottom: number };
  workArea: { left: number; top: number; right: number; bottom: number };
}

export interface AppSettings {
  schemaVersion: number;
  enabled: boolean;
  hotzonesEnabled: boolean;
  edgeSize: number;
  hoverDelayMs: number;
  pollIntervalMs: number;
  actionCooldownMs: number;
  pausedApps: string[];
  hotzones: HotzoneSetting[];
  monitorProfiles: MonitorProfile[];
  edgeHide: EdgeHideSettings;
  mouseGestures: MouseGestureSettings;
  windowDrag: WindowDragSettings;
  topmostPin: TopmostPinSettings;
  ocr: OcrSettings;
}

export type GestureTriggerButton = "right" | "middle" | "x1" | "x2";
export type GestureMode = "action" | "region-screenshot";

export interface GesturePoint {
  x: number;
  y: number;
}

export interface GestureTemplate {
  id: string;
  name: string;
  enabled: boolean;
  builtin: boolean;
  mode: GestureMode;
  action: HotzoneAction;
  modifierActions?: ModifierAction[];
  samples: GesturePoint[][];
}

export interface MouseGestureSettings {
  enabled: boolean;
  triggerButton: GestureTriggerButton;
  minDistance: number;
  sensitivity: number;
  showTrail: boolean;
  fullscreenPause: boolean;
  pausedApps: string[];
  gestures: GestureTemplate[];
}

export interface EdgeHideSettings {
  enabled: boolean;
  showPreview: boolean;
  showRestoreHint: boolean;
  keepExpandedWhenForeground: boolean;
  edges: Edge[];
  monitorProfiles: EdgeHideMonitorProfile[];
  stripSize: number;
  triggerDistance: number;
  distanceTriggerEnabled: boolean;
  ratioTriggerEnabled: boolean;
  triggerRatio: number;
  collapseDelayMs: number;
  restoreDelayMs: number;
  excludedApps: string[];
}

export interface EdgeHideMonitorProfile {
  monitorId: string;
  edges: Edge[];
}

export interface WindowDragSettings {
  enabled: boolean;
  moveModifiers: ModifierKey[];
  resizeModifiers: ModifierKey[];
  moveButton: MouseButton;
  resizeButton: MouseButton;
}

export interface TopmostPinSettings {
  enabled: boolean;
}

export interface OcrSettings {
  language: OcrLanguage;
  screenshotResult: ScreenshotResultMode;
}

export interface HelperMessage<T = unknown> {
  id: string;
  type: string;
  time: number;
  data: T;
}

export type HelperStatus = "disconnected" | "connecting" | "connected";
