import { beforeEach, describe, expect, it, vi } from "vitest";
const invoke = vi.hoisted(() => vi.fn());
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
      if (command === "desktop_status") return { helperExists: true, helperElevated: false, administratorModeSupported: true, token: "old" };
      throw new Error(`Unexpected command: ${command}`);
    });
  });

  it("replaces the connection token after elevation and after cancellation fallback", async () => {
    const bridge = await createDesktopHostBridge();
    invoke.mockResolvedValueOnce({ token: "elevated-token", elevated: true, warning: null });
    expect(await bridge.setHelperElevation!(true)).toEqual({ ok: true, elevated: true, warning: undefined });
    expect(invoke).toHaveBeenLastCalledWith("set_helper_elevation", { elevated: true });
    expect(bridge.getHelperToken()).toBe("elevated-token");
    expect(bridge.getPrivilegeState!()).toEqual({ supported: true, elevated: true });
    invoke.mockResolvedValueOnce({ token: "ordinary-token", elevated: false, warning: "adminCancelled" });
    expect(await bridge.setHelperElevation!(true)).toEqual({ ok: true, elevated: false, warning: "adminCancelled" });
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
});
