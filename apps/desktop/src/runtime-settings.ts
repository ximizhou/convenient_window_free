import type { AppSettings, HotzoneAction } from "./types";

const helperActionKinds = new Set([
  "none",
  "shortcut",
  "show-desktop",
  "toggle-window-topmost",
  "lock-screen",
  "volume-adjust",
  "open-command"
]);

export function settingsForHelper(settings: AppSettings): AppSettings {
  const runtime = structuredClone(settings);
  if (!Array.isArray(runtime.hotzones) || !Array.isArray(runtime.monitorProfiles) || !runtime.mouseGestures) {
    return runtime;
  }
  runtime.hotzones.forEach(sanitizeHotzoneActions);
  runtime.monitorProfiles.forEach((profile) => profile.hotzones.forEach(sanitizeHotzoneActions));
  runtime.mouseGestures.gestures.forEach((gesture) => {
    gesture.action = runtimeAction(gesture.action);
    gesture.modifierActions = gesture.modifierActions?.map((variant) => ({
      ...variant,
      action: runtimeAction(variant.action)
    }));
  });
  return runtime;
}

function sanitizeHotzoneActions(zone: AppSettings["hotzones"][number]): void {
  zone.actions = zone.actions.map((slot) => ({
    ...slot,
    action: runtimeAction(slot.action),
    modifierActions: slot.modifierActions?.map((variant) => ({
      ...variant,
      action: runtimeAction(variant.action)
    }))
  }));
}

function runtimeAction(action: HotzoneAction): HotzoneAction {
  return helperActionKinds.has(action.kind) ? action : { kind: "none" };
}
