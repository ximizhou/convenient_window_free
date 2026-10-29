export type DiagnosticHost = "utools" | "desktop";
export type DiagnosticConnection = "connected" | "connecting" | "disconnected";
export type DiagnosticPhase = "idle" | "starting" | "stopping" | "installing" | "recovering" | "running" | "error";
export type DiagnosticErrorCategory = "instance-conflict" | "access-denied" | "integrity" | "network" | "timeout" | "protocol" | "storage" | "unknown";

export interface DiagnosticInput {
  host: DiagnosticHost;
  appVersion?: unknown;
  expectedHelperVersion?: unknown;
  actualHelperVersion?: unknown;
  protocolVersion?: unknown;
  schemaVersion?: unknown;
  connection?: DiagnosticConnection;
  phase?: DiagnosticPhase;
  displays?: readonly unknown[];
  rawDiagnostics?: unknown;
  error?: unknown;
}

export interface DiagnosticError {
  source: "interface" | "host" | "preload" | "manifest" | "helper";
  category: DiagnosticErrorCategory;
  systemCode: string | null;
}

export interface DiagnosticReport {
  formatVersion: 1;
  generatedAt: string;
  host: DiagnosticHost;
  versions: {
    app: string | null;
    expectedHelper: string | null;
    actualHelper: string | null;
    protocol: number | null;
    schema: number | null;
  };
  runtime: {
    connection: DiagnosticConnection;
    phase: DiagnosticPhase;
    installed: boolean | null;
    development: boolean | null;
    helperRunning: boolean | null;
    lastExitCode: number | null;
  };
  displays: Array<{ index: number; x: number | null; y: number | null; width: number | null; height: number | null; scaleFactor: number | null; primary: boolean | null }>;
  errors: DiagnosticError[];
}

function record(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function version(value: unknown): string | null {
  return typeof value === "string" && /^(?:development|\d{1,4}\.\d{1,4}\.\d{1,4}(?:-[a-zA-Z0-9.-]{1,32})?)$/.test(value) ? value : null;
}

function number(value: unknown, min: number, max: number, integer = true): number | null {
  return typeof value === "number" && Number.isFinite(value) && value >= min && value <= max && (!integer || Number.isInteger(value)) ? value : null;
}

function boolean(value: unknown): boolean | null { return typeof value === "boolean" ? value : null; }

// Diagnostic errors are intentionally classified, never copied. Free-form errors can
// contain commands, window titles, paths or credentials that a path-only regex misses.
export function summarizeDiagnosticError(value: unknown, source: DiagnosticError["source"]): DiagnosticError | null {
  const text = value instanceof Error ? value.message : typeof value === "string" ? value : "";
  if (!text.trim()) return null;
  const sample = text.slice(0, 8192);
  const category: DiagnosticErrorCategory = /HELPER_INSTANCE_CONFLICT|另一个.*宿主|端口.*占用|already running|address.*in use|EADDRINUSE/i.test(sample) ? "instance-conflict"
    : /access.*denied|permission.*denied|拒绝访问|权限不足|EPERM|EACCES|os error 5\b/i.test(sample) ? "access-denied"
    : /sha-?256|hash.*mismatch|integrity|哈希|完整性|校验失败/i.test(sample) ? "integrity"
    : /protocol|schema|协议|版本不兼容/i.test(sample) ? "protocol"
    : /timed?\s*out|timeout|超时|ETIMEDOUT/i.test(sample) ? "timeout"
    : /ENOTFOUND|ECONNRESET|ECONNREFUSED|network|download|proxy|网络|下载|代理/i.test(sample) ? "network"
    : /ENOSPC|EROFS|写入|保存失败|storage|disk|磁盘/i.test(sample) ? "storage" : "unknown";
  const nodeCode = sample.match(/\b(EACCES|EPERM|EADDRINUSE|ETIMEDOUT|ENOTFOUND|ECONNRESET|ECONNREFUSED|ENOSPC|EROFS|ENOENT)\b/)?.[1];
  const windowsCode = sample.match(/\bos error (\d{1,5})\b/i)?.[1];
  return { source, category, systemCode: nodeCode ?? (windowsCode ? `os-error-${windowsCode}` : null) };
}

export function buildDiagnosticReport(input: DiagnosticInput, now = new Date()): DiagnosticReport {
  const raw = record(input.rawDiagnostics);
  const errors = [
    summarizeDiagnosticError(input.error, "interface"),
    summarizeDiagnosticError(raw.lastError, "host"),
    summarizeDiagnosticError(raw.preloadError, "preload"),
    summarizeDiagnosticError(raw.manifestError, "manifest"),
    summarizeDiagnosticError(raw.helperBackendError, "helper")
  ].filter((error): error is DiagnosticError => error !== null);
  const connection = ["connected", "connecting", "disconnected"].includes(input.connection ?? "") ? input.connection! : "disconnected";
  const phase = ["idle", "starting", "stopping", "installing", "recovering", "running", "error"].includes(input.phase ?? "") ? input.phase! : "idle";
  return {
    formatVersion: 1,
    generatedAt: now.toISOString(),
    host: input.host === "desktop" ? "desktop" : "utools",
    versions: {
      app: version(input.appVersion ?? raw.pluginVersion),
      expectedHelper: version(input.expectedHelperVersion),
      actualHelper: version(input.actualHelperVersion),
      protocol: number(input.protocolVersion, 1, 999),
      schema: number(input.schemaVersion, 1, 999)
    },
    runtime: {
      connection, phase,
      installed: boolean(raw.installed),
      development: boolean(raw.development),
      helperRunning: boolean(raw.helperRunning),
      lastExitCode: number(raw.lastExitCode, -2147483648, 4294967295)
    },
    displays: (Array.isArray(input.displays) ? input.displays : []).slice(0, 32).map((value, index) => {
      const display = record(value);
      const bounds = record(display.bounds ?? display);
      const x = number(bounds.left ?? bounds.x, -1000000, 1000000);
      const y = number(bounds.top ?? bounds.y, -1000000, 1000000);
      const right = number(bounds.right, -1000000, 1000000);
      const bottom = number(bounds.bottom, -1000000, 1000000);
      return {
        index: index + 1,
        x, y,
        width: number(right !== null && x !== null ? right - x : bounds.width, 1, 100000),
        height: number(bottom !== null && y !== null ? bottom - y : bounds.height, 1, 100000),
        scaleFactor: number(display.scaleFactor, 0.25, 16, false),
        primary: boolean(display.primary)
      };
    }),
    errors
  };
}

export const diagnosticLabels = {
  zh: {
    title: "诊断报告", description: "查看版本、连接状态与错误摘要，方便排查问题。", generate: "生成报告", refresh: "刷新报告", loading: "正在读取…",
    privacy: "报告已移除个人路径、令牌与原始日志，仅保存在本机。分享前可在下方查看。", preview: "查看报告内容", copy: "复制报告", save: "保存文件", copied: "报告已复制", saved: "报告已保存", downloadStarted: "已请求下载报告", failed: "无法生成报告，请重试。", copyFailed: "无法复制，请保存文件。", saveFailed: "无法保存，请复制报告。"
  },
  en: {
    title: "Diagnostics", description: "Review versions, connection state and error summaries to troubleshoot issues.", generate: "Generate report", refresh: "Refresh report", loading: "Reading…",
    privacy: "Personal paths, tokens and raw logs are excluded. The report stays on this device. Review it below before sharing.", preview: "View report contents", copy: "Copy report", save: "Save file", copied: "Report copied", saved: "Report saved", downloadStarted: "Report download requested", failed: "Could not generate the report. Try again.", copyFailed: "Could not copy. Save the report instead.", saveFailed: "Could not save. Copy the report instead."
  }
} as const;
