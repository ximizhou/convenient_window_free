import { describe, expect, it } from "vitest";
import { buildDiagnosticReport, summarizeDiagnosticError } from "./diagnostics";

describe("diagnostic report privacy and useful evidence", () => {
  it("exports only allowlisted fields, without raw host data or display identities", () => {
    const report = buildDiagnosticReport({
      host: "utools", expectedHelperVersion: "0.6.9", actualHelperVersion: "0.6.8", protocolVersion: 7, schemaVersion: 9,
      connection: "connected", phase: "running",
      rawDiagnostics: { pluginVersion: "0.6.9", installed: true, development: false, helperPath: "C:\\Users\\私密用户\\工具.exe", authToken: "secret-token", logTail: ["private log"], settings: { command: "secret command" } },
      displays: [{ id: "device-serial-private", name: "Private screen", x: -1920, y: 0, width: 1920, height: 1080, scaleFactor: 1.25, primary: false }]
    }, new Date("2026-01-02T03:04:05Z"));
    expect(report.versions).toEqual({ app: "0.6.9", expectedHelper: "0.6.9", actualHelper: "0.6.8", protocol: 7, schema: 9 });
    expect(report.displays).toEqual([{ index: 1, x: -1920, y: 0, width: 1920, height: 1080, scaleFactor: 1.25, primary: false }]);
    expect(report.runtime.installed).toBe(true);
    const json = JSON.stringify(report);
    for (const privateValue of ["私密用户", "secret-token", "private log", "secret command", "device-serial-private", "Private screen", "helperPath"]) expect(json).not.toContain(privateValue);
  });

  it.each([
    ['EACCES opening "C:\\Users\\张三\\Private Files\\配置.json" --token=secret', "access-denied", "EACCES"],
    ["download failed /home/alice/private config.json https://alice:password@example.test/path?token=token-secret&key=key-secret", "network", null],
    ["无法写入 \\\\private-server\\private-share\\文件.txt bearer supersecret", "storage", null],
    ["connection timeout file:///Users/alice/data.txt api_key = private-api-key", "timeout", null],
    ["HELPER_INSTANCE_CONFLICT PID 12345 private-app.exe window title = payroll", "instance-conflict", null],
    ["unknown --password 'secret phrase' --command 'private shell command'", "unknown", null],
    ["启动失败 os error 5, C:/Users/Alice/Secret Folder/helper.exe", "access-denied", "os-error-5"]
  ])("classifies an error without copying its private content: %s", (message, category, systemCode) => {
    expect(summarizeDiagnosticError(message, "host")).toEqual({ source: "host", category, systemCode });
  });

  it("never treats the expected installed version as the actual running version", () => {
    const report = buildDiagnosticReport({ host: "utools", expectedHelperVersion: "0.6.9", rawDiagnostics: { helperVersion: "0.6.9" } });
    expect(report.versions.actualHelper).toBeNull();
  });

  it("summarizes the helper's actual signed rectangle format without its device ID", () => {
    const report = buildDiagnosticReport({ host: "desktop", displays: [{ id: "private-device", bounds: { left: -1920, top: -200, right: 0, bottom: 880 }, primary: true }] });
    expect(report.displays[0]).toEqual({ index: 1, x: -1920, y: -200, width: 1920, height: 1080, scaleFactor: null, primary: true });
  });

  it("handles missing data and errors without serializing arbitrary objects", () => {
    const raw = { toJSON: () => { throw new Error("must not serialize raw data"); }, lastError: new Error("ENOSPC /private/data"), helperRunning: false, lastExitCode: 5 };
    const report = buildDiagnosticReport({ host: "desktop", rawDiagnostics: raw, error: { message: "untrusted-object-secret" } });
    expect(report.runtime.helperRunning).toBe(false);
    expect(report.runtime.lastExitCode).toBe(5);
    expect(report.errors).toEqual([{ source: "host", category: "storage", systemCode: "ENOSPC" }]);
    expect(() => JSON.stringify(report)).not.toThrow();
  });

  it("rejects invalid typed fields and bounds report growth", () => {
    const report = buildDiagnosticReport({ host: "utools", appVersion: "/private/path", actualHelperVersion: "1.2.3 --token=private", protocolVersion: Infinity, schemaVersion: "private", displays: Array.from({ length: 50 }, () => ({ x: "private", y: Infinity, width: -10, height: 900, primary: "private" })) });
    expect(report.versions).toEqual({ app: null, expectedHelper: null, actualHelper: null, protocol: null, schema: null });
    expect(report.displays).toHaveLength(32);
    expect(report.displays[0]).toEqual({ index: 1, x: null, y: null, width: null, height: 900, scaleFactor: null, primary: null });
  });
});
