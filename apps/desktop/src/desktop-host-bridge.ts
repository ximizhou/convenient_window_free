import { translator, LANGUAGE_KEY, resolveInitialLanguage, type Language } from "./i18n";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { DesktopDiagnostics, HostBridge } from "./host-bridge";

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
  let token = status.token;
  let elevated = status.helperElevated;
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
          dataDir: result.dataDir
        };
      } catch (error) {
        return { ok: false, error: errorMessage(error), helperPath: helperState.installDir };
      }
    },
    async stopHelper() {
      try {
        await invoke("stop_helper");
        elevated = false;
        return { ok: true };
      } catch (error) {
        return { ok: false, error: errorMessage(error) };
      }
    },
    getPrivilegeState: () => ({ supported: status.administratorModeSupported, elevated }),
    async setHelperElevation(desired) {
      try {
        const result = await invoke<StartHelperResult>("set_helper_elevation", { elevated: desired });
        token = result.token;
        elevated = result.elevated;
        return { ok: true, elevated, warning: result.warning ?? undefined };
      } catch (error) {
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
