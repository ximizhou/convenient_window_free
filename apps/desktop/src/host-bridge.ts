export interface HostBridge {
  readonly kind: "desktop";
  startHelper(): Promise<{
    ok: boolean;
    alreadyRunning?: boolean;
    error?: string;
    helperPath?: string;
    dataDir?: string;
  }>;
  stopHelper(): Promise<{ ok: boolean; error?: string }>;
  getHelperToken(): string | null;
  getHelperState(): HelperInstallState;
  openExternal(url: string): { ok: boolean; error?: string };
  redirect(code: string): { ok: boolean; error?: string };
  diagnostics(): Promise<DesktopDiagnostics>;
  getInitialSettings(): unknown | null;
  saveSettings(settings: unknown): Promise<void>;
  importSettings(): Promise<unknown | null>;
  exportSettings(settings: unknown): Promise<boolean>;
  isAutostartEnabled(): Promise<boolean>;
  setAutostartEnabled(enabled: boolean): Promise<void>;
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
