import { beforeEach, describe, expect, it, vi } from "vitest";
const invoke = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/event", () => ({ listen }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
import { createDesktopHostBridge } from "./desktop-host-bridge";

describe("helper privilege switching", () => {
  beforeEach(() => {
    vi.stubGlobal("navigator", { language: "en-US" });
    invoke.mockReset();
    invoke.mockImplementation(async (command: string) => {
      if (command === "load_config") return null;
      if (command === "desktop_status") return { helperExists: true, helperRunning: true, helperElevated: false, administratorModeSupported: true, token: "old" };
      throw new Error(`Unexpected command: ${command}`);
    });
  });

  it("passes native startup errors to the settings subscriber and preserves unlisten", async () => {
    const bridge = await createDesktopHostBridge();
    const stop = vi.fn();
    listen.mockResolvedValueOnce(stop);
    const handler = vi.fn();
    expect(await bridge.onStartupChanged!(handler)).toBe(stop);
    expect(listen).toHaveBeenLastCalledWith("startup-changed", expect.any(Function));
    const receive = listen.mock.calls.at(-1)![1];
    receive({ payload: "access denied" });
    expect(handler).toHaveBeenLastCalledWith("access denied");
    receive({ payload: null });
    expect(handler).toHaveBeenLastCalledWith(undefined);
  });

  it("replaces the connection token after elevation and after cancellation fallback", async () => {
    const bridge = await createDesktopHostBridge();
    invoke.mockResolvedValueOnce({ token: "elevated-token", elevated: true, warning: null });
    expect(await bridge.setHelperElevation!(true)).toEqual({ ok: true, elevated: true, warning: undefined });
    expect(invoke).toHaveBeenLastCalledWith("set_helper_elevation", { elevated: true });
    expect(bridge.getHelperToken()).toBe("elevated-token");
    expect(bridge.getPrivilegeState!()).toEqual({ supported: true, elevated: true });
    invoke.mockResolvedValueOnce({ token: "ordinary-token", elevated: false, warning: "access denied" });
    expect(await bridge.setHelperElevation!(true)).toEqual({ ok: true, elevated: false, warning: "access denied" });
    expect(bridge.getHelperToken()).toBe("ordinary-token");
  });

  it("automatic recovery uses start_helper without requesting elevation", async () => {
    const bridge = await createDesktopHostBridge();
    invoke.mockResolvedValueOnce({ token: "recovered", elevated: false, helperPath: "helper.exe", alreadyRunning: false });
    expect((await bridge.startHelper()).ok).toBe(true);
    expect(invoke).toHaveBeenLastCalledWith("start_helper");
    expect(bridge.getHelperToken()).toBe("recovered");
    invoke.mockRejectedValueOnce("helper still running");
    expect(await bridge.setHelperElevation!(false)).toEqual({ ok: false, elevated: false, error: "helper still running" });
  });
  it("refreshes the real state when the old elevated helper stops but the replacement fails", async () => {
    invoke.mockImplementationOnce(async () => ({ helperExists: true, helperRunning: true, helperElevated: true, administratorModeSupported: true, token: "elevated-old" }));
    const bridge = await createDesktopHostBridge();
    invoke.mockRejectedValueOnce("replacement failed");
    invoke.mockResolvedValueOnce({ helperRunning: false, helperElevated: false, token: null });
    expect(await bridge.setHelperElevation!(false)).toMatchObject({ ok: false, elevated: false, error: "replacement failed" });
    expect(invoke).toHaveBeenLastCalledWith("desktop_status");
    expect(bridge.getPrivilegeState!()).toMatchObject({ elevated: false });
    expect(bridge.getHelperToken()).toBeNull();
  });

  it("preserves elevation only when the status query confirms the old helper still runs", async () => {
    invoke.mockImplementationOnce(async () => ({ helperExists: true, helperRunning: true, helperElevated: true, administratorModeSupported: true, token: "elevated-old" }));
    const bridge = await createDesktopHostBridge();
    invoke.mockRejectedValueOnce("old helper refused to stop");
    invoke.mockResolvedValueOnce({ helperRunning: true, helperElevated: true, token: "elevated-old" });
    expect(await bridge.setHelperElevation!(false)).toMatchObject({ ok: false, elevated: true });
    expect(invoke).toHaveBeenLastCalledWith("desktop_status");
    expect(bridge.getHelperToken()).toBe("elevated-old");
  });

  it("reports unknown and discards the token when both switching and querying fail", async () => {
    const bridge = await createDesktopHostBridge();
    invoke.mockRejectedValueOnce("switch failed");
    invoke.mockRejectedValueOnce("status unavailable");
    expect(await bridge.setHelperElevation!(true)).toEqual({ ok: false, elevated: null, error: "switch failed" });
    expect(bridge.getPrivilegeState!()).toEqual({ supported: true, elevated: null });
    expect(bridge.getHelperToken()).toBeNull();
  });

  it("ignores a leftover token and elevation flag when no helper is running", async () => {
    invoke.mockImplementationOnce(async () => ({ helperExists: true, helperRunning: false, helperElevated: true, administratorModeSupported: true, token: "stale" }));
    const bridge = await createDesktopHostBridge();
    expect(bridge.getPrivilegeState!().elevated).toBe(false);
    expect(bridge.getHelperToken()).toBeNull();
  });

  it("refreshes state after automatic restart failure without invoking elevation", async () => {
    const bridge = await createDesktopHostBridge();
    invoke.mockRejectedValueOnce("restart failed");
    invoke.mockResolvedValueOnce({ helperRunning: false, helperElevated: true, token: "stale" });
    expect(await bridge.startHelper()).toMatchObject({ ok: false, error: "restart failed" });
    expect(invoke.mock.calls.slice(2).map(([command]) => command)).toEqual(["start_helper", "desktop_status"]);
    expect(bridge.getPrivilegeState!().elevated).toBe(false);
    expect(bridge.getHelperToken()).toBeNull();
  });

  it("clears state after stopping and refreshes it when stopping fails", async () => {
    const bridge = await createDesktopHostBridge();
    invoke.mockRejectedValueOnce("stop failed");
    invoke.mockResolvedValueOnce({ helperRunning: true, helperElevated: true, token: "still-running" });
    expect(await bridge.stopHelper()).toEqual({ ok: false, error: "stop failed" });
    expect(bridge.getPrivilegeState!().elevated).toBe(true);
    expect(bridge.getHelperToken()).toBe("still-running");
    invoke.mockResolvedValueOnce({});
    expect(await bridge.stopHelper()).toEqual({ ok: true });
    expect(bridge.getPrivilegeState!().elevated).toBe(false);
    expect(bridge.getHelperToken()).toBeNull();
  });

  it("refreshes the token after a denied UAC request falls back to ordinary mode", async () => {
    const bridge = await createDesktopHostBridge();
    invoke.mockResolvedValueOnce({ token: "fallback-token", elevated: false, helperPath: "helper.exe", warning: "adminFallback" });
    expect(await bridge.setHelperElevation!(true)).toEqual({ ok: true, elevated: false, warning: "adminFallback" });
    expect(bridge.getHelperToken()).toBe("fallback-token");
    expect(bridge.getPrivilegeState!().elevated).toBe(false);
  });

  it("reports the real startup state after a failed preference update without switching the current helper", async () => {
    const bridge = await createDesktopHostBridge();
    invoke.mockRejectedValueOnce("access denied");
    invoke.mockResolvedValueOnce({ enabled: false });
    expect(await bridge.setAdminStartup!(true)).toEqual({ enabled: false, error: "access denied" });
    expect(invoke.mock.calls.slice(2).map(([command]) => command)).toEqual(["set_admin_startup", "admin_startup_status"]);
    expect(bridge.getHelperToken()).toBe("old");
    expect(bridge.getPrivilegeState!().elevated).toBe(false);
  });

  it("keeps a failed startup update unknown when its status cannot be read", async () => {
    const bridge = await createDesktopHostBridge();
    invoke.mockRejectedValueOnce("access denied");
    invoke.mockRejectedValueOnce("registry unavailable");
    expect(await bridge.setAdminStartup!(false)).toEqual({ enabled: null, error: "access denied" });
  });

  it("passes login fallback warnings to the UI while retaining the ordinary helper token", async () => {
    const bridge = await createDesktopHostBridge();
    invoke.mockResolvedValueOnce({ token: "ordinary", elevated: false, warning: "adminFallback" });
    expect(await bridge.startHelper()).toMatchObject({ ok: true, warning: "adminFallback" });
    expect(bridge.getHelperToken()).toBe("ordinary");
    expect(bridge.getPrivilegeState!().elevated).toBe(false);
  });
});
