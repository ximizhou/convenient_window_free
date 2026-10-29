import type { AppSettings, DisplayInfo } from "./types";

export interface OfflineMonitorProfile {
  monitorId: string;
  hotzoneCount: number;
  edgeCount: number;
  hasHotzones: boolean;
  hasEdgeHide: boolean;
}

export function canManageMonitorProfiles(displays: readonly DisplayInfo[], ready: boolean): boolean {
  return ready && displays.length > 0 && displays.every((display) => typeof display.id === "string" && display.id.length > 0);
}

export function listOfflineMonitorProfiles(settings: AppSettings, displays: readonly DisplayInfo[], ready: boolean): OfflineMonitorProfile[] {
  if (!canManageMonitorProfiles(displays, ready)) return [];
  // Both stable and legacy identities protect connected screens before migration.
  const online = new Set(displays.flatMap((display) => [display.id, display.legacyId].filter((id): id is string => Boolean(id))));
  const profiles = new Map<string, OfflineMonitorProfile>();
  const get = (monitorId: string) => {
    if (!monitorId || online.has(monitorId)) return null;
    if (!profiles.has(monitorId)) profiles.set(monitorId, { monitorId, hotzoneCount: 0, edgeCount: 0, hasHotzones: false, hasEdgeHide: false });
    return profiles.get(monitorId)!;
  };
  for (const profile of settings.monitorProfiles) {
    const item = get(profile.monitorId);
    if (item) { item.hotzoneCount += profile.hotzones.length; item.hasHotzones = true; }
  }
  for (const profile of settings.edgeHide.monitorProfiles) {
    const item = get(profile.monitorId);
    if (item) { item.edgeCount += profile.edges.length; item.hasEdgeHide = true; }
  }
  return [...profiles.values()];
}

// Recheck live topology at confirmation time. A screen can reconnect while its
// confirmation row is open; stale UI must never remove that screen's settings.
export function removeOfflineMonitorProfile(settings: AppSettings, displays: readonly DisplayInfo[], ready: boolean, monitorId: string): AppSettings | null {
  if (!listOfflineMonitorProfiles(settings, displays, ready).some((profile) => profile.monitorId === monitorId)) return null;
  return {
    ...settings,
    monitorProfiles: settings.monitorProfiles.filter((profile) => profile.monitorId !== monitorId),
    edgeHide: { ...settings.edgeHide, monitorProfiles: settings.edgeHide.monitorProfiles.filter((profile) => profile.monitorId !== monitorId) }
  };
}

export const monitorProfileLabels = {
  zh: {
    title: "离线显示器配置", description: "断开的显示器会保留配置，重新连接后继续使用。", waiting: "连接后台助手并检测到显示器后，可查看离线配置。", empty: "没有离线显示器配置。", screen: "离线显示器", hotzones: "个热区", edges: "个贴边方向", remove: "删除配置", confirm: "确认删除", cancel: "取消", confirmation: "删除此显示器保存的热区和贴边配置？重新连接后将使用全局配置。", failed: "删除未完成，请重试。", changed: "显示器状态已变化，请重新查看列表。", identity: "查看显示器标识"
  },
  en: {
    title: "Offline display profiles", description: "Settings are kept when a display disconnects and reused when it returns.", waiting: "Connect the helper and wait for display detection to manage offline profiles.", empty: "No offline display profiles.", screen: "Offline display", hotzones: "hot zones", edges: "edge directions", remove: "Remove profile", confirm: "Confirm removal", cancel: "Cancel", confirmation: "Remove this display's saved hot zones and edge settings? It will use global settings when reconnected.", failed: "The profile was not removed. Try again.", changed: "Display state changed. Review the list again.", identity: "View display identifier"
  }
} as const;
