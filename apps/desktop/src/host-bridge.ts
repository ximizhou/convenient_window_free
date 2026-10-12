export interface AdminStartupState {
  enabled: boolean | null;
  error?: string;
}

export interface HostBridge {
  readonly kind: "desktop";
  setLanguage?(language: import("./i18n").Language): Promise<void>;
  startHelper(): Promise<{
    ok: boolean;
    alreadyRunning?: boolean;
    error?: string;
    helperPath?: string;
    dataDir?: string;
    warning?: string;
  }>;
  stopHelper(): Promise<{ ok: boolean; error?: string }>;
  getPrivilegeSupport?(): { supported: boolean };
  getPrivilegeState?(): { supported: boolean; elevated: boolean | null };
  setHelperElevation?(elevated: boolean): Promise<{ ok: boolean; elevated: boolean | null; warning?: string; error?: string }>;
  getAdminStartup?(): Promise<AdminStartupState>;
  setAdminStartup?(enabled: boolean): Promise<AdminStartupState>;
  getStartup?(): Promise<{ enabled: boolean | null; error?: string }>;
  setStartup?(enabled: boolean): Promise<{ enabled: boolean | null; error?: string }>;
  onStartupChanged?(handler: (error?: string) => void): Promise<() => void>;
  getHelperToken(): string | null;
  getHelperState(): HelperInstallState;
  openExternal(url: string): { ok: boolean; error?: string };
  redirect(code: string): { ok: boolean; error?: string };
  diagnostics(): Promise<DesktopDiagnostics>;
  getInitialSettings(): unknown | null;
  saveSettings(settings: unknown): Promise<void>;
  importSettings(): Promise<unknown | null>;
  exportSettings(settings: unknown): Promise<boolean>;
}

let activeBridge: HostBridge | null = null;

export function configureHostBridge(bridge: HostBridge): void {
  activeBridge = bridge;
}

export function getHostBridge(): HostBridge {
  if (!activeBridge) throw new Error("Host bridge has not been configured");
  return activeBridge;
}

export function getOptionalHostBridge(): HostBridge | null {
  return activeBridge;
}

export interface DesktopDiagnostics {
  appDataDir: string;
  settingsPath: string;
  helperDataDir: string;
  helperPath: string;
  helperRunning: boolean;
  helperPayloadBytes: number;
  lastExitCode: number | null;
  lastError: string | null;
  logTail: string[];
}
