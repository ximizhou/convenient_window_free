export {};

declare global {
  interface HelperOperationResult {
    ok: boolean;
    error?: string;
    cancelled?: boolean;
    alreadyInstalled?: boolean;
    version?: string;
  }

  interface HelperInstallState {
    installed: boolean;
    development: boolean;
    version: string;
    bytes: number;
    repository?: string;
    release?: string;
    installDir?: string;
    error?: string;
  }

  interface HelperInstallProgress {
    phase: "preparing" | "downloading" | "verifying" | "installing" | "ready" | "error";
    message: string;
    detail?: string;
    currentFile?: string;
    percent: number;
    completed: number;
    total: number;
    bytesReceived: number;
    bytesTotal: number;
    bytesPerSecond?: number;
    secondsRemaining?: number;
    attempt?: number;
    maxAttempts?: number;
    transport?: "system-proxy" | "direct";
    browserFallback?: boolean;
  }
}
