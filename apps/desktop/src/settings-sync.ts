import { normalizeSettings } from "./settings-store";
import type { AppSettings } from "./types";

export interface PreparedSettingsUpdate {
  editable: AppSettings;
  normalized: AppSettings;
}

export function prepareSettingsUpdate(settings: AppSettings): PreparedSettingsUpdate {
  const normalized = normalizeSettings(settings);
  return { editable: { ...settings }, normalized };
}
