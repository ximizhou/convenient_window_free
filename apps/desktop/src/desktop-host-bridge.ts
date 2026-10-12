import { translator, LANGUAGE_KEY, resolveInitialLanguage, type Language } from "./i18n";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { AdminStartupState, DesktopDiagnostics, HostBridge } from "./host-bridge";

interface DesktopStatus {
  dataDir: string;
  helperPath: string;
  helperExists: boolean;
  helperRunning: boolean;
  helperBytes: number;
  helperVersion: string;
  helperError: string | null;
  repository: string;
  token: string | null;
  helperElevated: boolean;
  administratorModeSupported: boolean;
}

interface StartHelperResult {
  alreadyRunning: boolean;
  dataDir: string;
  helperPath: string;
  token: string;
  elevated: boolean;
  warning: string | null;
}

const PUBLIC_REPOSITORY = "https://github.com/ximizhou/convenient_window_free";

export async function createDesktopHostBridge(): Promise<HostBridge> {
  const [status, initialSettings] = await Promise.all([
    invoke<DesktopStatus>("desktop_status"),
    invoke<unknown | null>("load_config")
  ]);
  let storedLanguage: string | null = null;
  try { storedLanguage = localStorage.getItem(LANGUAGE_KEY); } catch { /* Storage may be unavailable. */ }
  let language: Language = resolveInitialLanguage(storedLanguage, navigator.language);
  let token = status.helperRunning ? status.token : null;
  let elevated: boolean | null = status.helperRunning ? status.helperElevated : false;
  let saveQueue: Promise<void> = Promise.resolve();
  let helperState: HelperInstallState = {
    installed: status.helperExists,
    development: false,
    version: status.helperVersion,
    bytes: status.helperBytes,
    repository: status.repository,
    release: status.repository,
    installDir: status.helperPath,
    error: status.helperError ?? undefined
  };

  async function refreshRuntimeState(): Promise<void> {
    try {
      const current = await invoke<DesktopStatus>("desktop_status");
      elevated = current.helperRunning ? current.helperElevated : false;
      // The token file can outlive a stopped helper; never reconnect using it.
      token = current.helperRunning ? current.token : null;
    } catch {
      elevated = null;
      token = null;
    }
  }

  return {
    kind: "desktop",
    async setLanguage(value) {
      language = value;
      const ui = translator(language);
      await invoke("set_native_labels", { labels: { title: ui("brandName"), show: ui("trayShow"), autostart: ui("trayAutostart"), quit: ui("trayQuit") } });
    },
    async startHelper() {
      try {
        const result = await invoke<StartHelperResult>("start_helper");
        token = result.token;
        elevated = result.elevated;
        helperState = { ...helperState, installDir: result.helperPath };
        return {
          ok: true,
          alreadyRunning: result.alreadyRunning,
          helperPath: result.helperPath,
          dataDir: result.dataDir,
          warning: result.warning ?? undefined
        };
      } catch (error) {
        await refreshRuntimeState();
        return { ok: false, error: errorMessage(error), helperPath: helperState.installDir };
      }
    },
    async stopHelper() {
      try {
        await invoke("stop_helper");
        token = null;
        elevated = false;
        return { ok: true };
      } catch (error) {
        await refreshRuntimeState();
        return { ok: false, error: errorMessage(error) };
      }
    },
    getPrivilegeSupport: () => ({ supported: status.administratorModeSupported }),
    getPrivilegeState: () => ({ supported: status.administratorModeSupported, elevated }),
    async getStartup() {
      try { return { enabled: await invoke<boolean>("startup_status") }; }
      catch { return { enabled: null, error: "startupUnknown" }; }
    },
    async setStartup(enabled) {
      try { return { enabled: await invoke<boolean>("set_startup", { enabled }) }; }
      catch (error) {
        const actual = await invoke<boolean>("startup_status").catch(() => null);
        return { enabled: actual, error: errorMessage(error) };
      }
    },
    onStartupChanged: handler => listen<string | null>("startup-changed", event => handler(event.payload ?? undefined)),
    async getAdminStartup() {
      try { return await invoke<AdminStartupState>("admin_startup_status"); }
      catch { return { enabled: null, error: "adminStartupUnknown" }; }
    },
    async setAdminStartup(enabled) {
      try { return await invoke<AdminStartupState>("set_admin_startup", { enabled }); }
      catch (error) {
        const state = await invoke<AdminStartupState>("admin_startup_status").catch(() => ({ enabled: null }));
        return { ...state, error: errorMessage(error) };
      }
    },
    async setHelperElevation(desired) {
      try {
        const result = await invoke<StartHelperResult>("set_helper_elevation", { elevated: desired });
        token = result.token;
        elevated = result.elevated;
        helperState = { ...helperState, installDir: result.helperPath };
        return { ok: true, elevated, warning: result.warning ?? undefined };
      } catch (error) {
        await refreshRuntimeState();
        return { ok: false, elevated, error: errorMessage(error) };
      }
    },
    getHelperToken: () => token,
    getHelperState: () => ({ ...helperState }),
    openExternal(url) {
      if (url !== PUBLIC_REPOSITORY) return { ok: false, error: "不允许打开未登记的外部地址" };
      void openUrl(url).catch((error) => console.error("External URL open failed", error));
      return { ok: true };
    },
    redirect() {
      return { ok: false, error: "独立版不支持 uTools 宿主动作" };
    },
    diagnostics: () => invoke<DesktopDiagnostics>("diagnostics"),
    getInitialSettings: () => initialSettings,
    saveSettings(settings) {
      const saveTask = saveQueue.then(() => invoke<void>("save_config", { settings }));
      saveQueue = saveTask.catch(() => undefined);
      return saveTask;
    },
    async importSettings() {
      const selected = await open({
        title: translator(language)("configImportTitle"),
        multiple: false,
        directory: false,
        filters: [{ name: translator(language)("configFileType"), extensions: ["json"] }]
      });
      if (!selected || Array.isArray(selected)) return null;
      const content = await invoke<string>("read_config_file", { path: selected });
      return JSON.parse(content);
    },
    async exportSettings(settings) {
      const selected = await save({
        title: translator(language)("configExportTitle"),
        defaultPath: "convenient-window-settings.json",
        filters: [{ name: translator(language)("configFileType"), extensions: ["json"] }]
      });
      if (!selected) return false;
      await invoke("write_config_file", {
        path: selected,
        content: `${JSON.stringify(settings, null, 2)}\n`
      });
      return true;
    }
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
