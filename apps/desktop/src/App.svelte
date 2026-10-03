<script lang="ts">
  import { gestureDisplayName } from "./gesture-names";
  import { runtimeErrorKey } from "./runtime-error";
  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import { HelperClient, isSupportedHelperProtocol, SUPPORTED_HELPER_PROTOCOL } from "./helper-client";
  import { needsHelperUpgrade } from "./helper-version";
  import { HelperRecoveryGuard } from "./helper-recovery";
  import ActionPicker from "./ActionPicker.svelte";
  import MonitorStage from "./MonitorStage.svelte";
  import GestureCanvas from "./GestureCanvas.svelte";
  import ModifierRecorder from "./ModifierRecorder.svelte";
  import ShortcutRecorder from "./ShortcutRecorder.svelte";
  import { gestureSimilarity, resampleGesture } from "./gesture-algorithm";
  import { migrateMonitorProfileIds } from "./monitor-profile-migration";
  import { addModifierVariant, MAX_MODIFIER_VARIANTS } from "./modifier-variants";
  import { prepareSettingsUpdate } from "./settings-sync";
  import { SettingsApplyController } from "./settings-persistence";
  import { getHostBridge } from "./host-bridge";
  import { defaultSettings, loadSettings, MAX_GESTURE_TEMPLATES, normalizeSettings, saveSettings } from "./settings-store";
  import { LANGUAGE_KEY, normalizeLanguage, resolveInitialLanguage, translator } from "./i18n";
  import { en, format, zh } from "./i18n";
  import type { Language, UiKey } from "./i18n";
  import type {
    ActionKind, AppSettings, DisplayInfo, Edge, HelperPlatformInfo, HelperStatus, HotzoneAction,
    GesturePoint, GestureTemplate, HotzoneId, HotzoneSetting, ModifierKey,
    OcrLanguage, TriggerAction, TriggerKind
  } from "./types";

  type Mode = "power" | "hotzones" | "edge-hide" | "gestures" | "more";
  type ConnectionTestState = "idle" | "testing" | "success" | "failed";
  type ActionPreset = { label: string; labelEn: string; group: string; groupEn: string; kind: ActionKind; value?: string };
  type FeatureTutorial = "edge-hide";
  const host = getHostBridge();
  const helper = new HelperClient("ws://127.0.0.1:56873", () => host.getHelperToken(), () => language);
  const fallbackDisplay: DisplayInfo = {
    id: "display:0:0:1920:1080", primary: true,
    bounds: { left: 0, top: 0, right: 1920, bottom: 1080 },
    workArea: { left: 0, top: 0, right: 1920, bottom: 1040 }
  };
  const zoneLabels: Record<HotzoneId, string> = {
    "top-left": "左上角", top: "上边缘", "top-right": "右上角", right: "右边缘",
    "bottom-right": "右下角", bottom: "下边缘", "bottom-left": "左下角", left: "左边缘"
  };
  const zoneLabelsEn: Record<HotzoneId, string> = {
    "top-left": "Top-left corner", top: "Top edge", "top-right": "Top-right corner", right: "Right edge",
    "bottom-right": "Bottom-right corner", bottom: "Bottom edge", "bottom-left": "Bottom-left corner", left: "Left edge"
  };
  const triggerLabels: Record<TriggerKind, string> = {
    hover: "悬停", "left-click": "左键", "right-click": "右键", "wheel-up": "滚轮上",
    "wheel-down": "滚轮下", "slide-forward": "向下移动", "slide-backward": "向上移动"
  };
  const triggerLabelsEn: Record<TriggerKind, string> = {
    hover: "Hover", "left-click": "Left click", "right-click": "Right click", "wheel-up": "Wheel up",
    "wheel-down": "Wheel down", "slide-forward": "Slide down", "slide-backward": "Slide up"
  };
  const triggerGroups: { label: string; labelEn: string; items: TriggerKind[] }[] = [
    { label: "悬停", labelEn: "Hover", items: ["hover"] }, { label: "左键", labelEn: "Left", items: ["left-click"] },
    { label: "右键", labelEn: "Right", items: ["right-click"] }, { label: "滚轮", labelEn: "Wheel", items: ["wheel-up", "wheel-down"] },
    { label: "滑动", labelEn: "Swipe", items: ["slide-forward", "slide-backward"] }
  ];
  const actionPresets: ActionPreset[] = [
    { label: "不执行", labelEn: "None", group: "基础", groupEn: "Basic", kind: "none" },
    { label: "显示桌面", labelEn: "Show desktop", group: "桌面与窗口", groupEn: "Desktop & windows", kind: "show-desktop" },
    { label: "切换窗口置顶", labelEn: "Toggle always on top", group: "桌面与窗口", groupEn: "Desktop & windows", kind: "toggle-window-topmost" },
    { label: "任务视图", labelEn: "Task view", group: "桌面与窗口", groupEn: "Desktop & windows", kind: "shortcut", value: "Win+Tab" },
    { label: "文件资源管理器", labelEn: "File Explorer", group: "桌面与窗口", groupEn: "Desktop & windows", kind: "shortcut", value: "Win+E" },
    { label: "Windows 搜索", labelEn: "Windows Search", group: "桌面与窗口", groupEn: "Desktop & windows", kind: "shortcut", value: "Win+S" },
    { label: "锁定屏幕", labelEn: "Lock screen", group: "桌面与窗口", groupEn: "Desktop & windows", kind: "lock-screen" },
    { label: "最小化窗口", labelEn: "Minimize window", group: "桌面与窗口", groupEn: "Desktop & windows", kind: "shortcut", value: "Win+Down" },
    { label: "最大化窗口", labelEn: "Maximize window", group: "桌面与窗口", groupEn: "Desktop & windows", kind: "shortcut", value: "Win+Up" },
    { label: "关闭窗口", labelEn: "Close window", group: "桌面与窗口", groupEn: "Desktop & windows", kind: "shortcut", value: "Alt+F4" },
    { label: "上一个窗口", labelEn: "Previous window", group: "切换与导航", groupEn: "Switch & navigate", kind: "shortcut", value: "Alt+Shift+Tab" },
    { label: "下一个窗口", labelEn: "Next window", group: "切换与导航", groupEn: "Switch & navigate", kind: "shortcut", value: "Alt+Tab" },
    { label: "上一个标签页", labelEn: "Previous tab", group: "切换与导航", groupEn: "Switch & navigate", kind: "shortcut", value: "Ctrl+Shift+Tab" },
    { label: "下一个标签页", labelEn: "Next tab", group: "切换与导航", groupEn: "Switch & navigate", kind: "shortcut", value: "Ctrl+Tab" },
    { label: "上一个虚拟桌面", labelEn: "Previous desktop", group: "切换与导航", groupEn: "Switch & navigate", kind: "shortcut", value: "Win+Ctrl+Left" },
    { label: "下一个虚拟桌面", labelEn: "Next desktop", group: "切换与导航", groupEn: "Switch & navigate", kind: "shortcut", value: "Win+Ctrl+Right" },
    { label: "亮度增大", labelEn: "Brightness up", group: "屏幕", groupEn: "Screen", kind: "brightness-adjust", value: "0.05" },
    { label: "亮度减小", labelEn: "Brightness down", group: "屏幕", groupEn: "Screen", kind: "brightness-adjust", value: "-0.05" },
    { label: "音量增大", labelEn: "Volume up", group: "声音与媒体", groupEn: "Sound & media", kind: "volume-adjust", value: "0.02" },
    { label: "音量减小", labelEn: "Volume down", group: "声音与媒体", groupEn: "Sound & media", kind: "volume-adjust", value: "-0.02" },
    { label: "静音", labelEn: "Mute", group: "声音与媒体", groupEn: "Sound & media", kind: "shortcut", value: "VolumeMute" },
    { label: "播放 / 暂停", labelEn: "Play / pause", group: "声音与媒体", groupEn: "Sound & media", kind: "shortcut", value: "MediaPlayPause" },
    { label: "复制", labelEn: "Copy", group: "编辑", groupEn: "Edit", kind: "shortcut", value: "Ctrl+C" },
    { label: "粘贴", labelEn: "Paste", group: "编辑", groupEn: "Edit", kind: "shortcut", value: "Ctrl+V" },
    { label: "自定义快捷键", labelEn: "Custom shortcut", group: "高级", groupEn: "Advanced", kind: "shortcut" },
    { label: "运行命令", labelEn: "Run command", group: "高级", groupEn: "Advanced", kind: "open-command" }
  ];
  const maxCustomGestures = MAX_GESTURE_TEMPLATES - defaultSettings.mouseGestures.gestures.length;
  const CONFIG_APPLY_DEBOUNCE_MS = 260;
  const HELPER_STABILITY_MS = 60_000;
  const capabilityLabels: Record<keyof HelperPlatformInfo["capabilities"], [string, string]> = {
    globalInput: ["全局输入", "global input"],
    windowControl: ["窗口控制", "window control"],
    windowTopmost: ["窗口置顶", "window topmost"],
    screenCapture: ["屏幕截图", "screen capture"],
    ocr: ["文字识别", "OCR"],
    audio: ["音量控制", "audio"],
    systemActions: ["系统动作", "system actions"],
    edgeHide: ["贴边隐藏", "edge hide"]
  };

  function unavailableCapabilityText(platform: HelperPlatformInfo): string {
    const unavailable = Object.entries(platform.capabilities)
      .filter(([, enabled]) => !enabled)
      .map(([name]) => capabilityLabels[name as keyof HelperPlatformInfo["capabilities"]]?.[english ? 1 : 0] ?? name);
    return unavailable.length > 0
      ? `${english ? "Unavailable" : "暂不支持"}: ${unavailable.join(english ? ", " : "、")}`
      : (english ? "Base capabilities available" : "基础能力可用");
  }

  let settings: AppSettings = loadSettings();
  let mode: Mode | null = null;
  let helperStatus: HelperStatus = "disconnected";
  const administratorModeSupported = host.getPrivilegeState?.().supported ?? false;
  let helperElevated = host.getPrivilegeState?.().elevated ?? false;
  let switchingPrivilege = false;
  let privilegeNotice = "";
  let helperPlatform: HelperPlatformInfo | null = null;
  let displays: DisplayInfo[] = [fallbackDisplay];
  let displayReady = false;
  let selectedDisplayId = fallbackDisplay.id;
  let selectedZone: HotzoneId = "right";
  let activeTrigger: TriggerKind = "hover";
  let selectedHotzoneModifiers: ModifierKey[] = [];
  let hotzoneModifierDraft: ModifierKey[] | null = null;
  let hotzoneModifierError = "";
  let shortcutError = "";
  // 录制结果的响应式镜像：currentAction() 返回的对象不保证被 Svelte 追踪，
  // 直接读它的 .value 在录制后不会刷新界面，用独立状态驱动显示。
  let shortcutDraft = "";
  let windowEnhancementTab: "edge" | "drag" = "edge";
  let activeFeatureTutorial: FeatureTutorial | null = null;
  $: windowDragBindingConflict = settings.windowDrag.moveButton === settings.windowDrag.resizeButton
    && sameModifiers(settings.windowDrag.moveModifiers, settings.windowDrag.resizeModifiers);
  let actionEditorRevision = 0;
  let actionEditorOverrideKey = "";
  let actionEditorOverride: HotzoneAction | null = null;
  $: currentDisplayHotzones = settings.monitorProfiles.find(
    (profile) => profile.monitorId === selectedDisplayId
  )?.hotzones ?? settings.hotzones;
  $: currentDisplayEdgeHideEdges = settings.edgeHide.monitorProfiles.find(
    (profile) => profile.monitorId === selectedDisplayId
  )?.edges ?? settings.edgeHide.edges;
  $: currentHotzoneModifierActions = currentDisplayHotzones
    .find((zone) => zone.id === selectedZone)?.actions
    .find((slot) => slot.trigger === activeTrigger)?.modifierActions ?? [];
  let selectedGestureId = settings.mouseGestures.gestures[0]?.id ?? "";
  let selectedGestureModifiers: ModifierKey[] = [];
  let gestureModifierDraft: ModifierKey[] | null = null;
  let gestureModifierError = "";
  // 手势动作对象不是响应式：原地改 kind/value 界面读不到。用 draft 驱动编辑区显示。
  // 只在「切换手势」和「设置动作」两处赋值；不要用 $: 同步 ——
  // 录制结果写回 action 后 $: 会拿旧依赖值把 draft 覆盖回去。
  let gestureActionDraft: HotzoneAction = { kind: "none" };
  // 与快捷键录制器同理：手势动作的当前对象不保证被 Svelte 追踪，
  // 用独立状态驱动显示，避免「值写进去但界面不刷新」。
  let gestureShortcutDraft = "";
  let gestureShortcutError = "";
  let availableOcrLanguages: OcrLanguage[] | null = null;
  $: activeGesture = settings.mouseGestures.gestures.find((gesture) => gesture.id === selectedGestureId)
    ?? settings.mouseGestures.gestures[0];
  let gestureConflict = "";
  let foregroundApp = "";
  let runtimeSummary = "";
  let lastAction = "";
  let lastMessage: UiStatusKey | string = "waiting";
  let helperError = "";
  let helperInstallState: HelperInstallState = host.getHelperState();
  const settingsApply = new SettingsApplyController<AppSettings>(
    (value) => saveSettings(value),
    (value) => helper.sendConfig(value),
    CONFIG_APPLY_DEBOUNCE_MS
  );
  let stopping = false;
  let starting = false;
  let upgradingHelper = false;
  let helperUpgradeAttempts = 0;
  let helperUpgradeTimer: ReturnType<typeof setTimeout> | null = null;
  const helperRecovery = new HelperRecoveryGuard();
  let helperWasReady = false;
  let recoveringHelper = false;
  let helperRecoveryFailed = false;
  let helperRecoveryTimer: ReturnType<typeof setTimeout> | null = null;
  let helperStabilityTimer: ReturnType<typeof setTimeout> | null = null;
  let connectionTestState: ConnectionTestState = "idle";
  let connectionTestStartedAt = 0;
  let connectionTestTimer: ReturnType<typeof setTimeout> | null = null;
  let connectionTestResetTimer: ReturnType<typeof setTimeout> | null = null;
  const expectedHelperVersion = helperInstallState.development || helperInstallState.version === "unknown"
    ? undefined
    : helperInstallState.version;

  type Theme = "dark" | "light";
  const THEME_KEY = "convenient-window-theme";
  let theme: Theme = "dark";
  function applyTheme(value: Theme, persistChoice = true): void {
    theme = value;
    document.documentElement.dataset.theme = value;
    if (persistChoice) { try { localStorage.setItem(THEME_KEY, value); } catch { /* 忽略隐私模式写入失败 */ } }
  }
  function initTheme(): void {
    let stored: string | null = null;
    try { stored = localStorage.getItem(THEME_KEY); } catch { /* 忽略读取失败 */ }
    const initial: Theme = stored === "light" || stored === "dark"
      ? stored
      : window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
    applyTheme(initial, false);
  }
  function toggleTheme(): void { applyTheme(theme === "dark" ? "light" : "dark"); }

  let language: Language = "zh-CN";
  $: english = language === "en-US";
  let ui = translator("zh-CN");
  $: ui = translator(language);
  $: void host.setLanguage?.(language).catch(error => console.error("Native language update failed", error));
  $: if (typeof document !== "undefined") document.documentElement.lang = language;
  function initLanguage(): void {
    let stored: string | null = null;
    try { stored = localStorage.getItem(LANGUAGE_KEY); } catch { /* 忽略读取失败 */ }
    // 没有保存过选择时跟随系统语言：zh* 用简中，其余一律英文。
    language = resolveInitialLanguage(stored, typeof navigator === "undefined" ? undefined : navigator.language);
  }
  function setLanguage(event: Event): void {
    language = normalizeLanguage((event.currentTarget as HTMLSelectElement).value);
    try { localStorage.setItem(LANGUAGE_KEY, language); } catch { /* Ignore unavailable storage. */ }
    helper.sendConfig(settings);
  }

  // 状态栏文案与 uTools 插件保持同一套字典键和同一套映射写法。
  type UiStatusKey = "waiting" | "helperSync" | "helperStopped" | "configApplied" | "configAdjusted" | "displaysDetected" | "runtimeUpdated" | "actionTriggered" | "ocrCopied" | "helperResponse" | "masterOff" | "helperInstallNeeded" | "checking" | "requestNotSent" | "testTimeout" | "savedApplying" | "configSavedApplying" | "configSent" | "savedWaiting" | "foregroundMissing" | "gestureLimit" | "helperRecovering" | "helperRecoveringConnection" | "helperRecoveryStopped" | "helperRecoveryFailed" | "helperRestarting" | "helperStarting" | "helperStartedConnecting" | "helperStopping" | "helperStartFailed" | "helperUpgradeFailed" | "helperUpgradeStarted" | "helperUpgradeStartFailed" | "helperErrorTitle" | "helperRecovered" | "connectionLost" | "bridgeMissingDesktop" | "runtimeIdle" | "noAction" | "configResetSent" | "configResetDone" | "configExported" | "configExportCancelled" | "configImported" | "configImportCancelled" | "configInvalid" | "diagnosticsCopied" | "displaysMigrated" | "gestureCreated" | "gestureDuplicateCreated" | "gestureDeleted" | "gestureSamplesUpdated" | "gestureSamplesCleared" | "gestureVariantAdded" | "gestureVariantDeleted" | "variantAdded" | "variantDeleted" | "edgeUpdated";
  const statusKeys: Record<UiStatusKey, UiKey> = {
    waiting: "statusWaiting", helperSync: "statusHelperSync", helperStopped: "statusHelperStopped",
    configApplied: "statusConfigApplied", configAdjusted: "statusConfigAdjusted", displaysDetected: "statusDisplaysDetected", runtimeUpdated: "statusRuntimeUpdated",
    actionTriggered: "statusActionTriggered", ocrCopied: "statusOcrCopied", helperResponse: "statusHelperResponse",
    masterOff: "statusMasterOff", helperInstallNeeded: "statusHelperInstallNeeded",
    checking: "statusChecking", requestNotSent: "statusRequestNotSent", testTimeout: "statusTestTimeout",
    savedApplying: "statusSavedApplying", configSavedApplying: "configSavedApplying", configSent: "statusConfigSent", savedWaiting: "statusSavedWaiting", foregroundMissing: "statusForegroundMissing",
    gestureLimit: "statusGestureLimit", helperRecovering: "statusHelperRecovering", helperRecoveringConnection: "statusHelperRecoveringConnection", helperRecoveryStopped: "statusHelperRecoveryStopped", helperRecoveryFailed: "helperRecoveryFailed",
    helperRestarting: "statusHelperRestarting", helperStarting: "statusHelperStarting", helperStartedConnecting: "helperStartedConnecting", helperStopping: "statusHelperStopping", helperStartFailed: "statusHelperStartFailed",
    helperUpgradeFailed: "helperUpgradeFailed", helperUpgradeStarted: "helperUpgradeStarted", helperUpgradeStartFailed: "helperUpgradeStartFailed", helperErrorTitle: "helperErrorTitle", helperRecovered: "helperRecovered",
    connectionLost: "statusConnectionLost", bridgeMissingDesktop: "bridgeMissingDesktop", runtimeIdle: "statusRuntimeIdle", noAction: "statusNoAction",
    configResetSent: "defaultConfigSent", configResetDone: "configResetDone", configExported: "configExported", configExportCancelled: "configExportCancelled",
    configImported: "configImported", configImportCancelled: "configImportCancelled", configInvalid: "configInvalid", diagnosticsCopied: "diagnosticsCopied", displaysMigrated: "displaysMigrated",
    gestureCreated: "gestureCreated", gestureDuplicateCreated: "gestureDuplicateCreated", gestureDeleted: "gestureDeleted", gestureSamplesUpdated: "gestureSamplesUpdated", gestureSamplesCleared: "gestureSamplesCleared",
    gestureVariantAdded: "gestureVariantAdded", gestureVariantDeleted: "gestureVariantDeleted", variantAdded: "variantAdded", variantDeleted: "variantDeleted", edgeUpdated: "edgeUpdated"
  };
  function statusText(status: UiStatusKey | string, useEnglish = english): string {
    const key = statusKeys[status as UiStatusKey] ?? (Object.hasOwn(zh, status) ? status as UiKey : undefined);
    return key ? (useEnglish ? en : zh)[key] : status;
  }
  $: renderedLastMessage = statusText(lastMessage, english);

  onMount(() => {
    initTheme();
    initLanguage();
    // 冷启动补一次 draft 同步：selectedGestureId 初值指向 gestures[0]（默认"向上 · 复制"，
    // 动作是快捷键 Ctrl+C），而 gestureActionDraft 初值是 {kind:"none"}，两者不一致会让
    // 用户首次打开手势面板就看到"不执行"（已用 E2E 复现）。syncGestureActionDraft 原本
    // 只在 selectGesture 里调用，于是"不点卡片直接进面板"这条默认路径漏了同步。
    syncGestureActionDraft();
    const offStatus = helper.onStatus((status) => {
      helperStatus = status;
      if (status === "connected") lastMessage = "helperSync";
      if (status === "disconnected" && connectionTestState === "testing") finishConnectionTest(false, "connectionLost");
      if (status === "disconnected") {
        helperElevated = false;
        displayReady = false;
        runtimeSummary = "";
        helperPlatform = null;
        const wasReady = helperWasReady;
        helperWasReady = false;
        if (!switchingPrivilege && !stopping && !upgradingHelper && settings.enabled && !helperError) {
          helperError = "helperRecoveringConnection";
        }
        if (switchingPrivilege) return;
        if (upgradingHelper) scheduleHelperUpgradeRestart();
        else if (stopping) { stopping = false; lastMessage = "helperStopped"; }
        else if (!helperRecoveryFailed && settings.enabled && (wasReady || recoveringHelper)) scheduleHelperRecovery();
      }
    });
    const offMessage = helper.onMessage((message) => {
      if (message.type === "helper.ready") {
        const data = message.data as { version?: unknown; protocolVersion?: unknown; ocrLanguages?: unknown; platform?: HelperPlatformInfo; elevated?: boolean } | null;
        helperElevated = data?.elevated === true;
        helperPlatform = helper.platformInfo ?? (data?.platform ?? null);
        availableOcrLanguages = Array.isArray(data?.ocrLanguages)
          ? data.ocrLanguages.filter((language): language is OcrLanguage => language === "auto" || language === "zh-Hans" || language === "en")
          : null;
        if (!isSupportedHelperProtocol(data?.protocolVersion)) {
          lastMessage = format(ui("statusHelperProtocol"), { min: SUPPORTED_HELPER_PROTOCOL, max: SUPPORTED_HELPER_PROTOCOL });
          helperError = lastMessage;
          helperRecoveryFailed = true;
          helper.stop();
        } else if (needsHelperUpgrade(expectedHelperVersion, data?.version)) {
          requestHelperUpgrade(data?.version);
        } else {
          helperUpgradeAttempts = 0;
          helperError = "";
          markHelperReady();
        }
      } else if (message.type === "config.applied") {
        const data = message.data as { revision?: unknown; adjusted?: unknown } | null;
        if (helper.isLatestConfigRevision(data?.revision)) {
          lastMessage = data?.adjusted ? "configAdjusted" : "configApplied";
        }
      } else if (message.type === "runtime.status") {
        const data = message.data as { displays?: DisplayInfo[]; foreground?: string; message?: string };
        if (Array.isArray(data.displays) && data.displays.length) {
          displayReady = true;
          displays = data.displays;
          const selectedDisplay = displays.find((display) =>
            display.id === selectedDisplayId || display.legacyId === selectedDisplayId
          );
          if (selectedDisplay) selectedDisplayId = selectedDisplay.id;
          if (migrateMonitorProfileIds(settings, displays)) persist("displaysMigrated");
          if (!displays.some((display) => display.id === selectedDisplayId)) {
            selectedDisplayId = displays.find((display) => display.primary)?.id ?? displays[0].id;
          }
          runtimeSummary = `${displays.length}${statusText("displaysDetected")}`;
        }
        if (data.foreground) foregroundApp = data.foreground;
        const status = message.data as { code?: string; params?: { title?: string; topmost?: boolean } };
        if (status.code === "gesture_not_recognized") lastMessage = "gestureNotRecognized";
        if (status.code === "window_topmost_changed") {
          lastMessage = format(ui(status.params?.topmost ? "windowPinned" : "windowUnpinned"), { title: status.params?.title || ui("unnamedWindow") });
        }
      } else if (message.type === "action.triggered") {
        const data = message.data as { source?: string; kind?: string };
        lastAction = [data.source, data.kind].filter(Boolean).join(" · ") || "actionTriggered";
      } else if (message.type === "gesture.recognized") {
        const data = message.data as { id?: string; name?: string; capturePath?: unknown };
        const recognized = settings.mouseGestures.gestures.find(gesture => gesture.id === data.id);
        const name = gestureName(recognized ?? { id: data.id ?? "", name: data.name } as GestureTemplate);
        lastAction = typeof data.capturePath === "string" && data.capturePath.length
          ? `${name} · ${data.capturePath}`
          : name;
      } else if (message.type === "ocr.completed") {
        const data = message.data as { characters?: number };
        lastAction = ui("ocrCopiedShort");
        lastMessage = format(ui("statusOcrCopiedCount"), { count: Math.max(0, Number(data.characters) || 0) });
      } else if (message.type === "runtime.error") {
        lastMessage = runtimeErrorKey(message.data);
        helperError = lastMessage;
      } else if (message.type === "helper.pong") {
        if (connectionTestState === "testing") {
          const elapsedMs = Math.max(1, Math.round(performance.now() - connectionTestStartedAt));
          finishConnectionTest(true, `${statusText("connectionOk")} · ${elapsedMs} ms`);
        } else if (connectionTestState === "idle") {
          lastMessage = "helperResponse";
        }
      }
    });
    helper.sendConfig(settings);
    if (hasSecureBridge() && helperInstallState.installed && settings.enabled) void startHelper();
    else if (hasSecureBridge() && helperInstallState.installed) lastMessage = "masterOff";
    else if (hasSecureBridge()) lastMessage = "helperInstallNeeded";
    else lastMessage = "bridgeMissingDesktop";
    return () => { settingsApply.dispose(); if (helperUpgradeTimer) clearTimeout(helperUpgradeTimer); clearHelperRecoveryTimers(); clearConnectionTestTimers(); offStatus(); offMessage(); helper.disconnect(); };
  });

  function runConnectionTest(): void {
    clearConnectionTestTimers();
    connectionTestState = "testing";
    connectionTestStartedAt = performance.now();
    lastMessage = "checking";
    if (!helper.ping()) {
      finishConnectionTest(false, "requestNotSent");
      return;
    }
    connectionTestTimer = setTimeout(() => finishConnectionTest(false, "testTimeout"), 3000);
  }

  function finishConnectionTest(success: boolean, message: string): void {
    if (connectionTestTimer) clearTimeout(connectionTestTimer);
    connectionTestTimer = null;
    connectionTestState = success ? "success" : "failed";
    lastMessage = message;
    if (success) helperError = "";
    else helperError = message;
    connectionTestResetTimer = setTimeout(() => {
      connectionTestResetTimer = null;
      connectionTestState = "idle";
    }, 2400);
  }

  function clearConnectionTestTimers(): void {
    if (connectionTestTimer) clearTimeout(connectionTestTimer);
    if (connectionTestResetTimer) clearTimeout(connectionTestResetTimer);
    connectionTestTimer = null;
    connectionTestResetTimer = null;
  }

  function persist(message: UiStatusKey | string = "savedApplying"): void {
    const prepared = prepareSettingsUpdate(settings);
    settings = prepared.editable;
    lastMessage = "configSavedApplying";
    void persistPreparedSettings(prepared.normalized, message);
  }

  async function persistPreparedSettings(value: AppSettings, message: UiStatusKey | string): Promise<void> {
    const result = await settingsApply.schedule(value, (sent) => {
      lastMessage = sent ? "configSent" : "savedWaiting";
    });
    if (!handleSettingsCommit(result)) return;
    lastMessage = message;
  }

  async function savePreparedSettings(value: AppSettings): Promise<boolean> {
    return handleSettingsCommit(await settingsApply.commit(value));
  }

  function handleSettingsCommit(result: { latest: boolean; ok: boolean; error?: unknown }): boolean {
    if (!result.latest) return false;
    if (!result.ok) {
      lastMessage = result.error instanceof Error
        ? format(ui("statusConfigSaveFailed"), { error: result.error.message })
        : format(ui("statusConfigSaveFailed"), { error: String(result.error) });
      return false;
    }
    return true;
  }

  function currentHotzones(): HotzoneSetting[] {
    return currentDisplayHotzones;
  }

  function ensureProfile(): HotzoneSetting[] {
    let profile = settings.monitorProfiles.find((item) => item.monitorId === selectedDisplayId);
    if (!profile) {
      profile = { monitorId: selectedDisplayId, hotzones: cloneHotzones(settings.hotzones) };
      settings.monitorProfiles = [...settings.monitorProfiles, profile];
    }
    currentDisplayHotzones = profile.hotzones;
    return profile.hotzones;
  }

  function currentZone(): HotzoneSetting {
    return currentHotzones().find((zone) => zone.id === selectedZone) ?? currentHotzones()[0];
  }

  function modifierId(modifiers: ModifierKey[]): string {
    return modifiers.join("+");
  }

  function modifierLabel(modifiers: ModifierKey[]): string {
    if (!modifiers.length) return ui("direct");
    const labels: Record<ModifierKey, string> = { ctrl: "Ctrl", alt: "Alt", shift: "Shift", win: "Win" };
    return modifiers.map((key) => labels[key]).join("+");
  }

  function sameModifiers(left: ModifierKey[], right: ModifierKey[]): boolean {
    return modifierId(left) === modifierId(right);
  }

  function showFeatureTutorial(id: FeatureTutorial): void {
    activeFeatureTutorial = id;
  }

  function hideFeatureTutorial(id: FeatureTutorial): void {
    if (activeFeatureTutorial === id) activeFeatureTutorial = null;
  }

  function handleFeatureTutorialMouseLeave(id: FeatureTutorial, event: MouseEvent): void {
    const currentTarget = event.currentTarget;
    if (currentTarget instanceof HTMLElement && currentTarget.contains(document.activeElement)) return;
    hideFeatureTutorial(id);
  }

  function handleFeatureTutorialFocusOut(id: FeatureTutorial, event: FocusEvent): void {
    const currentTarget = event.currentTarget;
    const nextTarget = event.relatedTarget;
    if (currentTarget instanceof HTMLElement && nextTarget instanceof Node && currentTarget.contains(nextTarget)) return;
    hideFeatureTutorial(id);
  }

  function actionForModifiers(slot: TriggerAction, modifiers: ModifierKey[]): HotzoneAction {
    if (!modifiers.length) return slot.action;
    return (slot.modifierActions ?? []).find((item) => sameModifiers(item.modifiers, modifiers))?.action
      ?? { kind: "none" };
  }

  function currentAction(): HotzoneAction {
    const editorKey = `${selectedDisplayId}:${selectedZone}:${activeTrigger}:${modifierId(selectedHotzoneModifiers)}`;
    if (actionEditorOverrideKey === editorKey && actionEditorOverride) return actionEditorOverride;
    return actionForModifiers(currentTriggerSlot(), selectedHotzoneModifiers);
  }

  function currentTriggerSlot(): TriggerAction {
    return currentZone().actions.find((item) => item.trigger === activeTrigger)
      ?? { trigger: activeTrigger, action: { kind: "none" }, modifierActions: [], cooldownMs: settings.actionCooldownMs, hoverDelayMs: settings.hoverDelayMs };
  }

  function ensureHotzoneActionTarget(): { slot: TriggerAction; action: HotzoneAction } {
    const slot = ensureProfile().find((zone) => zone.id === selectedZone)!.actions.find((item) => item.trigger === activeTrigger)!;
    slot.modifierActions ??= [];
    if (!selectedHotzoneModifiers.length) return { slot, action: slot.action };
    const variant = slot.modifierActions.find((item) => sameModifiers(item.modifiers, selectedHotzoneModifiers));
    if (!variant) {
      selectedHotzoneModifiers = [];
      return { slot, action: slot.action };
    }
    return { slot, action: variant.action };
  }

  function beginHotzoneVariant(): void {
    const variants = currentTriggerSlot().modifierActions ?? [];
    if (variants.length >= MAX_MODIFIER_VARIANTS) {
      hotzoneModifierError = format(ui("statusVariantLimit"), { max: MAX_MODIFIER_VARIANTS });
      return;
    }
    hotzoneModifierDraft = [];
    hotzoneModifierError = "";
  }

  function commitHotzoneVariant(modifiers: ModifierKey[]): void {
    hotzoneModifierDraft = [...modifiers];
    if (!modifiers.length) {
      hotzoneModifierError = "";
      return;
    }
    const slot = ensureProfile().find((zone) => zone.id === selectedZone)!.actions.find((item) => item.trigger === activeTrigger)!;
    const result = addModifierVariant(slot.modifierActions, modifiers);
    if (!result.ok) {
      hotzoneModifierError = result.message;
      return;
    }
    slot.modifierActions = result.variants;
    currentDisplayHotzones = [...ensureProfile()];
    selectedHotzoneModifiers = result.modifiers;
    hotzoneModifierDraft = null;
    hotzoneModifierError = "";
    actionEditorRevision += 1;
    settings = { ...settings };
    persist("variantAdded");
  }

  function cancelHotzoneVariant(): void {
    hotzoneModifierDraft = null;
    hotzoneModifierError = "";
  }

  function selectHotzoneVariant(modifiers: ModifierKey[]): void {
    selectedHotzoneModifiers = [...modifiers];
    cancelHotzoneVariant();
    actionEditorRevision += 1;
  }

  function removeHotzoneVariant(modifiers: ModifierKey[]): void {
    const slot = ensureProfile().find((zone) => zone.id === selectedZone)!.actions.find((item) => item.trigger === activeTrigger)!;
    slot.modifierActions = (slot.modifierActions ?? []).filter((item) => !sameModifiers(item.modifiers, modifiers));
    currentDisplayHotzones = [...ensureProfile()];
    selectedHotzoneModifiers = [];
    actionEditorRevision += 1;
    persist("variantDeleted");
  }

  function selectZone(zone: HotzoneId): void {
    selectedZone = zone;
    selectedHotzoneModifiers = [];
    cancelHotzoneVariant();
    mode = "hotzones";
    if (!["top", "right", "bottom", "left"].includes(zone) && activeTrigger.startsWith("slide")) activeTrigger = "hover";
  }

  function toggleMode(nextMode: Mode): void {
    if (mode === nextMode) {
      closeMode();
      return;
    }
    if (mode === "hotzones") cancelHotzoneVariant();
    if (mode === "gestures") cancelGestureVariant();
    mode = nextMode;
  }

  function closeMode(): void {
    if (mode === "hotzones") cancelHotzoneVariant();
    if (mode === "gestures") cancelGestureVariant();
    mode = null;
  }

  function selectDisplay(id: string): void {
    selectedDisplayId = id;
    selectedHotzoneModifiers = [];
    cancelHotzoneVariant();
  }

  function setActionPreset(index: number): void {
    const preset = actionPresets[index] ?? actionPresets[0];
    const { slot, action } = ensureHotzoneActionTarget();
    action.kind = preset.kind;
    action.value = preset.value;
    if (preset.kind === "volume-adjust" || preset.kind === "brightness-adjust") {
      slot.cooldownMs = Math.min(slot.cooldownMs ?? settings.actionCooldownMs, 32);
    }
    shortcutDraft = preset.kind === "shortcut" ? (preset.value ?? "") : "";
    actionEditorOverrideKey = `${selectedDisplayId}:${selectedZone}:${activeTrigger}:${modifierId(selectedHotzoneModifiers)}`;
    actionEditorOverride = action;
    actionEditorRevision += 1;
    settings = { ...settings };
    persist();
  }

  function setActionValue(event: Event): void {
    ensureHotzoneActionTarget().action.value = (event.currentTarget as HTMLInputElement).value;
    persist();
  }

  function setActionShortcut(value: string): void {
    const { action: slotAction } = ensureHotzoneActionTarget();
    const shown = currentAction();
    const targets = shown === slotAction ? [shown] : [shown, slotAction];
    if (!value) {
      for (const item of targets) { item.kind = "none"; delete item.value; }
      shortcutDraft = "";
      shortcutError = "";
      // 动作编辑区被 {#key actionEditorRevision} 包住，块内表达式会被编译器整体 untrack，
      // 只有 key 变化才会重建。若不递增 revision，清空后下拉框仍显示"自定义快捷键"、
      // 录制器也仍然存在，与实际已清空的配置矛盾（已由 E2E 复现）。
      actionEditorRevision += 1;
      settings = { ...settings };
      persist();
      return;
    }
    if (settings.hotzones.some((zone) => zone.actions.some((slot) =>
      [slot.action, ...(slot.modifierActions ?? []).map((entry) => entry.action)]
        .some((action) => action !== shown && action.kind === "shortcut" && action.value === value)
    )) || settings.monitorProfiles.some((profile) => profile.hotzones.some((zone) => zone.actions.some((slot) =>
      [slot.action, ...(slot.modifierActions ?? []).map((entry) => entry.action)]
        .some((action) => action !== shown && action.kind === "shortcut" && action.value === value)
    )))) {
      shortcutError = format(ui("statusShortcutTaken"), { value });
      return;
    }
    shortcutError = "";
    for (const item of targets) { item.kind = "shortcut"; item.value = value; }
    // currentAction() 返回的对象不是响应式的，改它的属性界面读不到；
    // shortcutDraft 才是驱动录制器显示的状态。
    shortcutDraft = value;
    settings = { ...settings };
    persist();
  }

  function setTriggerTiming(field: "cooldownMs" | "hoverDelayMs", event: Event): void {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    if (!Number.isFinite(value)) return;
    const slot = ensureProfile().find((zone) => zone.id === selectedZone)!.actions.find((item) => item.trigger === activeTrigger)!;
    slot[field] = field === "cooldownMs"
      ? Math.min(5000, Math.max(10, Math.trunc(value)))
      : Math.min(3000, Math.max(0, Math.trunc(value)));
    persist();
  }

  function triggerLabel(trigger: TriggerKind): string {
    if (trigger === "slide-forward") return ["left", "right"].includes(selectedZone) ? ui("triggerSlideDown") : ui("triggerSlideRight");
    if (trigger === "slide-backward") return ["left", "right"].includes(selectedZone) ? ui("triggerSlideUp") : ui("triggerSlideLeft");
    return english ? triggerLabelsEn[trigger] : triggerLabels[trigger];
  }

  function triggerGroupLabel(group: typeof triggerGroups[number]): string {
    return english ? group.labelEn : group.label;
  }

  function zoneLabel(zone: HotzoneId): string {
    return english ? zoneLabelsEn[zone] : zoneLabels[zone];
  }

  function actionOptions(): { label: string; group: string }[] {
    return actionPresets.map((item) => ({ label: english ? item.labelEn : item.label, group: english ? item.groupEn : item.group }));
  }

  function actionLabel(action: HotzoneAction): string {
    const preset = actionPresets[presetIndex(action)];
    return preset ? (english ? preset.labelEn : preset.label) : ui("customAction");
  }

  function gestureName(gesture: GestureTemplate, translate = ui): string {
    return gestureDisplayName(gesture, translate);
  }

  function presetIndex(action: HotzoneAction): number {
    const exact = actionPresets.findIndex((item) => item.kind === action.kind && item.value === action.value);
    if (exact >= 0) return exact;
    const custom = actionPresets.findIndex((item) => item.kind === action.kind && item.value === undefined);
    return Math.max(0, custom);
  }

  function toggleEdge(edge: Edge): void {
    const profile = settings.edgeHide.monitorProfiles.find((item) => item.monitorId === selectedDisplayId);
    const current = profile?.edges ?? settings.edgeHide.edges;
    const edges = current.includes(edge) ? current.filter((item) => item !== edge) : [...current, edge];
    settings = {
      ...settings,
      edgeHide: {
        ...settings.edgeHide,
        monitorProfiles: profile
          ? settings.edgeHide.monitorProfiles.map((item) => item.monitorId === selectedDisplayId ? { ...item, edges } : item)
          : [...settings.edgeHide.monitorProfiles, { monitorId: selectedDisplayId, edges }]
      }
    };
    persist("edgeUpdated");
  }

  function addForeground(target: "hotzones" | "edge" | "gestures" | "drag"): void {
    if (!foregroundApp) { lastMessage = "foregroundMissing"; return; }
    const list = target === "hotzones" ? settings.pausedApps
      : target === "edge" ? settings.edgeHide.excludedApps
      : target === "drag" ? settings.windowDrag.pausedApps
      : settings.mouseGestures.pausedApps;
    if (!list.some((item) => item.toLowerCase() === foregroundApp.toLowerCase())) list.push(foregroundApp);
    persist(format(ui("statusAppAdded"), { app: foregroundApp }));
  }

  function removeApp(target: "hotzones" | "edge" | "gestures" | "drag", app: string): void {
    if (target === "hotzones") settings.pausedApps = settings.pausedApps.filter((item) => item !== app);
    else if (target === "edge") settings.edgeHide.excludedApps = settings.edgeHide.excludedApps.filter((item) => item !== app);
    else if (target === "drag") settings.windowDrag.pausedApps = settings.windowDrag.pausedApps.filter((item) => item !== app);
    else settings.mouseGestures.pausedApps = settings.mouseGestures.pausedApps.filter((item) => item !== app);
    persist();
  }

  function currentGesture(): GestureTemplate {
    return settings.mouseGestures.gestures.find((gesture) => gesture.id === selectedGestureId)
      ?? settings.mouseGestures.gestures[0];
  }

  function currentGestureAction(): HotzoneAction {
    const gesture = currentGesture();
    if (!selectedGestureModifiers.length) return gesture.action;
    return (gesture.modifierActions ?? []).find((item) => sameModifiers(item.modifiers, selectedGestureModifiers))?.action
      ?? { kind: "none" };
  }

  function ensureGestureActionTarget(): HotzoneAction {
    const gesture = currentGesture();
    gesture.modifierActions ??= [];
    if (!selectedGestureModifiers.length) return gesture.action;
    const variant = gesture.modifierActions.find((item) => sameModifiers(item.modifiers, selectedGestureModifiers));
    if (!variant) {
      selectedGestureModifiers = [];
      return gesture.action;
    }
    return variant.action;
  }

  function beginGestureVariant(): void {
    const variants = currentGesture().modifierActions ?? [];
    if (variants.length >= MAX_MODIFIER_VARIANTS) {
      gestureModifierError = format(ui("statusVariantLimit"), { max: MAX_MODIFIER_VARIANTS });
      return;
    }
    gestureModifierDraft = [];
    gestureModifierError = "";
  }

  function commitGestureVariant(modifiers: ModifierKey[]): void {
    gestureModifierDraft = [...modifiers];
    if (!modifiers.length) {
      gestureModifierError = "";
      return;
    }
    const gesture = currentGesture();
    const result = addModifierVariant(gesture.modifierActions, modifiers);
    if (!result.ok) {
      gestureModifierError = result.message;
      return;
    }
    gesture.modifierActions = result.variants;
    selectedGestureModifiers = result.modifiers;
    gestureModifierDraft = null;
    gestureModifierError = "";
    settings = { ...settings };
    persist("gestureVariantAdded");
  }

  function cancelGestureVariant(): void {
    gestureModifierDraft = null;
    gestureModifierError = "";
  }

  function removeGestureVariant(modifiers: ModifierKey[]): void {
    const gesture = currentGesture();
    gesture.modifierActions = (gesture.modifierActions ?? []).filter((item) => !sameModifiers(item.modifiers, modifiers));
    selectedGestureModifiers = [];
    persist("gestureVariantDeleted");
  }

  function createGesture(): void {
    if (settings.mouseGestures.gestures.length >= MAX_GESTURE_TEMPLATES) {
      lastMessage = format(ui("statusGestureLimitCount"), { max: MAX_GESTURE_TEMPLATES });
      return;
    }
    const gesture: GestureTemplate = {
      id: `gesture-${globalThis.crypto?.randomUUID?.() ?? Date.now()}`,
      name: ui("newGesture"),
      enabled: true,
      builtin: false,
      mode: "action",
      action: { kind: "none" },
      modifierActions: [],
      samples: []
    };
    settings.mouseGestures.gestures = [...settings.mouseGestures.gestures, gesture];
    selectGesture(gesture.id);
    gestureConflict = "";
    persist("gestureCreated");
  }

  function selectGesture(id: string): void {
    selectedGestureId = id;
    selectedGestureModifiers = [];
    cancelGestureVariant();
    gestureConflict = findGestureConflict(currentGesture());
    // 切换手势时同步动作编辑区的 draft（此处是唯一需要读 action 的时机）
    syncGestureActionDraft();
  }

  function syncGestureActionDraft(): void {
    const action = currentGestureAction();
    gestureActionDraft = { ...action };
    gestureShortcutDraft = action.kind === "shortcut" ? (action.value ?? "") : "";
    gestureShortcutError = "";
  }

  function selectFirstActionGesture(): void {
    const gesture = settings.mouseGestures.gestures.find((item) => item.mode === "action");
    if (gesture) selectGesture(gesture.id);
  }

  function selectScreenshotGesture(): void {
    const gesture = settings.mouseGestures.gestures.find((item) => item.mode === "region-screenshot");
    if (gesture) selectGesture(gesture.id);
  }

  function recordGesture(sample: GesturePoint[]): void {
    const gesture = currentGesture();
    const normalized = resampleGesture(sample);
    if (!normalized.length) { gestureConflict = ui("gestureTooShort"); return; }
    gesture.samples = [...gesture.samples.slice(-7), normalized];
    gestureConflict = findGestureConflict(gesture);
    persist(gesture.samples.length < 3 ? format(ui("statusGestureSampleSaved"), { count: gesture.samples.length }) : "gestureSamplesUpdated");
  }

  function renameGesture(event: Event): void {
    const name = Array.from((event.currentTarget as HTMLInputElement).value.trim()).slice(0, 40).join("");
    const gesture = currentGesture();
    if (!name && !gesture.builtin) return;
    gesture.name = name || undefined;
    persist();
  }

  function duplicateGesture(): void {
    const source = currentGesture();
    const copy: GestureTemplate = {
      ...source,
      id: `gesture-${globalThis.crypto?.randomUUID?.() ?? Date.now()}`,
      name: format(ui("gestureCopyName"), { name: gestureName(source) }).slice(0, 40),
      builtin: false,
      action: { ...source.action },
      modifierActions: (source.modifierActions ?? []).map((item) => ({ modifiers: [...item.modifiers], action: { ...item.action } })),
      samples: source.samples.map((sample) => sample.map((point) => ({ ...point })))
    };
    settings.mouseGestures.gestures = [...settings.mouseGestures.gestures, copy];
    selectGesture(copy.id);
    persist("gestureDuplicateCreated");
  }

  function deleteGesture(): void {
    const gesture = currentGesture();
    if (gesture.builtin || !window.confirm(format(ui("gestureDeleteConfirm"), { name: gestureName(gesture) }))) return;
    settings.mouseGestures.gestures = settings.mouseGestures.gestures.filter((item) => item.id !== gesture.id);
    selectGesture(settings.mouseGestures.gestures[0]?.id ?? "");
    gestureConflict = "";
    persist("gestureDeleted");
  }

  function clearGestureSamples(): void {
    currentGesture().samples = [];
    gestureConflict = "";
    persist("gestureSamplesCleared");
  }

  function toggleGestureEnabled(event: Event): void {
    currentGesture().enabled = (event.currentTarget as HTMLInputElement).checked;
    persist();
  }

  function setGestureActionPreset(index: number): void {
    const preset = actionPresets[index] ?? actionPresets[0];
    const action = ensureGestureActionTarget();
    action.kind = preset.kind;
    action.value = preset.value;
    gestureActionDraft = { kind: preset.kind, value: preset.value };
    gestureShortcutDraft = preset.kind === "shortcut" ? (preset.value ?? "") : "";
    gestureShortcutError = "";
    settings = { ...settings };
    persist();
  }

  function setGestureActionValue(event: Event): void {
    ensureGestureActionTarget().value = (event.currentTarget as HTMLInputElement).value;
    persist();
  }

  // 与触发角对齐：手势的自定义快捷键同样用录制器，而不是手输文本。
  function setGestureActionShortcut(value: string): void {
    const action = ensureGestureActionTarget();
    gestureShortcutError = "";
    if (!value) {
      action.kind = "none";
      delete action.value;
      gestureShortcutDraft = "";
      gestureActionDraft = { kind: "none" };
    } else {
      action.kind = "shortcut";
      action.value = value;
      gestureShortcutDraft = value;
      gestureActionDraft = { kind: "shortcut", value };
    }
    settings = { ...settings };
    persist();
  }

  function gesturePath(points: GesturePoint[]): string {
    return points.map((point, index) => `${index ? "L" : "M"} ${point.x * 100} ${point.y * 100}`).join(" ");
  }

  function findGestureConflict(target: GestureTemplate): string {
    const sample = target.samples.at(-1);
    if (!sample) return "";
    let best: { name: string; score: number } | null = null;
    for (const gesture of settings.mouseGestures.gestures) {
      if (gesture.id === target.id || !gesture.enabled) continue;
      for (const other of gesture.samples) {
        const score = gestureSimilarity(sample, other);
        if (!best || score > best.score) best = { name: gestureName(gesture), score };
      }
    }
    return best && best.score >= 0.88 ? format(ui("gestureSimilarity"), { name: best.name, score: Math.round(best.score * 100) }) : "";
  }

  function markHelperReady(): void {
    helperError = "";
    helperWasReady = true;
    helperRecoveryFailed = false;
    if (recoveringHelper) {
      recoveringHelper = false;
      lastMessage = "helperRecovered";
    }
    if (helperStabilityTimer) clearTimeout(helperStabilityTimer);
    helperStabilityTimer = setTimeout(() => {
      helperStabilityTimer = null;
      if (helperWasReady && helperStatus === "connected") helperRecovery.markStable();
    }, HELPER_STABILITY_MS);
  }

  function scheduleHelperRecovery(): void {
    if (switchingPrivilege || helperRecoveryTimer || helperRecoveryFailed || stopping || upgradingHelper) return;
    if (helperRecovery.requestRecovery() === "fail") {
      failHelperRecovery("helperRecoveryStopped");
      return;
    }
    recoveringHelper = true;
    lastMessage = "helperRecovering";
    helperRecoveryTimer = setTimeout(async () => {
      helperRecoveryTimer = null;
      try {
        const result = await host.startHelper();
        if (!result.ok) {
          failHelperRecovery(result.error ?? "helperRecoveryFailed");
          return;
        }
        lastMessage = "helperRestarting";
        helper.connect();
      } catch (error) {
        failHelperRecovery(format(ui("helperRecoveryFailedDetail"), { error: error instanceof Error ? error.message : String(error) }));
      }
    }, 500);
  }

  function failHelperRecovery(message: string): void {
    recoveringHelper = false;
    helperRecoveryFailed = true;
    helper.disconnect();
    helperError = message;
    lastMessage = message;
  }

  function resetHelperRecovery(): void {
    clearHelperRecoveryTimers();
    helperRecovery.reset();
    helperWasReady = false;
    recoveringHelper = false;
    helperRecoveryFailed = false;
    helperError = "";
  }

  function clearHelperRecoveryTimers(): void {
    if (helperRecoveryTimer) clearTimeout(helperRecoveryTimer);
    if (helperStabilityTimer) clearTimeout(helperStabilityTimer);
    helperRecoveryTimer = null;
    helperStabilityTimer = null;
  }

  async function startHelper(): Promise<boolean> {
    const result = await host.startHelper();
    lastMessage = result.ok
      ? result.alreadyRunning ? "helperStarting" : "helperStartedConnecting"
      : result.error ?? "helperStartFailed";
    if (result.ok) helper.connect();
    else helperError = lastMessage;
    return result.ok;
  }
  async function switchHelperPrivilege(): Promise<void> {
    if (!host.setHelperElevation || switchingPrivilege || starting || stopping) return;
    const desired = !helperElevated;
    switchingPrivilege = true;
    privilegeNotice = "";
    resetHelperRecovery();
    if (!(await applyNow())) { switchingPrivilege = false; return; }
    helper.disconnect();
    try {
      const result = await host.setHelperElevation(desired);
      helperElevated = result.elevated;
      if (result.ok) {
        privilegeNotice = result.warning ?? "";
        helper.sendConfig(settings);
        helper.connect();
      } else {
        helperError = result.error ?? "adminSwitchFailed";
      }
    } catch (error) {
      helperError = error instanceof Error ? error.message : String(error);
    } finally { switchingPrivilege = false; }
  }
  function openHelperPage(target: "repository" | "release"): void {
    const url = helperInstallState[target];
    if (url) host.openExternal(url);
  }
  function stopHelper(): void {
    stopping = true;
    resetHelperRecovery();
    helper.stop();
    lastMessage = "helperStopping";
  }
  async function setPowerEnabled(enabled: boolean): Promise<void> {
    if (switchingPrivilege) return;
    settings.enabled = enabled;
    if (!(await applyNow())) {
      settings.enabled = !enabled;
      return;
    }
    if (enabled) {
      resetHelperRecovery();
      starting = true;
      const started = await startHelper();
      starting = false;
      if (!started) {
        settings.enabled = false;
        await savePreparedSettings(normalizeSettings(settings));
      }
    } else {
      stopHelper();
    }
  }
  function togglePower(event: Event): void {
    void setPowerEnabled((event.currentTarget as HTMLInputElement).checked);
  }
  function requestHelperUpgrade(actualVersion: unknown): void {
    if (upgradingHelper) return;
    if (helperUpgradeAttempts >= 4) {
      lastMessage = "helperUpgradeFailed";
      return;
    }
    helperUpgradeAttempts += 1;
    helperRecovery.reset();
    upgradingHelper = true;
    const current = typeof actualVersion === "string" ? actualVersion : ui("helperVersionLegacy");
    lastMessage = format(ui("helperUpgradeProgress"), { current, expected: expectedHelperVersion ?? "" });
    helper.stop();
  }
  function scheduleHelperUpgradeRestart(): void {
    if (!upgradingHelper || helperUpgradeTimer) return;
    helperUpgradeTimer = setTimeout(async () => {
      helperUpgradeTimer = null;
      const result = await host.startHelper();
      upgradingHelper = false;
      if (result.ok) {
        lastMessage = "helperUpgradeStarted";
        helper.connect();
      } else {
        lastMessage = result.error ?? "helperUpgradeStartFailed";
      }
    }, 500);
  }
  async function applyNow(): Promise<boolean> {
    const prepared = prepareSettingsUpdate(settings);
    settings = prepared.editable;
    const result = await settingsApply.applyNow(prepared.normalized);
    if (!handleSettingsCommit(result)) return false;
    lastMessage = result.sent ? "configSent" : "savedWaiting";
    return true;
  }
  async function resetSettings(): Promise<void> {
    settings = normalizeSettings(defaultSettings);
    selectedHotzoneModifiers = [];
    cancelHotzoneVariant();
    selectGesture(settings.mouseGestures.gestures[0]?.id ?? "");
    const result = await settingsApply.applyNow(settings);
    if (!handleSettingsCommit(result)) return;
    lastMessage = result.sent ? "configResetSent" : "configResetDone";
  }
  async function exportSettings(): Promise<void> {
    try {
      lastMessage = await host.exportSettings(normalizeSettings(settings)) ? "configExported" : "configExportCancelled";
    } catch (error) {
      lastMessage = format(ui("configExportFailed"), { error: error instanceof Error ? error.message : String(error) });
    }
  }
  async function importSettings(): Promise<void> {
    try {
      const imported = await host.importSettings();
      if (!imported) { lastMessage = "configImportCancelled"; return; }
      settings = normalizeSettings(imported);
      selectedHotzoneModifiers = [];
      cancelHotzoneVariant();
      selectGesture(settings.mouseGestures.gestures[0]?.id ?? "");
      if (!(await savePreparedSettings(settings))) return;
      helper.sendConfig(settings);
      lastMessage = "configImported";
    } catch (error) {
      lastMessage = format(ui("configImportFailed"), { error: error instanceof Error ? error.message : ui("configInvalid") });
    }
  }
  async function copyDiagnostics(): Promise<void> {
    try {
      await navigator.clipboard.writeText(JSON.stringify(await host.diagnostics(), null, 2));
      lastMessage = "diagnosticsCopied";
    } catch (error) {
      lastMessage = format(ui("diagnosticsFailed"), { error: error instanceof Error ? error.message : String(error) });
    }
  }
  function cloneHotzones(zones: HotzoneSetting[]): HotzoneSetting[] { return zones.map((zone) => ({ ...zone, actions: zone.actions.map((item) => ({ trigger: item.trigger, action: { ...item.action }, modifierActions: (item.modifierActions ?? []).map((variant) => ({ modifiers: [...variant.modifiers], action: { ...variant.action } })), cooldownMs: item.cooldownMs, hoverDelayMs: item.hoverDelayMs })) })); }
  function hasSecureBridge(): boolean { return host.kind === "desktop"; }
</script>

<div class:app-disabled={!settings.enabled} class="app-shell">
  <header class="topbar">
    <div class="brand"><img src="app-icon.png" alt="" /><strong>{ui("brandName")}</strong></div>
    <div class:connected={helperStatus === "connected"} class="connection"><i></i>{helperStatus === "connected" ? ui("connected") : helperStatus === "connecting" ? ui("connecting") : ui("disconnected")}<span>{displayReady ? (runtimeSummary || ui("statusRuntimeIdle")) : helperStatus === "connecting" ? ui("connecting") : ui("displayPreview")}</span></div>
    <label class="master">{ui("masterState")} <input checked={settings.enabled} disabled={starting || stopping || switchingPrivilege} on:change={togglePower} type="checkbox" /><span></span></label>
    <button aria-label={theme === "dark" ? ui("themeLight") : ui("themeDark")} class="theme-toggle" on:click={toggleTheme} title={theme === "dark" ? ui("themeLight") : ui("themeDark")} type="button">
      <svg class="icon-sun" fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.8" viewBox="0 0 24 24"><circle cx="12" cy="12" r="4.2" /><path d="M12 2.5v2.4M12 19.1v2.4M2.5 12h2.4M19.1 12h2.4M5 5l1.7 1.7M17.3 17.3 19 19M19 5l-1.7 1.7M6.7 17.3 5 19" /></svg>
      <svg class="icon-moon" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" viewBox="0 0 24 24"><path d="M20.4 13.2A8.4 8.4 0 1 1 10.8 3.6a6.8 6.8 0 0 0 9.6 9.6z" /></svg>
    </button>
  </header>

  <main class:drawer-open={mode !== null} class:gesture-open={mode === "gestures"} class="workspace">
    <section class="scene" aria-label={ui("displayAndPreview")}>
      <div class="scene-tools">
        <button class:ready={displayReady} class:pending={!displayReady && helperStatus === "connecting"} class="display-picker" on:click={() => helper.ping()} type="button"><i class:ready={displayReady} class:pending={!displayReady && helperStatus === "connecting"}></i><span class="display-status">{displayReady ? `${displays.length}${ui("displays")}` : helperStatus === "connecting" ? ui("connecting") : ui("displayPreview")}</span><span class="display-action">{ui("checkConnection")}</span></button>
        {#if mode === "hotzones"}<p>{ui("chooseArea")}</p>{:else if mode === "edge-hide"}<p>{ui("chooseEdge")}</p>{:else if mode === "gestures"}<p>{ui("drawGesture")}</p>{:else}<p>{ui("chooseFeature")}</p>{/if}
      </div>

      <MonitorStage
        {english}
        {displays}
        {mode}
        {selectedDisplayId}
        {selectedZone}
        displayReady={displayReady}
        edgeHideEnabled={settings.edgeHide.enabled}
        edgeHideEdges={currentDisplayEdgeHideEdges}
        hotzonesEnabled={settings.hotzonesEnabled}
        hotzones={currentDisplayHotzones}
        onSelectDisplay={selectDisplay}
        onSelectZone={selectZone}
        onToggleEdge={toggleEdge}
      />

      <nav class="mode-nav" aria-label={ui("featureNavigation")}>
        <button class:active={mode === "power"} on:click={() => toggleMode("power")} type="button"><span class="nav-icon power-icon"></span><b>{ui("power")}</b></button>
        <button class:active={mode === "hotzones"} on:click={() => toggleMode("hotzones")} type="button"><span class="nav-icon corner-icon"></span><b>{ui("hotzones")}</b></button>
        <button class:active={mode === "edge-hide"} on:click={() => toggleMode("edge-hide")} type="button"><span class="nav-icon edge-icon"></span><b>{ui("edgeHide")}</b></button>
        <button class:active={mode === "gestures"} on:click={() => toggleMode("gestures")} type="button"><span class="nav-icon gesture-icon"></span><b>{ui("gestures")}</b></button>
        <button class:active={mode === "more"} on:click={() => toggleMode("more")} type="button"><span class="nav-icon more-icon"></span><b>{ui("more")}</b></button>
      </nav>
    </section>

    {#if mode !== null}
      <aside class:gesture-drawer={mode === "gestures"} class="drawer" in:fly={{ x: 28, duration: 220 }}>
        <header class="drawer-head">
          <div><span>{mode === "power" ? ui("power") : mode === "hotzones" ? ui("hotzones") : mode === "edge-hide" ? ui("edgeHide") : mode === "gestures" ? ui("gestures") : ui("moreSettings")}</span><p>{mode === "hotzones" ? `${zoneLabel(selectedZone)} · S${Math.max(0, displays.findIndex((item) => item.id === selectedDisplayId)) + 1}` : mode === "edge-hide" ? windowEnhancementTab === "edge" ? ui("edgeSubtitle") : ui("dragSubtitle") : mode === "gestures" ? ui("gestureSubtitle") : mode === "more" ? ui("moreSubtitle") : ui("powerSubtitle")}</p></div>
          <button aria-label={ui("close")} class="close-drawer" on:click={closeMode} type="button">×</button>
        </header>

        {#key mode}
          <div class="drawer-body" in:fly={{ y: 10, duration: 170 }}>
            {#if mode === "hotzones"}
              <div class="feature-intro hotzone-master-intro">
                <div class="feature-copy-stack">
                  <span>{ui("hotzoneMaster")}</span>
                  <h2>{ui("hotzoneTitle")}</h2>
                  <p>{ui("hotzoneDescription")}</p>
                </div>
                <div class="feature-master-toggle"><b class:on={settings.hotzonesEnabled}>{settings.hotzonesEnabled ? ui("enabled") : ui("disabled")}</b><label class="mini-switch"><input aria-label={ui("enableHotzones")} bind:checked={settings.hotzonesEnabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
              </div>
              <div class="feature-settings-head"><span>{ui("hotzoneSettings")}</span><strong>{settings.hotzonesEnabled ? ui("unifiedOn") : ui("keepConfig")}</strong></div>
              <div class="feature-settings-body" class:off={!settings.hotzonesEnabled} inert={!settings.hotzonesEnabled}>
                <div class="trigger-tabs">
                  {#each triggerGroups as group}
                    <button class:active={group.items.includes(activeTrigger)} disabled={group.items.includes("slide-forward") && !["top", "right", "bottom", "left"].includes(selectedZone)} on:click={() => { activeTrigger = group.items[0]; selectedHotzoneModifiers = []; cancelHotzoneVariant(); }} type="button">{triggerGroupLabel(group)}</button>
                  {/each}
                </div>
                {#if activeTrigger.startsWith("wheel") || activeTrigger.startsWith("slide")}
                  <div class="direction-tabs">
                    {#each triggerGroups.find((group) => group.items.includes(activeTrigger))?.items ?? [] as trigger}
                      <button class:active={activeTrigger === trigger} on:click={() => { activeTrigger = trigger; selectedHotzoneModifiers = []; cancelHotzoneVariant(); }} type="button">{triggerLabel(trigger)}</button>
                    {/each}
                  </div>
                {/if}
                <div class="modifier-variants">
                  <div class="variant-tabs">
                    <button class:active={!selectedHotzoneModifiers.length && hotzoneModifierDraft === null} on:click={() => selectHotzoneVariant([])} type="button">{ui("direct")}</button>
                    {#each currentHotzoneModifierActions as variant}
                      <button class:active={hotzoneModifierDraft === null && sameModifiers(selectedHotzoneModifiers, variant.modifiers)} on:click={() => selectHotzoneVariant(variant.modifiers)} type="button">{modifierLabel(variant.modifiers)}</button>
                    {/each}
                    <span class="variant-spacer"></span>
                    <button aria-label={ui("addHotzoneVariant")} class="variant-add" disabled={currentHotzoneModifierActions.length >= MAX_MODIFIER_VARIANTS} on:click={beginHotzoneVariant} title={ui("newVariant")} type="button">＋</button>
                    {#if selectedHotzoneModifiers.length && hotzoneModifierDraft === null}<button aria-label={ui("deleteVariant")} class="variant-delete" on:click={() => removeHotzoneVariant(selectedHotzoneModifiers)} title={ui("deleteHotzoneVariant")} type="button">×</button>{/if}
                  </div>
                  {#if hotzoneModifierDraft !== null}
                    <div class="modifier-draft"><ModifierRecorder english={english} label={ui("recordHotzoneVariant")} value={hotzoneModifierDraft} onChange={commitHotzoneVariant} /><button aria-label={ui("cancelHotzoneVariant")} class="variant-delete" on:click={cancelHotzoneVariant} title={ui("cancel")} type="button">×</button></div>
                  {/if}
                  {#if hotzoneModifierError}<p class="modifier-error" role="alert">{hotzoneModifierError}</p>{/if}
                </div>
                {#if hotzoneModifierDraft === null}
                {#key `${selectedDisplayId}:${selectedZone}:${activeTrigger}:${modifierId(selectedHotzoneModifiers)}:${actionEditorRevision}`}
                <div class="action-editor">
                  <label><span>{ui("action")}</span><ActionPicker options={actionOptions()} value={presetIndex(currentAction())} onSelect={setActionPreset} /></label>
                  {#if currentAction().kind === "open-command" || (currentAction().kind === "shortcut" && !actionPresets.some((item) => item.kind === "shortcut" && item.value !== undefined && item.value === currentAction().value))}
                    <label><span>{currentAction().kind === "shortcut" ? ui("shortcut") : ui("command")}</span>{#if currentAction().kind === "shortcut"}<ShortcutRecorder english={english} label={ui("recordShortcut")} value={shortcutDraft} onChange={setActionShortcut} />{#if shortcutError}<p class="modifier-error" role="alert">{shortcutError}</p>{/if}{:else}<input value={currentAction().value ?? ""} on:input={setActionValue} placeholder={ui("enterValue")} />{/if}</label>
                  {/if}
                  <div class="action-state"><i class:enabled={currentAction().kind !== "none"}></i><div><strong>{triggerLabel(activeTrigger)}</strong><p>{currentAction().kind === "none" ? ui("noAction") : actionLabel(currentAction())}</p></div></div>
                </div>
                {/key}
                {/if}
                <div class:single={activeTrigger !== "hover"} class="timing">{#if activeTrigger === "hover"}<label><span>{ui("hoverDelay")}</span><div><input value={currentTriggerSlot().hoverDelayMs ?? settings.hoverDelayMs} min="0" max="3000" on:input={(event) => setTriggerTiming("hoverDelayMs", event)} type="number" /><em>ms</em></div></label>{/if}<label><span>{ui("cooldown")}</span><div><input value={currentTriggerSlot().cooldownMs ?? settings.actionCooldownMs} min="10" max="5000" on:input={(event) => setTriggerTiming("cooldownMs", event)} type="number" /><em>ms</em></div></label></div>
                <div class="subhead" style="margin-top:18px"><div><h2>{ui("hotzoneParameters")}</h2><p>{ui("hotzoneParametersDescription")}</p></div></div>
                <div class="form-grid"><label><span>{ui("edgeSize")}</span><div><input bind:value={settings.edgeSize} min="2" max="48" on:input={() => persist()} type="number" /><em>px</em></div></label></div>
                <div class="list-section"><div class="subhead"><div><h2>{ui("pausedApps")}</h2><p>{ui("foreground")}{foregroundApp || ui("noForeground")}</p></div><button class="quiet" on:click={() => addForeground("hotzones")} type="button">+ {ui("addApp")}</button></div><div class="app-list">{#each settings.pausedApps as app}<div><span>{app}</span><button aria-label={format(ui("removeApp"), { app })} on:click={() => removeApp("hotzones", app)} type="button">×</button></div>{:else}<p class="empty">{ui("noPausedApps")}</p>{/each}</div></div>
              </div>
            {:else if mode === "edge-hide"}
              <div class="window-tabs"><button class:active={windowEnhancementTab === "edge"} on:click={() => { windowEnhancementTab = "edge"; }} type="button">{ui("edgeTab")}</button><button class:active={windowEnhancementTab === "drag"} on:click={() => { windowEnhancementTab = "drag"; }} type="button">{ui("dragTab")}</button></div>
              {#if windowEnhancementTab === "edge"}
                <div class="feature-intro edge-master-intro" on:mouseleave={(event) => handleFeatureTutorialMouseLeave("edge-hide", event)} on:focusout={(event) => handleFeatureTutorialFocusOut("edge-hide", event)} role="group">
                  <div class="feature-copy-stack"><span>{ui("edgeMaster")}</span><div class="feature-title-row"><h2>{ui("edgeTitle")}</h2><div class="feature-help" on:mouseenter={() => showFeatureTutorial("edge-hide")} on:focusin={() => showFeatureTutorial("edge-hide")} role="group"><button aria-controls="edge-hide-tutorial" aria-describedby={activeFeatureTutorial === "edge-hide" ? "edge-hide-tutorial" : undefined} aria-expanded={activeFeatureTutorial === "edge-hide"} aria-label={ui("viewEdgeHideTutorial")} class="feature-help-button" on:click={() => showFeatureTutorial("edge-hide")} type="button">?</button>{#if activeFeatureTutorial === "edge-hide"}<section class="feature-help-card" id="edge-hide-tutorial" role="tooltip"><strong>{ui("edgeHideTutorial")}</strong><div aria-hidden="true" class="tutorial-demo"><div class="tutorial-screen"><div class="tutorial-edge-window"><i></i><span></span></div><div class="tutorial-cursor"><i></i></div></div></div><p>{ui("edgeHideTutorialBody")}</p></section>{/if}</div></div><p>{ui("edgeDescription")}</p></div>
                  <div class="feature-master-toggle"><b class:on={settings.edgeHide.enabled}>{settings.edgeHide.enabled ? ui("enabled") : ui("disabled")}</b><label class="mini-switch"><input aria-label={ui("enableEdge")} bind:checked={settings.edgeHide.enabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                </div>
                <div class="feature-settings-head"><span>{ui("edgeSettings")}</span><strong>{settings.edgeHide.enabled ? ui("unifiedOn") : ui("edgeEnabled")}</strong></div>
                <div class="feature-settings-body" class:off={!settings.edgeHide.enabled} inert={!settings.edgeHide.enabled}>
                <div class="setting-title edge-preview-setting"><div><h2>{ui("snapHint")}</h2><p>{ui("snapHintDescription")}</p></div><label class="mini-switch"><input aria-label={ui("snapHint")} bind:checked={settings.edgeHide.showPreview} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                <div class="setting-title edge-restore-hint-setting"><div><h2>{ui("expansionOutline")}</h2><p>{ui("expansionOutlineDescription")}</p></div><label class="mini-switch"><input aria-label={ui("expansionOutline")} bind:checked={settings.edgeHide.showRestoreHint} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                <div class="setting-title edge-foreground-setting"><div><h2>{ui("keepExpandedWhenForeground")}</h2><p>{ui("keepExpandedWhenForegroundDescription")}</p></div><label class="mini-switch"><input aria-label={ui("keepExpandedWhenForeground")} bind:checked={settings.edgeHide.keepExpandedWhenForeground} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                <div class="edge-trigger-heading"><h2>{ui("collapseTriggers")}</h2><p>{ui("collapseTriggersHint")}</p></div>
                <div class="edge-trigger-methods">
                  <section class:off={!settings.edgeHide.distanceTriggerEnabled} class="edge-trigger-method">
                    <div class="setting-title"><div><h3>{ui("nearEdge")}</h3><p>{ui("nearEdgeDescription")}</p></div><label class="mini-switch"><input aria-label={ui("enableNearEdge")} bind:checked={settings.edgeHide.distanceTriggerEnabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                    <label class="trigger-value"><span>{ui("edgeDistance")}</span><div><input bind:value={settings.edgeHide.triggerDistance} disabled={!settings.edgeHide.distanceTriggerEnabled} min="4" max="96" on:input={() => persist()} type="number" /><em>px</em></div></label>
                  </section>
                  <section class:off={!settings.edgeHide.ratioTriggerEnabled} class="edge-trigger-method">
                    <div class="setting-title"><div><h3>{ui("moveOutRatio")}</h3><p>{ui("moveOutRatioDescription")}</p></div><label class="mini-switch"><input aria-label={ui("enableMoveOutRatio")} bind:checked={settings.edgeHide.ratioTriggerEnabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                    <label class="trigger-value"><span>{ui("windowRatio")}</span><div><input bind:value={settings.edgeHide.triggerRatio} disabled={!settings.edgeHide.ratioTriggerEnabled} min="1" max="100" on:input={() => persist()} type="number" /><em>%</em></div></label>
                  </section>
                </div>
                <div class="form-grid"><label><span>{ui("stripSize")}</span><div><input bind:value={settings.edgeHide.stripSize} min="4" max="64" on:input={() => persist()} type="number" /><em>px</em></div></label><label><span>{ui("collapseDelay")}</span><div><input bind:value={settings.edgeHide.collapseDelayMs} min="0" max="5000" on:input={() => persist()} type="number" /><em>ms</em></div></label><label><span>{ui("restoreDelay")}</span><div><input bind:value={settings.edgeHide.restoreDelayMs} min="0" max="5000" on:input={() => persist()} type="number" /><em>ms</em></div></label></div>
                <div class="list-section"><div class="subhead"><div><h2>{ui("excludedApps")}</h2><p>{ui("foreground")}{foregroundApp || ui("noForeground")}</p></div><button class="quiet" on:click={() => addForeground("edge")} type="button">+ {ui("addApp")}</button></div><div class="app-list">{#each settings.edgeHide.excludedApps as app}<div><span>{app}</span><button aria-label={format(ui("removeApp"), { app })} on:click={() => removeApp("edge", app)} type="button">×</button></div>{:else}<p class="empty">{ui("noExcludedApps")}</p>{/each}</div></div>
                </div>
              {:else}
                <div class="feature-intro drag-master-intro">
                  <div class="feature-copy-stack"><span>{ui("dragMaster")}</span><div class="feature-title-row"><h2>{ui("dragTitle")}</h2></div><p>{ui("dragDescription")}</p></div>
                  <div class="feature-master-toggle"><b class:on={settings.windowDrag.enabled}>{settings.windowDrag.enabled ? ui("enabled") : ui("disabled")}</b><label class="mini-switch"><input aria-label={ui("enableDrag")} bind:checked={settings.windowDrag.enabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                </div>
                <div class="feature-settings-head"><span>{ui("dragSettings")}</span><strong>{settings.windowDrag.enabled ? ui("unifiedOn") : ui("edgeEnabled")}</strong></div>
                <div class="feature-settings-body" class:off={!settings.windowDrag.enabled} inert={!settings.windowDrag.enabled}>
                <div class="drag-bindings">
                  <div><span><b>{ui("moveWindow")}</b><small>{ui("defaultAltLeft")}</small></span><ModifierRecorder english={english} label={ui("recordMoveModifiers")} value={settings.windowDrag.moveModifiers} onChange={(value) => { settings.windowDrag.moveModifiers = value; persist(); }} /><select aria-label={ui("moveWindowButton")} bind:value={settings.windowDrag.moveButton} on:change={() => persist()}><option value="left">{ui("leftButton")}</option><option value="right">{ui("rightButton")}</option><option value="middle">{ui("middleButton")}</option><option value="x1">{ui("sideButton1")}</option><option value="x2">{ui("sideButton2")}</option></select></div>
                  <div><span><b>{ui("resizeWindow")}</b><small>{ui("defaultAltRight")}</small></span><ModifierRecorder english={english} label={ui("recordResizeModifiers")} value={settings.windowDrag.resizeModifiers} onChange={(value) => { settings.windowDrag.resizeModifiers = value; persist(); }} /><select aria-label={ui("resizeWindowButton")} bind:value={settings.windowDrag.resizeButton} on:change={() => persist()}><option value="left">{ui("leftButton")}</option><option value="right">{ui("rightButton")}</option><option value="middle">{ui("middleButton")}</option><option value="x1">{ui("sideButton1")}</option><option value="x2">{ui("sideButton2")}</option></select></div>
                </div>
                {#if windowDragBindingConflict}<p class="gesture-warning">{ui("moveResizeConflict")}</p>{/if}
                <div class="list-section drag-paused-apps"><div class="subhead"><div><h2>{ui("pausedDragApps")}</h2><p>{ui("foreground")}{foregroundApp || ui("noForeground")}</p></div><button class="quiet" on:click={() => addForeground("drag")} type="button">+ {ui("addCurrentApp")}</button></div><div class="app-list">{#each settings.windowDrag.pausedApps as app}<div><span>{app}</span><button aria-label={format(ui("removeApp"), { app })} on:click={() => removeApp("drag", app)} type="button">×</button></div>{:else}<p class="empty">{ui("noPausedDragApps")}</p>{/each}</div></div>
                </div>
                <div class="setting-title pin-setting"><div><h2>{ui("pinTooltip")}</h2><p>{ui("pinTooltipDescription")}</p></div><label class="mini-switch"><input aria-label={ui("enablePin")} bind:checked={settings.topmostPin.enabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
              {/if}
            {:else if mode === "gestures"}
              <div class="gesture-intro">
                <div><span>{ui("gestureTitle")}</span><h2>{ui("gestureHeading")}</h2><p>{ui("gestureDescription")}</p></div>
                <label class="mini-switch"><input aria-label={ui("enableGesture")} bind:checked={settings.mouseGestures.enabled} on:change={() => persist()} type="checkbox" /><span></span></label>
              </div>

              <div class="gesture-capabilities" aria-label={ui("gestureOverview")}>
                <button class:active={activeGesture.mode === "action" && activeGesture.builtin} on:click={selectFirstActionGesture} type="button"><i class="capability-mark action-mark">↗</i><span><b>{ui("actionGestures")}</b><small>{ui("actionGesturesDescription")}</small></span><em>{settings.mouseGestures.gestures.filter((item) => item.mode === "action" && item.builtin).length}{ui("gestureBuiltinCount")}</em></button>
                <button class:active={activeGesture.mode === "action" && !activeGesture.builtin} on:click={createGesture} type="button"><i class="capability-mark custom-mark">＋</i><span><b>{ui("customGesture")}</b><small>{ui("recordPatternHint")}</small></span><em>{settings.mouseGestures.gestures.filter((item) => !item.builtin).length} / {maxCustomGestures}</em></button>
                <button class:screenshot-active={activeGesture.mode === "region-screenshot"} on:click={selectScreenshotGesture} type="button"><i class="capability-mark screenshot-mark"></i><span><b>{ui("regionScreenshot")}</b><small>{ui("regionScreenshotHint")}</small></span><em>{ui("provided")}</em></button>
              </div>

              <div class="gesture-workbench">
                <section class="gesture-catalog">
                  <div class="gesture-section-head"><div><span>{ui("gestureLibrary")}</span><strong>{settings.mouseGestures.gestures.length} / {MAX_GESTURE_TEMPLATES}</strong></div><button disabled={settings.mouseGestures.gestures.length >= MAX_GESTURE_TEMPLATES} on:click={createGesture} type="button">{ui("createNew")}</button></div>
                  <p class="gesture-section-copy">{ui("gestureLibraryHint")}</p>
                  <div class="gesture-library" aria-label={ui("allGestures")}>
                    {#each settings.mouseGestures.gestures as gesture}
                      <button class:active={gesture.id === activeGesture.id} class:screenshot={gesture.mode === "region-screenshot"} on:click={() => selectGesture(gesture.id)} type="button">
                        <svg viewBox="0 0 100 100" aria-hidden="true">{#if gesture.samples.at(-1)?.length}<path d={gesturePath(gesture.samples.at(-1) ?? [])} />{/if}</svg>
                        <span><b>{gestureName(gesture, ui)}</b><small>{gesture.mode === "region-screenshot" ? ui("regionScreenshot") : gesture.builtin ? ui("builtinGestures") : format(ui("sampleCount"), { count: gesture.samples.length })}</small></span>
                        <i class:off={!gesture.enabled} title={gesture.enabled ? ui("enabledState") : ui("disabledState")}></i>
                      </button>
                    {/each}
                  </div>
                </section>

                <section class:screenshot-editor={activeGesture.mode === "region-screenshot"} class="gesture-editor">
                  <div class="gesture-paper-head">
                    <label><span>{ui("currentGesture")}</span><input aria-label={ui("gestureNameLabel")} value={gestureName(activeGesture, ui)} on:input={renameGesture} /></label>
                    <span class:special={activeGesture.mode === "region-screenshot"} class="gesture-kind">{activeGesture.mode === "region-screenshot" ? ui("screenshotMode") : format(ui("sampleCount"), { count: activeGesture.samples.length })}</span>
                  </div>
                  {#if activeGesture.mode === "region-screenshot"}
                    <div class="screenshot-panel">
                      <div><span>{ui("screenshotOutput")}</span><strong>{ui("pinAndOcr")}</strong></div>
                      <div class="result-modes"><button class:active={settings.ocr.screenshotResult === "pin"} on:click={() => { settings.ocr.screenshotResult = "pin"; persist(); }} type="button">{ui("pin")}</button><button class:active={settings.ocr.screenshotResult === "copy-text"} on:click={() => { settings.ocr.screenshotResult = "copy-text"; persist(); }} type="button">{ui("recognizeCopy")}</button><button class:active={settings.ocr.screenshotResult === "pin-and-copy"} on:click={() => { settings.ocr.screenshotResult = "pin-and-copy"; persist(); }} type="button">{ui("pinRecognize")}</button></div>
                      <label class="ocr-language"><span>{ui("ocrLanguage")}</span><select bind:value={settings.ocr.language} on:change={() => persist()}><option value="auto" disabled={availableOcrLanguages !== null && !availableOcrLanguages.includes("auto")}>{ui("auto")}{availableOcrLanguages !== null && !availableOcrLanguages.includes("auto") ? ui("unavailable") : ""}</option><option value="zh-Hans" disabled={availableOcrLanguages !== null && !availableOcrLanguages.includes("zh-Hans")}>{ui("simplifiedChinese")}{availableOcrLanguages !== null && !availableOcrLanguages.includes("zh-Hans") ? ui("notInstalled") : ""}</option><option value="en" disabled={availableOcrLanguages !== null && !availableOcrLanguages.includes("en")}>{ui("englishLanguage")}{availableOcrLanguages !== null && !availableOcrLanguages.includes("en") ? ui("notInstalled") : ""}</option></select></label>
                      <dl><div><dt>{ui("move")}</dt><dd>{ui("dragImage")}</dd></div><div><dt>{ui("resize")}</dt><dd>{ui("dragEdges")}</dd></div><div><dt>{ui("opacity")}</dt><dd>{ui("scrollWheel")}</dd></div><div><dt>{ui("textRecognition")}</dt><dd>{ui("rightClickOcr")}</dd></div></dl>
                    </div>
                  {/if}
                  <div class="gesture-canvas-wrap"><GestureCanvas english={english} sample={activeGesture.samples.at(-1) ?? []} onRecord={recordGesture} /></div>
                  <div class="gesture-canvas-foot"><span>{ui("holdLeft")}</span><strong>{activeGesture.samples.length < 2 ? ui("recommendTwo") : activeGesture.samples.length < 3 ? ui("recordOneMore") : ui("enoughSamples")}</strong></div>
                  {#if gestureConflict}<p class="gesture-warning">{gestureConflict}</p>{/if}
                  <div class="gesture-toolbar">
                    <button class="quiet" on:click={duplicateGesture} type="button">{ui("duplicate")}</button>
                    <button class="quiet" on:click={clearGestureSamples} type="button">{ui("rerecord")}</button>
                    <button class="quiet danger" disabled={activeGesture.builtin} on:click={deleteGesture} type="button">{ui("delete")}</button>
                    <label class="gesture-enable"><span>{activeGesture.enabled ? ui("enabledState") : ui("disabledState")}</span><span class="mini-switch"><input aria-label={ui("enableCurrentGesture")} checked={activeGesture.enabled} on:change={toggleGestureEnabled} type="checkbox" /><i></i></span></label>
                  </div>
                  {#if activeGesture.mode === "action"}
                    <div class="action-editor gesture-action">
                      <div class="variant-tabs gesture-variants">
                        <button class:active={!selectedGestureModifiers.length && gestureModifierDraft === null} on:click={() => { selectedGestureModifiers = []; cancelGestureVariant(); }} type="button">{ui("direct")}</button>
                        {#each activeGesture.modifierActions ?? [] as variant}
                          <button class:active={gestureModifierDraft === null && sameModifiers(selectedGestureModifiers, variant.modifiers)} on:click={() => { selectedGestureModifiers = [...variant.modifiers]; cancelGestureVariant(); }} type="button">{modifierLabel(variant.modifiers)}</button>
                        {/each}
                        <span class="variant-spacer"></span>
                        <button aria-label={ui("addGestureVariant")} class="variant-add" disabled={(activeGesture.modifierActions ?? []).length >= MAX_MODIFIER_VARIANTS} on:click={beginGestureVariant} title={ui("newVariant")} type="button">＋</button>
                        {#if selectedGestureModifiers.length && gestureModifierDraft === null}<button aria-label={ui("deleteGestureVariant")} class="variant-delete" on:click={() => removeGestureVariant(selectedGestureModifiers)} title={ui("deleteHotzoneVariant")} type="button">×</button>{/if}
                      </div>
                      {#if gestureModifierDraft !== null}
                        <div class="modifier-draft"><ModifierRecorder english={english} label={ui("recordGestureVariant")} emptyHint={ui("gestureVariantRecording")} value={gestureModifierDraft} onChange={commitGestureVariant} /><button aria-label={ui("cancelGestureVariant")} class="variant-delete" on:click={cancelGestureVariant} title={ui("cancel")} type="button">×</button></div>
                      {/if}
                      {#if gestureModifierError}<p class="modifier-error" role="alert">{gestureModifierError}</p>{/if}
                      {#if gestureModifierDraft === null}
                        <label><span>{ui("recognizeThen")}</span><ActionPicker options={actionOptions()} value={presetIndex(gestureActionDraft)} onSelect={setGestureActionPreset} /></label>
                        {#if gestureActionDraft.kind === "open-command" || (gestureActionDraft.kind === "shortcut" && !actionPresets.some((item) => item.kind === "shortcut" && item.value !== undefined && item.value === gestureActionDraft.value))}
                          <label><span>{gestureActionDraft.kind === "shortcut" ? ui("shortcut") : ui("command")}</span>{#if gestureActionDraft.kind === "shortcut"}<ShortcutRecorder english={english} label={ui("recordShortcut")} value={gestureShortcutDraft} onChange={setGestureActionShortcut} />{#if gestureShortcutError}<p class="modifier-error" role="alert">{gestureShortcutError}</p>{/if}{:else}<input value={gestureActionDraft.value ?? ""} on:input={setGestureActionValue} placeholder={ui("enterValue")} />{/if}</label>
                        {/if}
                      {/if}
                    </div>
                  {/if}
                </section>
              </div>

              <section class="gesture-global-settings">
                <div class="gesture-section-head"><div><span>{ui("gestureGlobal")}</span><strong>{ui("allGestures")}</strong></div></div>
                <div class="gesture-controls">
                  <label><span>{ui("triggerMouseButton")}</span><select bind:value={settings.mouseGestures.triggerButton} on:change={() => persist()}><option value="right">{ui("rightButton")}</option><option value="middle">{ui("middleButton")}</option><option value="x1">{ui("sideButton1")}</option><option value="x2">{ui("sideButton2")}</option></select></label>
                  <label><span>{ui("minDistance")} <b>{settings.mouseGestures.minDistance}px</b></span><input bind:value={settings.mouseGestures.minDistance} min="12" max="240" on:input={() => persist()} type="range" /></label>
                  <label><span>{ui("sensitivity")} <b>{settings.mouseGestures.sensitivity}</b></span><input bind:value={settings.mouseGestures.sensitivity} min="35" max="95" on:input={() => persist()} type="range" /></label>
                </div>
                <div class="gesture-options"><label><input bind:checked={settings.mouseGestures.showTrail} on:change={() => persist()} type="checkbox" /><span><b>{ui("showTrail")}</b><small>{ui("showTrailDescription")}</small></span></label><label><input bind:checked={settings.mouseGestures.fullscreenPause} on:change={() => persist()} type="checkbox" /><span><b>{ui("fullscreenPause")}</b><small>{ui("fullscreenPauseDescription")}</small></span></label></div>
              </section>
              <div class="list-section gesture-paused-apps"><div class="subhead"><div><h2>{ui("pausedGestureApps")}</h2><p>{ui("foreground")}{foregroundApp || ui("noForeground")}</p></div><button class="quiet" on:click={() => addForeground("gestures")} type="button">+ {ui("addCurrentApp")}</button></div><div class="app-list">{#each settings.mouseGestures.pausedApps as app}<div><span>{app}</span><button aria-label={format(ui("removeApp"), { app })} on:click={() => removeApp("gestures", app)} type="button">×</button></div>{:else}<p class="empty">{ui("noPausedGestureApps")}</p>{/each}</div></div>
            {:else if mode === "power"}
              {#if !helperInstallState.installed}
                <section class="helper-install-card">
                  <div class="helper-install-copy">
                    <span class="eyebrow">{ui("helperInstallIncomplete")}</span>
                    <h2>{ui("helperFilesMissing")}</h2>
                    <p>{ui("helperNotRunnable")}</p>
                  </div>
                </section>
              {/if}
              <div class="power-summary"><div class="power-orb" class:on={settings.enabled && helperStatus === "connected"}><span></span></div><h2>{settings.enabled ? helperStatus === "connected" ? ui("runtimeRunning") : ui("runtimeStarting") : ui("runtimeOff")}</h2><p>{settings.enabled ? helperStatus === "connected" ? format(ui("runtimeRunningDetail"), { summary: runtimeSummary }) : ui("runtimeStartingDetail") : ui("runtimeOffDetail")}</p></div>
              <div class="power-facts desktop-power-facts">
                <div><span>{ui("masterState")}</span><strong class:on={settings.enabled}>{settings.enabled ? ui("masterOn") : ui("masterOffState")}</strong><p>{ui("masterStateDetail")}</p></div>
                <div><span>{ui("helperSection")}</span><strong class:on={helperStatus === "connected"}>{helperStatus === "connected" ? ui("connected") : helperStatus === "connecting" ? ui("connecting") : ui("disconnected")}</strong><p>{ui("helperRole")}</p>{#if helperPlatform}<small class="platform-capabilities">{helperPlatform.system} · {helperPlatform.architecture}{helperPlatform.session ? ` · ${helperPlatform.session}` : ""} · {unavailableCapabilityText(helperPlatform)}</small>{/if}</div>
              </div>
              {#if administratorModeSupported}
                <div class="permission-settings">
                  <label class="permission-toggle" title={ui("adminModeDetail")}>
                    <span>{ui("adminMode")}</span>
                    {#if switchingPrivilege}<small role="status">{ui("adminSwitching")}</small>{/if}
                    <span class="mini-switch">
                      <input type="checkbox" role="switch" checked={helperElevated} disabled={switchingPrivilege || starting || stopping || upgradingHelper || recoveringHelper || !settings.enabled || helperStatus !== "connected"} on:change={(event) => { event.currentTarget.checked = helperElevated; void switchHelperPrivilege(); }} />
                      <span aria-hidden="true"></span>
                    </span>
                  </label>
                  {#if privilegeNotice}<p role="status">{statusText(privilegeNotice)}</p>{/if}
                </div>
              {/if}
              <div class="power-actions"><button class="apply" disabled={!helperInstallState.installed || settings.enabled || starting || stopping || switchingPrivilege} on:click={() => setPowerEnabled(true)} type="button">{ui("openFeature")}</button><button class="quiet" disabled={switchingPrivilege || starting || stopping || (!settings.enabled && helperStatus === "disconnected")} on:click={() => setPowerEnabled(false)} type="button">{ui("closeFeature")}</button><button aria-live="polite" class:failed={connectionTestState === "failed"} class:success={connectionTestState === "success"} class:testing={connectionTestState === "testing"} class="quiet connection-test" disabled={helperStatus !== "connected" || connectionTestState === "testing"} on:click={runConnectionTest} type="button"><i aria-hidden="true"></i><span>{connectionTestState === "testing" ? ui("connectionTesting") : connectionTestState === "success" ? ui("connectionOk") : connectionTestState === "failed" ? ui("connectionFailed") : ui("connectionTest")}</span></button><button class="quiet" on:click={copyDiagnostics} type="button">{ui("diagnostics")}</button></div>
              <div class="helper-meta"><span>{format(ui("helperVersion"), { version: helperInstallState.version })}</span><button on:click={() => openHelperPage("repository")} type="button">{ui("publicDownload")}</button><code>{helperInstallState.installDir ?? ui("helperInstallDirUnknown")}</code></div>
              <div class:error={Boolean(helperError)} class="status-rail"><div><span>{ui("recentAction")}</span><strong>{lastAction || statusText("noAction")}</strong></div><div><span>{ui("currentState")}</span><strong aria-live="polite">{statusText(helperError, english) || renderedLastMessage}</strong></div></div>
            {:else}
              <div class="setting-title"><div><h2>{ui("moreGlobal")}</h2><p>{ui("moreDescription")}</p></div></div>
              <div class="language-setting"><div><h2>{ui("language")}</h2><p>{ui("languageDescription")}</p></div><select aria-label={ui("language")} bind:value={language} on:change={setLanguage}><option value="zh-CN">{ui("chinese")}</option><option value="en-US">{ui("english")}</option></select></div>
              <div class="timing single"><label><span>{ui("pollInterval")}</span><div><input value={settings.pollIntervalMs} min="10" max="250" on:input={(event) => { settings.pollIntervalMs = Number((event.currentTarget as HTMLInputElement).value); persist(); }} type="number" /><em>ms</em></div></label></div>
              <div class="config-section"><h2>{ui("config")}</h2><div class="config-actions"><button class="quiet" on:click={exportSettings} type="button">{ui("export")}</button><button class="quiet" on:click={importSettings} type="button">{ui("import")}</button><button class="danger" on:click={resetSettings} type="button">{ui("reset")}</button></div></div>
            {/if}
          </div>
        {/key}
      </aside>
    {/if}
  </main>
</div>
