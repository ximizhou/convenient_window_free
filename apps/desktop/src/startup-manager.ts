export type StartupSource = "userRun" | "machineRun" | "machineRun32" | "userStartup" | "commonStartup";

export interface StartupItem {
  id: string;
  name: string;
  command: string;
  source: StartupSource;
  enabled: boolean | null;
  editable: boolean;
  reason?: string;
  revision: string;
}

export type StartupRequest =
  | { action: "list" }
  | { action: "setEnabled"; id: string; revision: string; enabled: boolean }
  | { action: "add"; executablePath: string; name: string };

export interface StartupResponse {
  ok: boolean;
  version: 1;
  items?: StartupItem[];
  warnings?: string[];
  error?: { code: string; message?: string };
}

export type StartupFilter = "all" | "enabled" | "disabled";

const sources = new Set<StartupSource>(["userRun", "machineRun", "machineRun32", "userStartup", "commonStartup"]);

/** Reject incomplete replies rather than reporting a mutation as successful. */
export function readStartupItems(response: StartupResponse): StartupItem[] | null {
  if (!response || response.version !== 1 || !response.ok || !Array.isArray(response.items)) return null;
  const ids = new Set<string>();
  for (const item of response.items) {
    if (!item || typeof item.id !== "string" || !item.id || ids.has(item.id)
      || typeof item.name !== "string" || typeof item.command !== "string"
      || !sources.has(item.source) || ![true, false, null].includes(item.enabled)
      || typeof item.editable !== "boolean" || typeof item.revision !== "string" || !item.revision
      || (item.reason !== undefined && typeof item.reason !== "string")) return null;
    ids.add(item.id);
  }
  return response.items.map(item => ({ ...item }));
}

export function filterStartupItems(items: StartupItem[], query: string, filter: StartupFilter): StartupItem[] {
  const needle = query.trim().toLocaleLowerCase();
  return items.filter(item => (filter === "all" || item.enabled === (filter === "enabled"))
    && (!needle || (item.name + " " + item.command).toLocaleLowerCase().includes(needle)));
}

export function startupNameFromPath(filename: string): string {
  return filename.split(/[\\/]/).at(-1)?.replace(/\.exe$/i, "").slice(0, 80) ?? "";
}

export type StartupErrorKey = "unsupported" | "needsHelper" | "ordinaryUser" | "identityUnavailable" | "accessDenied" | "changed" | "invalidPath" | "invalidName" | "duplicate" | "missing" | "unavailable" | "failed";

export function startupErrorKey(code?: string): StartupErrorKey {
  switch (code?.replace(/[^a-z]/gi, "").toLowerCase()) {
    case "unsupported": case "unsupportedplatform": return "unsupported";
    case "needshelper": case "helperunavailable": return "needsHelper";
    case "ordinaryuserrequired": return "ordinaryUser";
    case "identityunavailable": return "identityUnavailable";
    case "accessdenied": case "permissiondenied": case "readonly": return "accessDenied";
    case "conflict": case "stale": case "revisionmismatch": case "entrychanged": return "changed";
    case "invalidpath": case "invalidexecutable": case "invalidinput": return "invalidPath";
    case "invalidname": return "invalidName";
    case "duplicate": case "alreadyexists": return "duplicate";
    case "notfound": case "missing": return "missing";
    case "unavailable": return "unavailable";
    default: return "failed";
  }
}
