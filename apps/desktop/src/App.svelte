<script lang="ts">
  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import { HelperClient, isSupportedHelperProtocol, SUPPORTED_HELPER_PROTOCOL } from "./helper-client";
  import { needsHelperUpgrade } from "./helper-version";
  import { HelperRecoveryGuard } from "./helper-recovery";
  import ActionPicker from "./ActionPicker.svelte";
  import MonitorStage from "./MonitorStage.svelte";
  import GestureCanvas from "./GestureCanvas.svelte";
  import ModifierRecorder from "./ModifierRecorder.svelte";
  import { gestureSimilarity, resampleGesture } from "./gesture-algorithm";
  import { migrateMonitorProfileIds } from "./monitor-profile-migration";
  import { addModifierVariant, MAX_MODIFIER_VARIANTS } from "./modifier-variants";
  import { prepareSettingsUpdate } from "./settings-sync";
  import { SettingsApplyController } from "./settings-persistence";
  import { getHostBridge } from "./host-bridge";
  import { defaultSettings, loadSettings, MAX_GESTURE_TEMPLATES, normalizeSettings, saveSettings } from "./settings-store";
  import type {
    ActionKind, AppSettings, DisplayInfo, Edge, HelperPlatformInfo, HelperStatus, HotzoneAction,
    GesturePoint, GestureTemplate, HotzoneId, HotzoneSetting, ModifierKey,
    OcrLanguage, TriggerAction, TriggerKind
  } from "./types";

  type Mode = "power" | "hotzones" | "edge-hide" | "gestures" | "more";
  type ConnectionTestState = "idle" | "testing" | "success" | "failed";
  type ActionPreset = { label: string; group: string; kind: ActionKind; value?: string };
  type FeatureTutorial = "edge-hide";
  const host = getHostBridge();
  const helper = new HelperClient("ws://127.0.0.1:56873", () => host.getHelperToken());
  const fallbackDisplay: DisplayInfo = {
    id: "display:0:0:1920:1080", primary: true,
    bounds: { left: 0, top: 0, right: 1920, bottom: 1080 },
    workArea: { left: 0, top: 0, right: 1920, bottom: 1040 }
  };
  const zoneLabels: Record<HotzoneId, string> = {
    "top-left": "左上角", top: "上边缘", "top-right": "右上角", right: "右边缘",
    "bottom-right": "右下角", bottom: "下边缘", "bottom-left": "左下角", left: "左边缘"
  };
  const triggerLabels: Record<TriggerKind, string> = {
    hover: "悬停", "left-click": "左键", "right-click": "右键", "wheel-up": "滚轮上",
    "wheel-down": "滚轮下", "slide-forward": "向下移动", "slide-backward": "向上移动"
  };
  const triggerGroups: { label: string; items: TriggerKind[] }[] = [
    { label: "悬停", items: ["hover"] }, { label: "左键", items: ["left-click"] },
    { label: "右键", items: ["right-click"] }, { label: "滚轮", items: ["wheel-up", "wheel-down"] },
    { label: "滑动", items: ["slide-forward", "slide-backward"] }
  ];
  const actionPresets: ActionPreset[] = [
    { label: "不执行", group: "基础", kind: "none" },
    { label: "显示桌面", group: "桌面与窗口", kind: "show-desktop" },
    { label: "切换窗口置顶", group: "桌面与窗口", kind: "toggle-window-topmost" },
    { label: "任务视图", group: "桌面与窗口", kind: "shortcut", value: "Win+Tab" },
    { label: "文件资源管理器", group: "桌面与窗口", kind: "shortcut", value: "Win+E" },
    { label: "Windows 搜索", group: "桌面与窗口", kind: "shortcut", value: "Win+S" },
    { label: "锁定屏幕", group: "桌面与窗口", kind: "lock-screen" },
    { label: "最小化窗口", group: "桌面与窗口", kind: "shortcut", value: "Win+Down" },
    { label: "最大化窗口", group: "桌面与窗口", kind: "shortcut", value: "Win+Up" },
    { label: "关闭窗口", group: "桌面与窗口", kind: "shortcut", value: "Alt+F4" },
    { label: "上一个窗口", group: "切换与导航", kind: "shortcut", value: "Alt+Shift+Tab" },
    { label: "下一个窗口", group: "切换与导航", kind: "shortcut", value: "Alt+Tab" },
    { label: "上一个标签页", group: "切换与导航", kind: "shortcut", value: "Ctrl+Shift+Tab" },
    { label: "下一个标签页", group: "切换与导航", kind: "shortcut", value: "Ctrl+Tab" },
    { label: "上一个虚拟桌面", group: "切换与导航", kind: "shortcut", value: "Win+Ctrl+Left" },
    { label: "下一个虚拟桌面", group: "切换与导航", kind: "shortcut", value: "Win+Ctrl+Right" },
    { label: "音量增大", group: "声音与媒体", kind: "volume-adjust", value: "0.02" },
    { label: "音量减小", group: "声音与媒体", kind: "volume-adjust", value: "-0.02" },
    { label: "静音", group: "声音与媒体", kind: "shortcut", value: "VolumeMute" },
    { label: "播放 / 暂停", group: "声音与媒体", kind: "shortcut", value: "MediaPlayPause" },
    { label: "复制", group: "编辑", kind: "shortcut", value: "Ctrl+C" },
    { label: "粘贴", group: "编辑", kind: "shortcut", value: "Ctrl+V" },
    { label: "自定义快捷键", group: "高级", kind: "shortcut" },
    { label: "运行命令", group: "高级", kind: "open-command" }
  ];
  const maxCustomGestures = MAX_GESTURE_TEMPLATES - defaultSettings.mouseGestures.gestures.length;
  const CONFIG_APPLY_DEBOUNCE_MS = 260;
  const HELPER_STABILITY_MS = 60_000;
  const capabilityLabels: Record<keyof HelperPlatformInfo["capabilities"], string> = {
    globalInput: "全局输入",
    windowControl: "窗口控制",
    windowTopmost: "窗口置顶",
    screenCapture: "屏幕截图",
    ocr: "文字识别",
    audio: "音量控制",
    systemActions: "系统动作",
    edgeHide: "贴边隐藏"
  };

  function unavailableCapabilityText(platform: HelperPlatformInfo): string {
    const unavailable = Object.entries(platform.capabilities)
      .filter(([, enabled]) => !enabled)
      .map(([name]) => capabilityLabels[name as keyof HelperPlatformInfo["capabilities"]] ?? name);
    return unavailable.length > 0
      ? `暂不支持：${unavailable.join("、")}`
      : "基础能力可用";
  }

  let settings: AppSettings = loadSettings();
  let mode: Mode | null = null;
  let helperStatus: HelperStatus = "disconnected";
  let helperPlatform: HelperPlatformInfo | null = null;
  let displays: DisplayInfo[] = [fallbackDisplay];
  let selectedDisplayId = fallbackDisplay.id;
  let selectedZone: HotzoneId = "right";
  let activeTrigger: TriggerKind = "hover";
  let selectedHotzoneModifiers: ModifierKey[] = [];
  let hotzoneModifierDraft: ModifierKey[] | null = null;
  let hotzoneModifierError = "";
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
  let availableOcrLanguages: OcrLanguage[] | null = null;
  $: activeGesture = settings.mouseGestures.gestures.find((gesture) => gesture.id === selectedGestureId)
    ?? settings.mouseGestures.gestures[0];
  let gestureConflict = "";
  let foregroundApp = "";
  let runtimeSummary = "等待 helper 上报显示器";
  let lastAction = "尚无动作";
  let lastMessage = "等待连接";
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

  onMount(() => {
    initTheme();
    const offStatus = helper.onStatus((status) => {
      helperStatus = status;
      if (status === "connected") lastMessage = "helper 已连接，正在同步配置";
      if (status === "disconnected" && connectionTestState === "testing") finishConnectionTest(false, "连接已断开");
      if (status === "disconnected") {
        helperPlatform = null;
        const wasReady = helperWasReady;
        helperWasReady = false;
        if (upgradingHelper) scheduleHelperUpgradeRestart();
        else if (stopping) { stopping = false; lastMessage = "helper 已停止"; }
        else if (!helperRecoveryFailed && settings.enabled && (wasReady || recoveringHelper)) scheduleHelperRecovery();
      }
    });
    const offMessage = helper.onMessage((message) => {
      if (message.type === "helper.ready") {
        const data = message.data as { version?: unknown; protocolVersion?: unknown; ocrLanguages?: unknown; platform?: HelperPlatformInfo } | null;
        helperPlatform = helper.platformInfo ?? (data?.platform ?? null);
        availableOcrLanguages = Array.isArray(data?.ocrLanguages)
          ? data.ocrLanguages.filter((language): language is OcrLanguage => language === "auto" || language === "zh-Hans" || language === "en")
          : null;
        if (!isSupportedHelperProtocol(data?.protocolVersion)) {
          lastMessage = `helper 协议不兼容，需要协议 v5-v${SUPPORTED_HELPER_PROTOCOL}`;
          helperRecoveryFailed = true;
          helper.stop();
        } else if (needsHelperUpgrade(expectedHelperVersion, data?.version)) {
          requestHelperUpgrade(data?.version);
        } else {
          helperUpgradeAttempts = 0;
          markHelperReady();
        }
      } else if (message.type === "config.applied") {
        const data = message.data as { revision?: unknown; adjusted?: unknown } | null;
        if (helper.isLatestConfigRevision(data?.revision)) {
          lastMessage = data?.adjusted ? "配置已应用，部分值已由 helper 调整" : "配置已应用";
        }
      } else if (message.type === "runtime.status") {
        const data = message.data as { displays?: DisplayInfo[]; foreground?: string; message?: string };
        if (Array.isArray(data.displays) && data.displays.length) {
          displays = data.displays;
          const selectedDisplay = displays.find((display) =>
            display.id === selectedDisplayId || display.legacyId === selectedDisplayId
          );
          if (selectedDisplay) selectedDisplayId = selectedDisplay.id;
          if (migrateMonitorProfileIds(settings, displays)) persist("显示器配置已迁移并应用");
          if (!displays.some((display) => display.id === selectedDisplayId)) {
            selectedDisplayId = displays.find((display) => display.primary)?.id ?? displays[0].id;
          }
          runtimeSummary = `${displays.length} 个显示器已识别`;
        }
        if (data.foreground) foregroundApp = data.foreground;
        if (data.message) lastMessage = data.message;
      } else if (message.type === "action.triggered") {
        const data = message.data as { source?: string; kind?: string };
        lastAction = [data.source, data.kind].filter(Boolean).join(" · ") || "动作已触发";
      } else if (message.type === "gesture.recognized") {
        const data = message.data as { name?: string; capturePath?: unknown };
        lastAction = typeof data.capturePath === "string" && data.capturePath.length
          ? `${data.name || "截图贴图"} · ${data.capturePath}`
          : data.name || "手势已识别";
      } else if (message.type === "ocr.completed") {
        const data = message.data as { characters?: number };
        lastAction = "OCR · 文字已复制";
        lastMessage = `已识别并复制 ${Math.max(0, Number(data.characters) || 0)} 个字符`;
      } else if (message.type === "runtime.error") {
        const data = message.data as { message?: string };
        lastMessage = data.message ?? "helper 运行错误";
      } else if (message.type === "helper.pong") {
        if (connectionTestState === "testing") {
          const elapsedMs = Math.max(1, Math.round(performance.now() - connectionTestStartedAt));
          finishConnectionTest(true, `连接正常 · ${elapsedMs} ms`);
        } else if (connectionTestState === "idle") {
          lastMessage = "helper 响应正常";
        }
      }
    });
    helper.sendConfig(settings);
    if (hasSecureBridge() && helperInstallState.installed && settings.enabled) void startHelper();
    else if (hasSecureBridge() && helperInstallState.installed) lastMessage = "功能总开关已关闭，后台助手未启动";
    else if (hasSecureBridge()) lastMessage = "安装后台助手后，热区与贴边功能才会生效";
    else lastMessage = "桌面宿主桥接未加载，请重新启动应用";
    return () => { settingsApply.dispose(); if (helperUpgradeTimer) clearTimeout(helperUpgradeTimer); clearHelperRecoveryTimers(); clearConnectionTestTimers(); offStatus(); offMessage(); helper.disconnect(); };
  });

  function runConnectionTest(): void {
    clearConnectionTestTimers();
    connectionTestState = "testing";
    connectionTestStartedAt = performance.now();
    lastMessage = "正在检查 helper 响应";
    if (!helper.ping()) {
      finishConnectionTest(false, "连接请求未发送");
      return;
    }
    connectionTestTimer = setTimeout(() => finishConnectionTest(false, "连接测试超时"), 3000);
  }

  function finishConnectionTest(success: boolean, message: string): void {
    if (connectionTestTimer) clearTimeout(connectionTestTimer);
    connectionTestTimer = null;
    connectionTestState = success ? "success" : "failed";
    lastMessage = message;
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

  function persist(message = "已自动保存，正在应用"): void {
    const prepared = prepareSettingsUpdate(settings);
    settings = prepared.editable;
    lastMessage = "正在保存设置…";
    void persistPreparedSettings(prepared.normalized, message);
  }

  async function persistPreparedSettings(value: AppSettings, message: string): Promise<void> {
    const result = await settingsApply.schedule(value, (sent) => {
      lastMessage = sent ? "配置已发送，等待确认" : "设置已保存，等待 helper 连接";
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
        ? `配置保存失败：${result.error.message}`
        : `配置保存失败：${String(result.error)}`;
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
    if (!modifiers.length) return "直接触发";
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
      hotzoneModifierError = `最多可添加 ${MAX_MODIFIER_VARIANTS} 个组合`;
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
    persist("组合已添加，请设置动作");
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
    persist("组合变体已删除");
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
    if (preset.kind === "volume-adjust") slot.cooldownMs = Math.min(slot.cooldownMs ?? settings.actionCooldownMs, 32);
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
    if (trigger === "slide-forward") return ["left", "right"].includes(selectedZone) ? "向下移动" : "向右移动";
    if (trigger === "slide-backward") return ["left", "right"].includes(selectedZone) ? "向上移动" : "向左移动";
    return triggerLabels[trigger];
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
    persist("边条状态已更新");
  }

  function addForeground(target: "hotzones" | "edge" | "gestures"): void {
    if (!foregroundApp) { lastMessage = "尚未获取到当前前台应用"; return; }
    const list = target === "hotzones" ? settings.pausedApps : target === "edge" ? settings.edgeHide.excludedApps : settings.mouseGestures.pausedApps;
    if (!list.some((item) => item.toLowerCase() === foregroundApp.toLowerCase())) list.push(foregroundApp);
    persist(`已添加 ${foregroundApp}`);
  }

  function removeApp(target: "hotzones" | "edge" | "gestures", app: string): void {
    if (target === "hotzones") settings.pausedApps = settings.pausedApps.filter((item) => item !== app);
    else if (target === "edge") settings.edgeHide.excludedApps = settings.edgeHide.excludedApps.filter((item) => item !== app);
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
      gestureModifierError = `最多可添加 ${MAX_MODIFIER_VARIANTS} 个组合`;
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
    persist("手势组合已添加，请设置动作");
  }

  function cancelGestureVariant(): void {
    gestureModifierDraft = null;
    gestureModifierError = "";
  }

  function removeGestureVariant(modifiers: ModifierKey[]): void {
    const gesture = currentGesture();
    gesture.modifierActions = (gesture.modifierActions ?? []).filter((item) => !sameModifiers(item.modifiers, modifiers));
    selectedGestureModifiers = [];
    persist("手势组合变体已删除");
  }

  function createGesture(): void {
    if (settings.mouseGestures.gestures.length >= MAX_GESTURE_TEMPLATES) {
      lastMessage = `手势数量已达到 ${MAX_GESTURE_TEMPLATES} 个上限`;
      return;
    }
    const gesture: GestureTemplate = {
      id: `gesture-${globalThis.crypto?.randomUUID?.() ?? Date.now()}`,
      name: "新手势",
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
    persist("已创建手势，请录制 2–3 次");
  }

  function selectGesture(id: string): void {
    selectedGestureId = id;
    selectedGestureModifiers = [];
    cancelGestureVariant();
    gestureConflict = findGestureConflict(currentGesture());
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
    if (!normalized.length) { gestureConflict = "轨迹过短，请重新录制"; return; }
    gesture.samples = [...gesture.samples.slice(-7), normalized];
    gestureConflict = findGestureConflict(gesture);
    persist(gesture.samples.length < 3 ? `已保存第 ${gesture.samples.length} 个样本，建议继续录制` : "手势样本已更新");
  }

  function renameGesture(event: Event): void {
    currentGesture().name = (event.currentTarget as HTMLInputElement).value.slice(0, 40);
    persist();
  }

  function duplicateGesture(): void {
    const source = currentGesture();
    const copy: GestureTemplate = {
      ...source,
      id: `gesture-${globalThis.crypto?.randomUUID?.() ?? Date.now()}`,
      name: `${source.name} 副本`.slice(0, 40),
      builtin: false,
      action: { ...source.action },
      modifierActions: (source.modifierActions ?? []).map((item) => ({ modifiers: [...item.modifiers], action: { ...item.action } })),
      samples: source.samples.map((sample) => sample.map((point) => ({ ...point })))
    };
    settings.mouseGestures.gestures = [...settings.mouseGestures.gestures, copy];
    selectGesture(copy.id);
    persist("手势副本已创建");
  }

  function deleteGesture(): void {
    const gesture = currentGesture();
    if (gesture.builtin || !window.confirm(`删除手势“${gesture.name}”？`)) return;
    settings.mouseGestures.gestures = settings.mouseGestures.gestures.filter((item) => item.id !== gesture.id);
    selectGesture(settings.mouseGestures.gestures[0]?.id ?? "");
    gestureConflict = "";
    persist("手势已删除");
  }

  function clearGestureSamples(): void {
    currentGesture().samples = [];
    gestureConflict = "";
    persist("旧样本已清除，请重新录制");
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
    settings = { ...settings };
    persist();
  }

  function setGestureActionValue(event: Event): void {
    ensureGestureActionTarget().value = (event.currentTarget as HTMLInputElement).value;
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
        if (!best || score > best.score) best = { name: gesture.name, score };
      }
    }
    return best && best.score >= 0.88 ? `与“${best.name}”较相似（${Math.round(best.score * 100)}%），建议重录` : "";
  }

  function markHelperReady(): void {
    helperWasReady = true;
    helperRecoveryFailed = false;
    if (recoveringHelper) {
      recoveringHelper = false;
      lastMessage = "helper 已自动恢复，正在同步配置";
    }
    if (helperStabilityTimer) clearTimeout(helperStabilityTimer);
    helperStabilityTimer = setTimeout(() => {
      helperStabilityTimer = null;
      if (helperWasReady && helperStatus === "connected") helperRecovery.markStable();
    }, HELPER_STABILITY_MS);
  }

  function scheduleHelperRecovery(): void {
    if (helperRecoveryTimer || helperRecoveryFailed || stopping || upgradingHelper) return;
    if (helperRecovery.requestRecovery() === "fail") {
      failHelperRecovery("后台助手连续异常退出，已停止自动恢复");
      return;
    }
    recoveringHelper = true;
    lastMessage = "helper 意外退出，正在自动恢复（1/1）";
    helperRecoveryTimer = setTimeout(async () => {
      helperRecoveryTimer = null;
      try {
        const result = await host.startHelper();
        if (!result.ok) {
          failHelperRecovery(result.error ?? "helper 自动恢复失败");
          return;
        }
        lastMessage = "helper 已重新启动，正在验证连接";
        helper.connect();
      } catch (error) {
        failHelperRecovery(`helper 自动恢复失败：${error instanceof Error ? error.message : String(error)}`);
      }
    }, 500);
  }

  function failHelperRecovery(message: string): void {
    recoveringHelper = false;
    helperRecoveryFailed = true;
    helper.disconnect();
    lastMessage = message;
  }

  function resetHelperRecovery(): void {
    clearHelperRecoveryTimers();
    helperRecovery.reset();
    helperWasReady = false;
    recoveringHelper = false;
    helperRecoveryFailed = false;
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
      ? result.alreadyRunning ? "正在连接 helper" : "helper 已启动，正在连接"
      : result.error ?? "无法启动 helper";
    if (result.ok) helper.connect();
    return result.ok;
  }
  function openHelperPage(target: "repository" | "release"): void {
    const url = helperInstallState[target];
    if (url) host.openExternal(url);
  }
  function stopHelper(): void {
    stopping = true;
    resetHelperRecovery();
    helper.stop();
    lastMessage = "正在停止 helper…";
  }
  async function setPowerEnabled(enabled: boolean): Promise<void> {
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
      lastMessage = "helper 自动升级失败，请停止后重新打开应用";
      return;
    }
    helperUpgradeAttempts += 1;
    helperRecovery.reset();
    upgradingHelper = true;
    const current = typeof actualVersion === "string" ? actualVersion : "旧版";
    lastMessage = `正在升级 helper ${current} → ${expectedHelperVersion}`;
    helper.stop();
  }
  function scheduleHelperUpgradeRestart(): void {
    if (!upgradingHelper || helperUpgradeTimer) return;
    helperUpgradeTimer = setTimeout(async () => {
      helperUpgradeTimer = null;
      const result = await host.startHelper();
      upgradingHelper = false;
      if (result.ok) {
        lastMessage = "新版 helper 已启动，正在验证版本";
        helper.connect();
      } else {
        lastMessage = result.error ?? "新版 helper 启动失败";
      }
    }, 500);
  }
  async function applyNow(): Promise<boolean> {
    const prepared = prepareSettingsUpdate(settings);
    settings = prepared.editable;
    const result = await settingsApply.applyNow(prepared.normalized);
    if (!handleSettingsCommit(result)) return false;
    lastMessage = result.sent ? "配置已发送，等待确认" : "设置已保存，等待 helper 连接";
    return true;
  }
  async function resetSettings(): Promise<void> {
    settings = normalizeSettings(defaultSettings);
    selectedHotzoneModifiers = [];
    cancelHotzoneVariant();
    selectGesture(settings.mouseGestures.gestures[0]?.id ?? "");
    const result = await settingsApply.applyNow(settings);
    if (!handleSettingsCommit(result)) return;
    lastMessage = result.sent ? "默认配置已发送" : "已恢复默认，等待 helper 连接";
  }
  async function exportSettings(): Promise<void> {
    try {
      lastMessage = await host.exportSettings(normalizeSettings(settings)) ? "配置已导出" : "已取消导出";
    } catch (error) {
      lastMessage = `导出失败：${error instanceof Error ? error.message : String(error)}`;
    }
  }
  async function importSettings(): Promise<void> {
    try {
      const imported = await host.importSettings();
      if (!imported) { lastMessage = "已取消导入"; return; }
      settings = normalizeSettings(imported);
      selectedHotzoneModifiers = [];
      cancelHotzoneVariant();
      selectGesture(settings.mouseGestures.gestures[0]?.id ?? "");
      if (!(await savePreparedSettings(settings))) return;
      helper.sendConfig(settings);
      lastMessage = "配置已导入";
    } catch (error) {
      lastMessage = `导入失败：${error instanceof Error ? error.message : "配置文件格式无效"}`;
    }
  }
  async function copyDiagnostics(): Promise<void> {
    try {
      await navigator.clipboard.writeText(JSON.stringify(await host.diagnostics(), null, 2));
      lastMessage = "诊断信息已复制";
    } catch (error) {
      lastMessage = `诊断读取失败：${error instanceof Error ? error.message : String(error)}`;
    }
  }
  function cloneHotzones(zones: HotzoneSetting[]): HotzoneSetting[] { return zones.map((zone) => ({ ...zone, actions: zone.actions.map((item) => ({ trigger: item.trigger, action: { ...item.action }, modifierActions: (item.modifierActions ?? []).map((variant) => ({ modifiers: [...variant.modifiers], action: { ...variant.action } })), cooldownMs: item.cooldownMs, hoverDelayMs: item.hoverDelayMs })) })); }
  function hasSecureBridge(): boolean { return host.kind === "desktop"; }
</script>

<div class:app-disabled={!settings.enabled} class="app-shell">
  <header class="topbar">
    <div class="brand"><img src="app-icon.png" alt="" /><strong>便捷窗口</strong></div>
    <div class:connected={helperStatus === "connected"} class="connection"><i></i>{helperStatus === "connected" ? "已连接" : helperStatus === "connecting" ? "连接中" : "未连接"}<span>{runtimeSummary}</span></div>
    <label class="master">功能总开关 <input checked={settings.enabled} disabled={starting || stopping} on:change={togglePower} type="checkbox" /><span></span></label>
    <button aria-label={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"} class="theme-toggle" on:click={toggleTheme} title={theme === "dark" ? "切换到浅色主题" : "切换到深色主题"} type="button">
      <svg class="icon-sun" fill="none" stroke="currentColor" stroke-linecap="round" stroke-width="1.8" viewBox="0 0 24 24"><circle cx="12" cy="12" r="4.2" /><path d="M12 2.5v2.4M12 19.1v2.4M2.5 12h2.4M19.1 12h2.4M5 5l1.7 1.7M17.3 17.3 19 19M19 5l-1.7 1.7M6.7 17.3 5 19" /></svg>
      <svg class="icon-moon" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" viewBox="0 0 24 24"><path d="M20.4 13.2A8.4 8.4 0 1 1 10.8 3.6a6.8 6.8 0 0 0 9.6 9.6z" /></svg>
    </button>
  </header>

  <main class:drawer-open={mode !== null} class:gesture-open={mode === "gestures"} class="workspace">
    <section class="scene" aria-label="显示器与功能预览">
      <div class="scene-tools">
        <button class="display-picker" on:click={() => helper.ping()} type="button"><i></i>{displays.length} 块屏幕 <span>重新识别</span></button>
        {#if mode === "hotzones"}<p>点击屏幕边角选择区域</p>{:else if mode === "edge-hide"}<p>点击窗口边缘启用收缩</p>{:else if mode === "gestures"}<p>按住触发键，在任意位置画出轨迹</p>{:else}<p>选择下方功能开始设置</p>{/if}
      </div>

      <MonitorStage
        {displays}
        {mode}
        {selectedDisplayId}
        {selectedZone}
        edgeHideEnabled={settings.edgeHide.enabled}
        edgeHideEdges={currentDisplayEdgeHideEdges}
        hotzonesEnabled={settings.hotzonesEnabled}
        hotzones={currentDisplayHotzones}
        onSelectDisplay={selectDisplay}
        onSelectZone={selectZone}
        onToggleEdge={toggleEdge}
      />

      <nav class="mode-nav" aria-label="功能导航">
        <button class:active={mode === "power"} on:click={() => toggleMode("power")} type="button"><span class="nav-icon power-icon"></span><b>运行中心</b></button>
        <button class:active={mode === "hotzones"} on:click={() => toggleMode("hotzones")} type="button"><span class="nav-icon corner-icon"></span><b>触发角</b></button>
        <button class:active={mode === "edge-hide"} on:click={() => toggleMode("edge-hide")} type="button"><span class="nav-icon edge-icon"></span><b>窗口增强</b></button>
        <button class:active={mode === "gestures"} on:click={() => toggleMode("gestures")} type="button"><span class="nav-icon gesture-icon"></span><b>鼠标手势</b></button>
        <button class:active={mode === "more"} on:click={() => toggleMode("more")} type="button"><span class="nav-icon more-icon"></span><b>更多</b></button>
      </nav>
    </section>

    {#if mode !== null}
      <aside class:gesture-drawer={mode === "gestures"} class="drawer" in:fly={{ x: 28, duration: 220 }}>
        <header class="drawer-head">
          <div><span>{mode === "power" ? "运行中心" : mode === "hotzones" ? "触发角" : mode === "edge-hide" ? "窗口增强" : mode === "gestures" ? "鼠标手势" : "更多设置"}</span><p>{mode === "hotzones" ? `${zoneLabels[selectedZone]} · S${Math.max(0, displays.findIndex((item) => item.id === selectedDisplayId)) + 1}` : mode === "edge-hide" ? windowEnhancementTab === "edge" ? "贴边隐藏与恢复" : "任意位置移动与缩放" : mode === "gestures" ? "单笔轨迹 · 全局生效" : mode === "more" ? "全局与配置管理" : "功能规则与后台助手"}</p></div>
          <button aria-label="收起设置" class="close-drawer" on:click={closeMode} type="button">×</button>
        </header>

        {#key mode}
          <div class="drawer-body" in:fly={{ y: 10, duration: 170 }}>
            {#if mode === "hotzones"}
              <div class="feature-intro hotzone-master-intro">
                <div class="feature-copy-stack">
                  <span>触发角总开关</span>
                  <h2>把屏幕边角变成快捷操作入口</h2>
                  <p>开启后，当前显示器的区域与触发方式统一生效；关闭只暂停动作，不删除已有配置。</p>
                </div>
                <div class="feature-master-toggle"><b class:on={settings.hotzonesEnabled}>{settings.hotzonesEnabled ? "总开关已开启" : "总开关已关闭"}</b><label class="mini-switch"><input aria-label="启用触发角功能" bind:checked={settings.hotzonesEnabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
              </div>
              <div class="feature-settings-head"><span>触发角设置</span><strong>{settings.hotzonesEnabled ? "由总开关统一启用" : "关闭后保留配置与预览"}</strong></div>
              <div class="feature-settings-body" class:off={!settings.hotzonesEnabled} inert={!settings.hotzonesEnabled}>
                <div class="trigger-tabs">
                  {#each triggerGroups as group}
                    <button class:active={group.items.includes(activeTrigger)} disabled={group.label === "滑动" && !["top", "right", "bottom", "left"].includes(selectedZone)} on:click={() => { activeTrigger = group.items[0]; selectedHotzoneModifiers = []; cancelHotzoneVariant(); }} type="button">{group.label}</button>
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
                    <button class:active={!selectedHotzoneModifiers.length && hotzoneModifierDraft === null} on:click={() => selectHotzoneVariant([])} type="button">直接触发</button>
                    {#each currentHotzoneModifierActions as variant}
                      <button class:active={hotzoneModifierDraft === null && sameModifiers(selectedHotzoneModifiers, variant.modifiers)} on:click={() => selectHotzoneVariant(variant.modifiers)} type="button">{modifierLabel(variant.modifiers)}</button>
                    {/each}
                    <span class="variant-spacer"></span>
                    <button aria-label="新增热区组合" class="variant-add" disabled={currentHotzoneModifierActions.length >= MAX_MODIFIER_VARIANTS} on:click={beginHotzoneVariant} title="新增组合" type="button">＋</button>
                    {#if selectedHotzoneModifiers.length && hotzoneModifierDraft === null}<button aria-label="删除当前组合变体" class="variant-delete" on:click={() => removeHotzoneVariant(selectedHotzoneModifiers)} title="删除当前组合" type="button">×</button>{/if}
                  </div>
                  {#if hotzoneModifierDraft !== null}
                    <div class="modifier-draft"><ModifierRecorder label="录制新热区组合" value={hotzoneModifierDraft} onChange={commitHotzoneVariant} /><button aria-label="取消新增组合" class="variant-delete" on:click={cancelHotzoneVariant} title="取消" type="button">×</button></div>
                  {/if}
                  {#if hotzoneModifierError}<p class="modifier-error" role="alert">{hotzoneModifierError}</p>{/if}
                </div>
                {#if hotzoneModifierDraft === null}
                {#key `${selectedZone}:${activeTrigger}:${modifierId(selectedHotzoneModifiers)}:${actionEditorRevision}`}
                <div class="action-editor">
                  <label><span>执行动作</span><ActionPicker options={actionPresets} value={presetIndex(currentAction())} onSelect={setActionPreset} /></label>
                  {#if currentAction().kind === "open-command" || (currentAction().kind === "shortcut" && !actionPresets.some((item) => item.kind === "shortcut" && item.value !== undefined && item.value === currentAction().value))}
                    <label><span>{currentAction().kind === "shortcut" ? "快捷键" : "命令"}</span><input value={currentAction().value ?? ""} on:input={setActionValue} placeholder={currentAction().kind === "shortcut" ? "例如 Ctrl+Alt+T" : "请输入参数"} /></label>
                  {/if}
                  <div class="action-state"><i class:enabled={currentAction().kind !== "none"}></i><div><strong>{triggerLabel(activeTrigger)}</strong><p>{currentAction().kind === "none" ? "尚未设置动作" : actionPresets[presetIndex(currentAction())]?.label ?? "自定义动作"}</p></div></div>
                </div>
                {/key}
                {/if}
                <div class:single={activeTrigger !== "hover"} class="timing">{#if activeTrigger === "hover"}<label><span>当前悬停延迟</span><div><input value={currentTriggerSlot().hoverDelayMs ?? settings.hoverDelayMs} min="0" max="3000" on:input={(event) => setTriggerTiming("hoverDelayMs", event)} type="number" /><em>ms</em></div></label>{/if}<label><span>当前触发冷却</span><div><input value={currentTriggerSlot().cooldownMs ?? settings.actionCooldownMs} min="10" max="5000" on:input={(event) => setTriggerTiming("cooldownMs", event)} type="number" /><em>ms</em></div></label></div>
              </div>
            {:else if mode === "edge-hide"}
              <div class="window-tabs"><button class:active={windowEnhancementTab === "edge"} on:click={() => { windowEnhancementTab = "edge"; }} type="button">贴边隐藏</button><button class:active={windowEnhancementTab === "drag"} on:click={() => { windowEnhancementTab = "drag"; }} type="button">拖拽与缩放</button></div>
              {#if windowEnhancementTab === "edge"}
                <div class="feature-intro edge-master-intro" on:mouseleave={(event) => handleFeatureTutorialMouseLeave("edge-hide", event)} on:focusout={(event) => handleFeatureTutorialFocusOut("edge-hide", event)} role="group">
                  <div class="feature-copy-stack"><span>贴边隐藏总开关</span><div class="feature-title-row"><h2>让窗口在屏幕边缘自动收起</h2><div class="feature-help" on:mouseenter={() => showFeatureTutorial("edge-hide")} on:focusin={() => showFeatureTutorial("edge-hide")} role="group"><button aria-controls="edge-hide-tutorial" aria-describedby={activeFeatureTutorial === "edge-hide" ? "edge-hide-tutorial" : undefined} aria-expanded={activeFeatureTutorial === "edge-hide"} aria-label="查看贴边隐藏教程" class="feature-help-button" on:click={() => showFeatureTutorial("edge-hide")} type="button">?</button>{#if activeFeatureTutorial === "edge-hide"}<section class="feature-help-card" id="edge-hide-tutorial" role="tooltip"><strong>贴边隐藏教程</strong><div aria-hidden="true" class="tutorial-demo"><div class="tutorial-screen"><div class="tutorial-edge-window"><i></i><span></span></div><div class="tutorial-cursor"><i></i></div></div></div><p>拖到屏幕外边缘并松开，窗口自动收起；移到露出区域即可恢复。</p></section>{/if}</div></div><p>开启后，下方的吸附提示、触发方式、露出宽度、延迟和排除规则统一生效；每台显示器独立设置，拼接缝处自动禁用。</p></div>
                  <div class="feature-master-toggle"><b class:on={settings.edgeHide.enabled}>{settings.edgeHide.enabled ? "总开关已开启" : "总开关已关闭"}</b><label class="mini-switch"><input aria-label="启用贴边隐藏" bind:checked={settings.edgeHide.enabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                </div>
                <div class="feature-settings-head"><span>贴边隐藏选项</span><strong>{settings.edgeHide.enabled ? "由总开关统一启用" : "开启总开关后可调整"}</strong></div>
                <div class="feature-settings-body" class:off={!settings.edgeHide.enabled} inert={!settings.edgeHide.enabled}>
                <div class="setting-title edge-preview-setting"><div><h2>显示吸附提示</h2><p>松开后会收纳时，在目标屏幕边缘显示蓝色强调线</p></div><label class="mini-switch"><input aria-label="显示贴边吸附提示" bind:checked={settings.edgeHide.showPreview} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                <div class="setting-title edge-restore-hint-setting"><div><h2>显示展开轮廓</h2><p>关闭只隐藏窗口收纳后的淡白轮廓，边缘恢复仍可触发</p></div><label class="mini-switch"><input aria-label="显示窗口展开轮廓" bind:checked={settings.edgeHide.showRestoreHint} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                <div class="setting-title edge-foreground-setting"><div><h2>前台窗口保持展开</h2><p>窗口展开后仍在使用时，不会因为鼠标离开而自动收回</p></div><label class="mini-switch"><input aria-label="前台窗口保持展开" bind:checked={settings.edgeHide.keepExpandedWhenForeground} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                <div class="edge-trigger-heading"><h2>收纳触发方式</h2><p>两种方式独立生效，满足任意一种即可触发</p></div>
                <div class="edge-trigger-methods">
                  <section class:off={!settings.edgeHide.distanceTriggerEnabled} class="edge-trigger-method">
                    <div class="setting-title"><div><h3>靠近边缘</h3><p>窗口边缘进入指定像素范围</p></div><label class="mini-switch"><input aria-label="启用靠近边缘触发" bind:checked={settings.edgeHide.distanceTriggerEnabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                    <label class="trigger-value"><span>边缘距离</span><div><input bind:value={settings.edgeHide.triggerDistance} disabled={!settings.edgeHide.distanceTriggerEnabled} min="4" max="96" on:input={() => persist()} type="number" /><em>px</em></div></label>
                  </section>
                  <section class:off={!settings.edgeHide.ratioTriggerEnabled} class="edge-trigger-method">
                    <div class="setting-title"><div><h3>移出比例</h3><p>窗口移出屏幕达到自身比例</p></div><label class="mini-switch"><input aria-label="启用移出比例触发" bind:checked={settings.edgeHide.ratioTriggerEnabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                    <label class="trigger-value"><span>窗口比例</span><div><input bind:value={settings.edgeHide.triggerRatio} disabled={!settings.edgeHide.ratioTriggerEnabled} min="1" max="100" on:input={() => persist()} type="number" /><em>%</em></div></label>
                  </section>
                </div>
                <div class="form-grid"><label><span>露出宽度</span><div><input bind:value={settings.edgeHide.stripSize} min="4" max="64" on:input={() => persist()} type="number" /><em>px</em></div></label><label><span>收缩延迟</span><div><input bind:value={settings.edgeHide.collapseDelayMs} min="0" max="5000" on:input={() => persist()} type="number" /><em>ms</em></div></label><label><span>恢复延迟</span><div><input bind:value={settings.edgeHide.restoreDelayMs} min="0" max="5000" on:input={() => persist()} type="number" /><em>ms</em></div></label></div>
                <div class="list-section"><div class="subhead"><div><h2>不收缩的应用</h2><p>当前前台：{foregroundApp || "尚未获取"}</p></div><button class="quiet" on:click={() => addForeground("edge")} type="button">+ 添加</button></div><div class="app-list">{#each settings.edgeHide.excludedApps as app}<div><span>{app}</span><button aria-label={`移除 ${app}`} on:click={() => removeApp("edge", app)} type="button">×</button></div>{:else}<p class="empty">还没有排除应用</p>{/each}</div></div>
                </div>
              {:else}
                <div class="feature-intro drag-master-intro">
                  <div class="feature-copy-stack"><span>拖拽与缩放总开关</span><div class="feature-title-row"><h2>从窗口任意位置移动或缩放</h2></div><p>开启后，下方两组鼠标组合才会接管目标窗口；窗口布局仍由 Windows 原生 Snap 负责。</p></div>
                  <div class="feature-master-toggle"><b class:on={settings.windowDrag.enabled}>{settings.windowDrag.enabled ? "总开关已开启" : "总开关已关闭"}</b><label class="mini-switch"><input aria-label="启用任意位置拖拽" bind:checked={settings.windowDrag.enabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
                </div>
                <div class="feature-settings-head"><span>拖拽组合设置</span><strong>{settings.windowDrag.enabled ? "由总开关统一启用" : "开启总开关后可调整"}</strong></div>
                <div class="feature-settings-body" class:off={!settings.windowDrag.enabled} inert={!settings.windowDrag.enabled}>
                <div class="drag-bindings">
                  <div><span><b>移动窗口</b><small>默认 Alt + 左键</small></span><ModifierRecorder label="录制移动窗口修饰键" value={settings.windowDrag.moveModifiers} onChange={(value) => { settings.windowDrag.moveModifiers = value; persist(); }} /><select aria-label="移动窗口鼠标键" bind:value={settings.windowDrag.moveButton} on:change={() => persist()}><option value="left">左键</option><option value="right">右键</option><option value="middle">中键</option><option value="x1">侧键 1</option><option value="x2">侧键 2</option></select></div>
                  <div><span><b>缩放窗口</b><small>默认 Alt + 右键</small></span><ModifierRecorder label="录制缩放窗口修饰键" value={settings.windowDrag.resizeModifiers} onChange={(value) => { settings.windowDrag.resizeModifiers = value; persist(); }} /><select aria-label="缩放窗口鼠标键" bind:value={settings.windowDrag.resizeButton} on:change={() => persist()}><option value="left">左键</option><option value="right">右键</option><option value="middle">中键</option><option value="x1">侧键 1</option><option value="x2">侧键 2</option></select></div>
                </div>
                {#if windowDragBindingConflict}<p class="gesture-warning">移动与缩放组合重复，请修改其中一项</p>{/if}
                </div>
                <div class="setting-title pin-setting"><div><h2>置顶小图钉</h2><p>跟随置顶窗口，点击即可取消置顶</p></div><label class="mini-switch"><input aria-label="启用置顶小图钉" bind:checked={settings.topmostPin.enabled} on:change={() => persist()} type="checkbox" /><span></span></label></div>
              {/if}
            {:else if mode === "gestures"}
              <div class="gesture-intro">
                <div><span>鼠标增强工作台</span><h2>画出轨迹，让鼠标多做一步</h2><p>内置 5 种起步方式，也可以继续录制自己的单笔图案；短按仍保持原鼠标功能。</p></div>
                <label class="mini-switch"><input aria-label="启用鼠标增强" bind:checked={settings.mouseGestures.enabled} on:change={() => persist()} type="checkbox" /><span></span></label>
              </div>

              <div class="gesture-capabilities" aria-label="鼠标增强功能总览">
                <button class:active={activeGesture.mode === "action" && activeGesture.builtin} on:click={selectFirstActionGesture} type="button"><i class="capability-mark action-mark">↗</i><span><b>动作手势</b><small>复制、粘贴、窗口与系统动作</small></span><em>{settings.mouseGestures.gestures.filter((item) => item.mode === "action" && item.builtin).length} 个内置</em></button>
                <button class:active={activeGesture.mode === "action" && !activeGesture.builtin} on:click={createGesture} type="button"><i class="capability-mark custom-mark">＋</i><span><b>自定义手势</b><small>录制图案并绑定任意可用动作</small></span><em>{settings.mouseGestures.gestures.filter((item) => !item.builtin).length} / {maxCustomGestures}</em></button>
                <button class:screenshot-active={activeGesture.mode === "region-screenshot"} on:click={selectScreenshotGesture} type="button"><i class="capability-mark screenshot-mark"></i><span><b>截图贴图</b><small>画矩形截图，并悬浮在桌面</small></span><em>已提供</em></button>
              </div>

              <div class="gesture-workbench">
                <section class="gesture-catalog">
                  <div class="gesture-section-head"><div><span>手势库</span><strong>{settings.mouseGestures.gestures.length} / {MAX_GESTURE_TEMPLATES}</strong></div><button disabled={settings.mouseGestures.gestures.length >= MAX_GESTURE_TEMPLATES} on:click={createGesture} type="button">＋ 新建</button></div>
                  <p class="gesture-section-copy">5 个内置模板始终保留；新手势会出现在同一列表中。</p>
                  <div class="gesture-library" aria-label="全部手势">
                    {#each settings.mouseGestures.gestures as gesture}
                      <button class:active={gesture.id === activeGesture.id} class:screenshot={gesture.mode === "region-screenshot"} on:click={() => selectGesture(gesture.id)} type="button">
                        <svg viewBox="0 0 100 100" aria-hidden="true">{#if gesture.samples.at(-1)?.length}<path d={gesturePath(gesture.samples.at(-1) ?? [])} />{/if}</svg>
                        <span><b>{gesture.name}</b><small>{gesture.mode === "region-screenshot" ? "截图贴图" : gesture.builtin ? "内置手势" : `${gesture.samples.length} 个样本`}</small></span>
                        <i class:off={!gesture.enabled} title={gesture.enabled ? "已启用" : "已停用"}></i>
                      </button>
                    {/each}
                  </div>
                </section>

                <section class:screenshot-editor={activeGesture.mode === "region-screenshot"} class="gesture-editor">
                  <div class="gesture-paper-head">
                    <label><span>当前手势</span><input aria-label="手势名称" value={activeGesture.name} on:input={renameGesture} /></label>
                    <span class:special={activeGesture.mode === "region-screenshot"} class="gesture-kind">{activeGesture.mode === "region-screenshot" ? "截图模式" : `${activeGesture.samples.length} 个样本`}</span>
                  </div>
                  {#if activeGesture.mode === "region-screenshot"}
                    <div class="screenshot-panel">
                      <div><span>矩形截图输出</span><strong>贴图与本地文字识别</strong></div>
                      <div class="result-modes"><button class:active={settings.ocr.screenshotResult === "pin"} on:click={() => { settings.ocr.screenshotResult = "pin"; persist(); }} type="button">生成贴图</button><button class:active={settings.ocr.screenshotResult === "copy-text"} on:click={() => { settings.ocr.screenshotResult = "copy-text"; persist(); }} type="button">识别并复制</button><button class:active={settings.ocr.screenshotResult === "pin-and-copy"} on:click={() => { settings.ocr.screenshotResult = "pin-and-copy"; persist(); }} type="button">贴图并识别</button></div>
                      <label class="ocr-language"><span>识别语言</span><select bind:value={settings.ocr.language} on:change={() => persist()}><option value="auto" disabled={availableOcrLanguages !== null && !availableOcrLanguages.includes("auto")}>自动{availableOcrLanguages !== null && !availableOcrLanguages.includes("auto") ? "（不可用）" : ""}</option><option value="zh-Hans" disabled={availableOcrLanguages !== null && !availableOcrLanguages.includes("zh-Hans")}>简体中文{availableOcrLanguages !== null && !availableOcrLanguages.includes("zh-Hans") ? "（未安装）" : ""}</option><option value="en" disabled={availableOcrLanguages !== null && !availableOcrLanguages.includes("en")}>英文{availableOcrLanguages !== null && !availableOcrLanguages.includes("en") ? "（未安装）" : ""}</option></select></label>
                      <dl><div><dt>移动</dt><dd>左键拖动图片</dd></div><div><dt>缩放</dt><dd>拖动四边或四角</dd></div><div><dt>透明度</dt><dd>滚动鼠标滚轮</dd></div><div><dt>文字识别</dt><dd>贴图右键执行 OCR</dd></div></dl>
                    </div>
                  {/if}
                  <div class="gesture-canvas-wrap"><GestureCanvas sample={activeGesture.samples.at(-1) ?? []} onRecord={recordGesture} /></div>
                  <div class="gesture-canvas-foot"><span>按住左键，一笔画完</span><strong>{activeGesture.samples.length < 2 ? "建议至少录制 2 次" : activeGesture.samples.length < 3 ? "再录 1 次更稳定" : "样本充足"}</strong></div>
                  {#if gestureConflict}<p class="gesture-warning">{gestureConflict}</p>{/if}
                  <div class="gesture-toolbar">
                    <button class="quiet" on:click={duplicateGesture} type="button">制作副本</button>
                    <button class="quiet" on:click={clearGestureSamples} type="button">重新录制</button>
                    <button class="quiet danger" disabled={activeGesture.builtin} on:click={deleteGesture} type="button">删除</button>
                    <label class="gesture-enable"><span>{activeGesture.enabled ? "已启用" : "已停用"}</span><span class="mini-switch"><input aria-label="启用当前手势" checked={activeGesture.enabled} on:change={toggleGestureEnabled} type="checkbox" /><i></i></span></label>
                  </div>
                  {#if activeGesture.mode === "action"}
                    <div class="action-editor gesture-action">
                      <div class="variant-tabs gesture-variants">
                        <button class:active={!selectedGestureModifiers.length && gestureModifierDraft === null} on:click={() => { selectedGestureModifiers = []; cancelGestureVariant(); }} type="button">直接触发</button>
                        {#each activeGesture.modifierActions ?? [] as variant}
                          <button class:active={gestureModifierDraft === null && sameModifiers(selectedGestureModifiers, variant.modifiers)} on:click={() => { selectedGestureModifiers = [...variant.modifiers]; cancelGestureVariant(); }} type="button">{modifierLabel(variant.modifiers)}</button>
                        {/each}
                        <span class="variant-spacer"></span>
                        <button aria-label="新增手势组合" class="variant-add" disabled={(activeGesture.modifierActions ?? []).length >= MAX_MODIFIER_VARIANTS} on:click={beginGestureVariant} title="新增组合" type="button">＋</button>
                        {#if selectedGestureModifiers.length && gestureModifierDraft === null}<button aria-label="删除当前手势组合" class="variant-delete" on:click={() => removeGestureVariant(selectedGestureModifiers)} title="删除当前组合" type="button">×</button>{/if}
                      </div>
                      {#if gestureModifierDraft !== null}
                        <div class="modifier-draft"><ModifierRecorder label="录制新手势组合" value={gestureModifierDraft} onChange={commitGestureVariant} /><button aria-label="取消新增手势组合" class="variant-delete" on:click={cancelGestureVariant} title="取消" type="button">×</button></div>
                      {/if}
                      {#if gestureModifierError}<p class="modifier-error" role="alert">{gestureModifierError}</p>{/if}
                      {#if gestureModifierDraft === null}
                        <label><span>识别后执行</span><ActionPicker options={actionPresets} value={presetIndex(currentGestureAction())} onSelect={setGestureActionPreset} /></label>
                        {#if currentGestureAction().kind === "open-command" || (currentGestureAction().kind === "shortcut" && !actionPresets.some((item) => item.kind === "shortcut" && item.value !== undefined && item.value === currentGestureAction().value))}
                          <label><span>{currentGestureAction().kind === "shortcut" ? "快捷键" : "命令"}</span><input value={currentGestureAction().value ?? ""} on:input={setGestureActionValue} placeholder="请输入参数" /></label>
                        {/if}
                      {/if}
                    </div>
                  {/if}
                </section>
              </div>

              <section class="gesture-global-settings">
                <div class="gesture-section-head"><div><span>全局识别设置</span><strong>对全部手势生效</strong></div></div>
                <div class="gesture-controls">
                  <label><span>触发鼠标键</span><select bind:value={settings.mouseGestures.triggerButton} on:change={() => persist()}><option value="right">右键</option><option value="middle">中键</option><option value="x1">侧键 1</option><option value="x2">侧键 2</option></select></label>
                  <label><span>最小移动距离 <b>{settings.mouseGestures.minDistance}px</b></span><input bind:value={settings.mouseGestures.minDistance} min="12" max="240" on:input={() => persist()} type="range" /></label>
                  <label><span>识别灵敏度 <b>{settings.mouseGestures.sensitivity}</b></span><input bind:value={settings.mouseGestures.sensitivity} min="35" max="95" on:input={() => persist()} type="range" /></label>
                </div>
                <div class="gesture-options"><label><input bind:checked={settings.mouseGestures.showTrail} on:change={() => persist()} type="checkbox" /><span><b>显示淡蓝轨迹与名称</b><small>按住触发键绘制时提供视觉反馈</small></span></label><label><input bind:checked={settings.mouseGestures.fullscreenPause} on:change={() => persist()} type="checkbox" /><span><b>全屏应用自动暂停</b><small>游戏和全屏播放时避免误触</small></span></label></div>
              </section>
              <div class="list-section gesture-paused-apps"><div class="subhead"><div><h2>暂停手势的应用</h2><p>当前前台：{foregroundApp || "尚未获取"}</p></div><button class="quiet" on:click={() => addForeground("gestures")} type="button">+ 添加当前应用</button></div><div class="app-list">{#each settings.mouseGestures.pausedApps as app}<div><span>{app}</span><button aria-label={`移除 ${app}`} on:click={() => removeApp("gestures", app)} type="button">×</button></div>{:else}<p class="empty">所有应用都会响应手势；需要排除时可添加当前前台应用。</p>{/each}</div></div>
            {:else if mode === "power"}
              {#if !helperInstallState.installed}
                <section class="helper-install-card">
                  <div class="helper-install-copy">
                    <span class="eyebrow">安装不完整</span>
                    <h2>后台助手文件缺失</h2>
                    <p>当前安装无法启动系统功能，请重新安装便捷窗口。</p>
                  </div>
                </section>
              {/if}
              <div class="power-summary"><div class="power-orb" class:on={settings.enabled && helperStatus === "connected"}><span></span></div><h2>{settings.enabled ? helperStatus === "connected" ? "功能正在运行" : "正在启动后台助手" : "功能已关闭"}</h2><p>{settings.enabled ? helperStatus === "connected" ? `${runtimeSummary}，已启用规则正在生效` : "总开关打开后会自动启动并连接后台助手" : "总开关关闭时后台助手同步停止，不占用后台资源"}</p></div>
              <div class="power-facts desktop-power-facts">
                <div><span>功能总开关</span><strong class:on={settings.enabled}>{settings.enabled ? "已打开" : "已关闭"}</strong><p>同时控制规则运行与助手生命周期</p></div>
                <div><span>后台助手</span><strong class:on={helperStatus === "connected"}>{helperStatus === "connected" ? "已连接" : helperStatus === "connecting" ? "连接中" : "未连接"}</strong><p>负责系统监听与窗口操作</p>{#if helperPlatform}<small class="platform-capabilities">{helperPlatform.system} · {helperPlatform.architecture}{helperPlatform.session ? ` · ${helperPlatform.session}` : ""} · {unavailableCapabilityText(helperPlatform)}</small>{/if}</div>
              </div>
              <div class="power-actions"><button class="apply" disabled={!helperInstallState.installed || settings.enabled || starting || stopping} on:click={() => setPowerEnabled(true)} type="button">打开功能</button><button class="quiet" disabled={starting || stopping || (!settings.enabled && helperStatus === "disconnected")} on:click={() => setPowerEnabled(false)} type="button">关闭功能</button><button aria-live="polite" class:failed={connectionTestState === "failed"} class:success={connectionTestState === "success"} class:testing={connectionTestState === "testing"} class="quiet connection-test" disabled={helperStatus !== "connected" || connectionTestState === "testing"} on:click={runConnectionTest} type="button"><i aria-hidden="true"></i><span>{connectionTestState === "testing" ? "测试中" : connectionTestState === "success" ? "连接正常" : connectionTestState === "failed" ? "测试失败" : "连接测试"}</span></button><button class="quiet" on:click={copyDiagnostics} type="button">复制诊断</button></div>
              <div class="helper-meta"><span>助手 {helperInstallState.version}</span><button on:click={() => openHelperPage("repository")} type="button">公开下载仓库</button><code>{helperInstallState.installDir ?? "尚未确定安装目录"}</code></div>
              <div class="status-rail"><div><span>最近动作</span><strong>{lastAction}</strong></div><div><span>当前状态</span><strong>{lastMessage}</strong></div></div>
            {:else}
              <div class="setting-title"><div><h2>全局参数</h2><p>调整所有显示器共用的基础参数</p></div></div>
              <div class="form-grid"><label><span>热区宽度</span><div><input bind:value={settings.edgeSize} min="2" max="48" on:input={() => persist()} type="number" /><em>px</em></div></label><label><span>轮询间隔</span><div><input bind:value={settings.pollIntervalMs} min="10" max="250" on:input={() => persist()} type="number" /><em>ms</em></div></label></div>
              <div class="list-section"><div class="subhead"><div><h2>暂停应用</h2><p>这些应用位于前台时不触发热区</p></div><button class="quiet" on:click={() => addForeground("hotzones")} type="button">+ 添加</button></div><div class="app-list">{#each settings.pausedApps as app}<div><span>{app}</span><button aria-label={`移除 ${app}`} on:click={() => removeApp("hotzones", app)} type="button">×</button></div>{:else}<p class="empty">还没有暂停应用</p>{/each}</div></div>
              <div class="config-section"><h2>配置管理</h2><div class="config-actions"><button class="quiet" on:click={exportSettings} type="button">导出配置</button><button class="quiet" on:click={importSettings} type="button">导入配置</button><button class="danger" on:click={resetSettings} type="button">恢复默认</button></div></div>
            {/if}
          </div>
        {/key}
      </aside>
    {/if}
  </main>
</div>
