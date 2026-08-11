import type { AppSettings, DisplayInfo } from "./types";

export function migrateMonitorProfileIds(settings: AppSettings, displays: DisplayInfo[]): boolean {
  const aliases = new Map(
    displays
      .filter((display) => display.legacyId && display.legacyId !== display.id)
      .map((display) => [display.legacyId!, display.id])
  );
  if (!aliases.size) return false;

  const hotzones = migrateProfiles(settings.monitorProfiles, aliases);
  const edgeHide = migrateProfiles(settings.edgeHide.monitorProfiles, aliases);
  if (hotzones.changed) settings.monitorProfiles = hotzones.profiles;
  if (edgeHide.changed) settings.edgeHide.monitorProfiles = edgeHide.profiles;
  return hotzones.changed || edgeHide.changed;
}

function migrateProfiles<T extends { monitorId: string }>(
  profiles: T[],
  aliases: Map<string, string>
): { profiles: T[]; changed: boolean } {
  let changed = false;
  const output: T[] = [];
  const stableIds = new Set(profiles.map((profile) => profile.monitorId));
  for (const profile of profiles) {
    const stableId = aliases.get(profile.monitorId);
    if (!stableId) {
      output.push(profile);
      continue;
    }
    changed = true;
    if (stableIds.has(stableId) || output.some((item) => item.monitorId === stableId)) continue;
    output.push({ ...profile, monitorId: stableId });
    stableIds.add(stableId);
  }
  return { profiles: output, changed };
}
