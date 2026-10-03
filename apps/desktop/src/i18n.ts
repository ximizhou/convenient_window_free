// 便捷窗口界面文案字典（简中 / 英文）。
//
// 本文件在两套前端中必须逐字节一致：
//   utools-plugin/src/i18n.ts            （私有 uTools 插件）
//   open-source/apps/desktop/src/i18n.ts （公开独立桌面）
// 私有仓库 scripts/check-i18n-parity.mjs 会校验两侧哈希，任一侧单独修改都会失败。
// 因此这里是两套界面文案的并集：某一宿主用不到的键不会渲染，但两边都要有译文。
//
// 约定：
// - 键名描述语义，不描述位置；同名概念在两套前端复用同一个键。
// - 只放界面文案。宿主专有能力名（如 uTools 指令）也在这里，避免两侧各写一份。
// - 需要插值或复数的文案由调用方拼接（例：`${count}${ui("displays")}`）。

export type Language = "zh-CN" | "en-US";

/** 两套前端共用同一个持久化键，便于用户预期一致，也便于问题定位。 */
export const LANGUAGE_KEY = "convenient-window-language";

/** 用户没有选择过时，跟随系统语言：zh* 用简中，其余一律英文。 */
export function systemLanguage(locale: string | null | undefined): Language {
  return typeof locale === "string" && /^zh/i.test(locale) ? "zh-CN" : "en-US";
}

export function normalizeLanguage(value: unknown): Language {
  return value === "en-US" || value === "zh-CN" ? value : "zh-CN";
}

/** 已保存的选择优先；没有保存过时（含隐私模式读取失败）跟随系统语言。 */
export function resolveInitialLanguage(stored: unknown, locale: string | null | undefined): Language {
  return stored === "en-US" || stored === "zh-CN" ? stored : systemLanguage(locale);
}

/** 用 `{name}` 占位符填值；缺值时保留占位符，便于在界面上看出漏配。 */
export function format(template: string, values: Record<string, string | number>): string {
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    Object.prototype.hasOwnProperty.call(values, name) ? String(values[name]) : match
  );
}

export const zh = {
  gestureNotRecognized: "手势未识别，已取消",
  gestureUp: "向上 · 复制",
  gestureDown: "向下 · 粘贴",
  gestureL: "L 型 · 关闭窗口",
  gestureCircle: "圆圈 · 切换窗口置顶",
  gestureRectangle: "矩形截图",
  errorInvalidConfig: "配置格式无效，请重新导入或恢复默认设置。",
  errorUnsupportedSchema: "配置版本不兼容，请同步升级应用和后台助手。",
  errorConfigSave: "配置保存失败，请检查磁盘空间和目录权限。",
  errorUnknownMessage: "后台助手不支持此请求，请同步升级应用。",
  errorRuntime: "系统操作失败，详情见诊断日志。",
  errorInputMonitor: "无法读取输入或窗口状态，请检查系统权限。",
  errorWindow: "窗口操作失败，请检查目标窗口及权限。",
  errorAction: "动作执行失败，请检查动作参数和系统权限。",
  errorScreenCapture: "截图失败，请检查屏幕录制权限。",
  errorOcr: "文字识别失败，请检查识别语言包。",
  errorVolume: "音量调节失败，请检查音频设备和权限。",
  errorBrightness: "亮度调节失败，请检查屏幕的亮度控制支持。",
  windowPinned: "已置顶：{title}",
  windowUnpinned: "已取消置顶：{title}",
  unnamedWindow: "窗口",
  trayShow: "打开设置",
  trayAutostart: "开机自动启动",
  trayQuit: "退出便捷窗口",
  configImportTitle: "导入便捷窗口配置",
  configExportTitle: "导出便捷窗口配置",
  configFileType: "JSON 配置",

  connected: "已连接", connecting: "连接中", disconnected: "未连接", displays: "块屏幕", displayPreview: "等待助手", checkConnection: "检查连接",
  chooseArea: "点击屏幕边角选择区域", chooseEdge: "点击窗口边缘启用收缩", drawGesture: "按住触发键，在任意位置画出轨迹", chooseFeature: "选择下方功能开始设置",
  power: "运行中心", hotzones: "触发角", edgeHide: "窗口增强", gestures: "鼠标手势", more: "更多", close: "收起设置",
  hotzoneSubtitle: "触发角", edgeSubtitle: "贴边隐藏与恢复", dragSubtitle: "任意位置移动与缩放", gestureSubtitle: "单笔轨迹 · 全局生效", moreSubtitle: "全局与配置管理", powerSubtitle: "功能规则与后台助手",
  hotzoneMaster: "触发角总开关", hotzoneTitle: "把屏幕边角变成快捷操作入口", hotzoneDescription: "开启后，当前显示器的区域与触发方式统一生效；关闭只暂停动作，不删除已有配置。",
  hotzoneSettings: "触发角设置", unifiedOn: "由总开关统一启用", keepConfig: "关闭后保留配置与预览", enableHotzones: "启用触发角功能",
  edgeMaster: "贴边隐藏总开关", edgeTitle: "让窗口在屏幕边缘自动收起", edgeDescription: "开启后，下方的吸附提示、触发方式、露出宽度、延迟和排除规则统一生效；每台显示器独立设置，拼接缝处自动禁用。",
  edgeSettings: "贴边隐藏选项", enableEdge: "启用贴边隐藏", edgeTab: "贴边隐藏", dragTab: "拖拽与缩放", dragMaster: "拖拽与缩放总开关", dragTitle: "从窗口任意位置移动或缩放", dragDescription: "开启后，下方两组鼠标组合才会接管目标窗口；窗口布局仍由 Windows 原生 Snap 负责。", dragSettings: "拖拽组合设置", enableDrag: "启用任意位置拖拽",
  gestureTitle: "鼠标增强工作台", gestureHeading: "画出轨迹，让鼠标多做一步", gestureDescription: "内置 5 种起步方式，也可以继续录制自己的单笔图案；短按仍保持原鼠标功能。", enableGesture: "启用鼠标增强",
  moreGlobal: "通用设置", moreDescription: "所有功能共用的应用级设置", config: "配置管理", export: "导出配置", import: "导入配置", reset: "恢复默认", language: "界面语言", languageDescription: "选择设置界面显示语言",
  hotzoneParameters: "热区参数", hotzoneParametersDescription: "触发角生效范围与响应频率",
  chinese: "中文", english: "English", enabled: "总开关已开启", disabled: "总开关已关闭", edgeEnabled: "开启总开关后可调整", gestureGlobal: "全局识别设置", allGestures: "对全部手势生效",
  direct: "直接触发", add: "新增", clickToDefine: "点击边角定义动作", edgeCaption: "外轮廓可用，拼接缝已禁用", focusMonitor: "一次只专注设置一块屏幕", chooseBelow: "选择下方功能开始设置",
  themeLight: "切换到浅色主题", themeDark: "切换到深色主题", action: "执行动作", shortcut: "快捷键", command: "命令", utoolsCommand: "uTools 指令", customAction: "自定义动作", noAction: "尚未设置动作", enterValue: "请输入参数",
  waitingForDisplays: "等待助手检测显示器后可编辑热区设置。",
  hoverDelay: "当前悬停延迟", cooldown: "当前触发冷却", addHotzoneVariant: "新增热区组合", deleteHotzoneVariant: "删除当前组合", cancelHotzoneVariant: "取消新增组合", recordHotzoneVariant: "录制新热区组合", cancel: "取消", addGestureVariant: "新增手势组合", deleteGestureVariant: "删除当前手势组合", recordGestureVariant: "录制新手势组合", enableCurrentGesture: "启用当前手势",
  screenshotOutput: "矩形截图输出", pinAndOcr: "贴图与本地文字识别", pin: "生成贴图", recognizeCopy: "识别并复制", pinRecognize: "贴图并识别", ocrLanguage: "识别语言", auto: "自动", unavailable: "（不可用）", simplifiedChinese: "简体中文", notInstalled: "（未安装）", englishLanguage: "英文", move: "移动", dragImage: "左键拖动图片", resize: "缩放", dragEdges: "拖动四边或四角", opacity: "透明度", scrollWheel: "滚动鼠标滚轮", textRecognition: "文字识别", rightClickOcr: "贴图右键执行 OCR", holdLeft: "按住左键，一笔画完", recommendTwo: "建议至少录制 2 次", recordOneMore: "再录 1 次更稳定", enoughSamples: "样本充足", duplicate: "制作副本", rerecord: "重新录制", enabledState: "已启用", disabledState: "已停用", recognizeThen: "识别后执行", triggerMouseButton: "触发鼠标键", rightButton: "右键", middleButton: "中键", sideButton1: "侧键 1", sideButton2: "侧键 2", minDistance: "最小移动距离", sensitivity: "识别灵敏度", showTrail: "显示淡蓝轨迹与名称", showTrailDescription: "按住触发键绘制时提供视觉反馈", fullscreenPause: "全屏应用自动暂停", fullscreenPauseDescription: "游戏和全屏播放时避免误触", pausedGestureApps: "暂停手势的应用", foreground: "当前前台：", addCurrentApp: "添加当前应用", noForeground: "尚未获取", remove: "移除", noPausedGestureApps: "所有应用都会响应手势；需要排除时可添加当前前台应用。", pausedDragApps: "不接管拖拽与缩放的应用", noPausedDragApps: "所有应用都会响应；设计软件等使用 Alt + 鼠标时可添加当前前台应用。",
  edgeSize: "热区宽度", pollInterval: "轮询间隔", pausedApps: "暂停应用", noPausedApps: "还没有暂停应用", removeApp: "移除", addApp: "添加",
  stageDownload: "下载", stageVerify: "校验", stageInstall: "安装", stageReady: "就绪",

  // 桌面宿主专有
  desktopShortSubtitle: "桌面宿主", pickupGroup: "拾取", edgeHideTutorial: "贴边隐藏教程", edgeHideTutorialBody: "拖到屏幕外边缘并松开，窗口自动收起；移到露出区域即可恢复。", viewEdgeHideTutorial: "查看贴边隐藏教程", maximizeWindow: "最大化窗口", minimizeWindow: "最小化窗口", moveWindow: "移动窗口", resizeWindow: "缩放窗口", moveWindowButton: "移动窗口鼠标键", resizeWindowButton: "缩放窗口鼠标键", recordMoveModifiers: "录制移动窗口修饰键", recordResizeModifiers: "录制缩放窗口修饰键", defaultAltLeft: "默认 Alt + 左键", defaultAltRight: "默认 Alt + 右键", snapHint: "显示吸附提示", snapHintDescription: "松开后会收纳时，在目标屏幕边缘显示蓝色强调线", expansionOutline: "显示展开轮廓", expansionOutlineDescription: "关闭只隐藏窗口收纳后的淡白轮廓，边缘恢复仍可触发", keepExpandedWhenForeground: "前台窗口保持展开", keepExpandedWhenForegroundDescription: "窗口展开后仍在使用时，不会因为鼠标离开而自动收回", collapseTriggers: "收纳触发方式", collapseTriggersHint: "两种方式独立生效，满足任意一种即可触发", nearEdge: "靠近边缘", nearEdgeDescription: "窗口边缘进入指定像素范围", moveOutRatio: "移出比例", moveOutRatioDescription: "窗口移出屏幕达到自身比例", enableNearEdge: "启用靠近边缘触发", enableMoveOutRatio: "启用移出比例触发", edgeDistance: "边缘距离", windowRatio: "窗口比例", stripSize: "露出宽度", restoreDelay: "恢复延迟", collapseDelay: "收缩延迟", excludedApps: "不收缩的应用", noExcludedApps: "还没有排除应用", pinTooltip: "置顶小图钉", pinTooltipDescription: "跟随置顶窗口，点击即可取消置顶", enablePin: "启用置顶小图钉", diagnostics: "复制诊断", diagnosticsCopied: "诊断信息已复制", helperSection: "后台助手", helperVersion: "助手 {version}", helperInstallDirUnknown: "尚未确定安装目录", helperInstallIncomplete: "安装不完整", helperFilesMissing: "后台助手文件缺失", helperNotRunnable: "当前安装无法启动系统功能，请重新安装便捷窗口。", helperRole: "负责系统监听与窗口操作", openFeature: "打开功能", closeFeature: "关闭功能", publicDownload: "公开下载仓库", runtimeRunning: "功能正在运行", runtimeStarting: "正在启动后台助手", runtimeOff: "功能已关闭", runtimeOffDetail: "总开关关闭时后台助手同步停止，不占用后台资源", runtimeStartingDetail: "总开关打开后会自动启动并连接后台助手", masterState: "功能总开关", masterOn: "已打开", masterOffState: "已关闭", recentAction: "最近动作", currentState: "当前状态", brandName: "便捷窗口", gestureLibrary: "手势库", currentGesture: "当前手势", builtinGestures: "内置手势", sampleCount: "{count} 个样本", screenshotMode: "截图模式", regionScreenshot: "截图贴图", newGesture: "新手势", gestureNameLabel: "手势名称", gestureLibraryHint: "5 个内置模板始终保留；新手势会出现在同一列表中。", gestureBuiltinCount: "个内置", actionGestures: "动作手势", actionGesturesDescription: "复制、粘贴、窗口与系统动作", gestureOverview: "鼠标增强功能总览", recordPatternHint: "录制图案并绑定任意可用动作", builtinCount: "内置 5 种起步方式", gestureVariantRecording: "点击输入组合键", moveResizeConflict: "移动与缩放组合重复，请修改其中一项", gestureTooShort: "轨迹过短，请重新录制", gestureSamplesCleared: "旧样本已清除，请重新录制", gestureCreated: "已创建手势，请录制 2–3 次", gestureDuplicateCreated: "手势副本已创建", gestureSamplesUpdated: "手势样本已更新", gestureDeleted: "手势已删除", gestureVariantDeleted: "手势组合变体已删除", gestureVariantAdded: "手势组合已添加，请设置动作", variantDeleted: "组合变体已删除", variantAdded: "组合已添加，请设置动作", deleteVariant: "删除当前组合变体", hotzoneVariantAdded: "新增热区组合", newVariant: "新增组合", delete: "删除", edit: "编辑", createNew: "＋ 新建", legacySample: "旧版", configExported: "配置已导出", configImported: "配置已导入", configExportCancelled: "已取消导出", configImportCancelled: "已取消导入", configInvalid: "配置文件格式无效", configResetDone: "已恢复默认，等待 helper 连接", defaultConfigSent: "默认配置已发送", configSavedApplying: "正在保存设置…", displaysMigrated: "显示器配置已迁移并应用", bridgeMissingDesktop: "桌面宿主桥接未加载，请重新启动应用", helperUpgradeFailed: "helper 自动升级失败，请停止后重新打开应用", helperUpgradeStarted: "新版 helper 已启动，正在验证版本", helperUpgradeStartFailed: "新版 helper 启动失败", helperStartedConnecting: "helper 已启动，正在连接", helperRecovered: "helper 已自动恢复，正在同步配置", helperErrorTitle: "helper 运行错误", helperRecoveryFailed: "helper 自动恢复失败", edgeUpdated: "边条状态已更新", connectionOk: "连接正常", connectionTesting: "测试中", connectionFailed: "测试失败", connectionTest: "连接测试", pickerHint: "不点卡片直接进面板", displayAndPreview: "显示器与功能预览", featureNavigation: "功能导航", gestureRecognition: "手势已识别", ocrCopiedShort: "OCR · 文字已复制",

  // helper 状态文案（两套前端共用）
  statusWaiting: "等待连接", statusHelperSync: "helper 已连接，正在同步配置", statusHelperStopped: "helper 已停止",
  statusConfigApplied: "配置已应用", statusConfigAdjusted: "配置已应用，部分值已由 helper 调整", statusDisplaysDetected: " 个显示器已识别", statusRuntimeUpdated: "运行状态已更新",
  statusActionTriggered: "动作已触发", statusOcrCopied: "文字已复制", statusHelperResponse: "helper 响应正常",
  statusMasterOff: "功能总开关已关闭，后台助手未启动", statusHelperInstallNeeded: "安装后台助手后，热区与贴边功能才会生效", statusBridgeMissing: "安全桥接未加载，请在 uTools 中重新加载插件",
  statusChecking: "正在检查 helper 响应", statusRequestNotSent: "连接请求未发送", statusTestTimeout: "连接测试超时",
  statusSavedApplying: "已自动保存，正在应用", statusConfigSent: "配置已发送，等待确认", statusSavedWaiting: "设置已保存，等待 helper 连接", statusForegroundMissing: "尚未获取到当前前台应用",
  statusGestureLimit: "手势数量已达到上限", statusHelperRecovering: "helper 意外退出，正在自动恢复（1/1）", statusHelperRestarting: "helper 已重新启动，正在验证连接", statusHelperStarting: "正在连接 helper", statusHelperStopping: "正在停止 helper…",
  statusRuntimeIdle: "等待 helper 上报显示器", statusNoAction: "尚无动作", statusConnectionLost: "连接已断开", statusHelperRecoveringConnection: "后台助手连接已断开，正在尝试恢复", statusHelperRecoveryStopped: "后台助手连续异常退出，已停止自动恢复", statusHelperStartFailed: "无法启动 helper",

  // 带占位符的状态与提示（用 format() 填值）
  moreSettings: "更多设置", masterStateDetail: "同时控制规则与助手生命周期", removeHelper: "删除助手文件", recordShortcut: "录制快捷键", cancelGestureVariant: "取消新增手势组合", runtimeRunningDetail: "{summary}，已启用规则正在生效",
  triggerSlideDown: "向下移动", triggerSlideUp: "向上移动", triggerSlideLeft: "向左移动", triggerSlideRight: "向右移动",
  statusHelperProtocol: "helper 协议不兼容，需要协议 v{min}-v{max}", statusOcrCopiedCount: "已识别并复制 {count} 个字符", statusConfigSaveFailed: "配置保存失败：{error}",
  statusVariantLimit: "最多可添加 {max} 个组合", statusShortcutTaken: "快捷键 {value} 已被其他动作使用", statusAppAdded: "已添加 {app}",
  statusGestureLimitCount: "手势数量已达到 {max} 个上限", statusGestureSampleSaved: "已保存第 {count} 个样本，建议继续录制",
  gestureCopyName: "{name} 副本", gestureDeleteConfirm: "删除手势“{name}”？", gestureSimilarity: "与“{name}”较相似（{score}%），建议重录",
  helperRecoveryFailedDetail: "helper 自动恢复失败：{error}", helperVersionLegacy: "旧版", helperUpgradeProgress: "正在升级 helper {current} → {expected}",
  configExportFailed: "导出失败：{error}", configImportFailed: "导入失败：{error}", diagnosticsFailed: "诊断读取失败：{error}",
  regionScreenshotHint: "画矩形截图，并悬浮在桌面", provided: "已提供", customGesture: "自定义手势", leftButton: "左键",

  // 音量/亮度调节反馈（桌面提示条与 uTools 状态栏共用同一套措辞）
  adjustmentVolume: "音量", adjustmentBrightness: "亮度", adjustmentPending: "调节中", adjustmentMuted: "静音", adjustmentFailed: "调节失败", adjustmentReading: "正在读取设备",
  // 覆盖窗自诊断（仅插件用，桌面保留同键以保持字典逐字节一致）
  hudUnavailable: "提示条不可用", hudNoWindowApi: "uTools 没有独立窗口接口", hudPageNotReady: "提示条页面没加载", hudPreloadNotReady: "提示条脚本没就绪"
} as const;

export const en: Record<keyof typeof zh, string> = {
  gestureNotRecognized: "Gesture not recognized; cancelled.",
  gestureUp: "Up · Copy",
  gestureDown: "Down · Paste",
  gestureL: "L · Close",
  gestureCircle: "Circle · Topmost",
  gestureRectangle: "Region capture",
  errorInvalidConfig: "Invalid settings. Import a valid configuration or restore defaults.",
  errorUnsupportedSchema: "Incompatible settings version. Update the app and helper together.",
  errorConfigSave: "Could not save settings. Check disk space and directory permissions.",
  errorUnknownMessage: "The helper does not support this request. Update the app and helper together.",
  errorRuntime: "The system operation failed. See the diagnostic log for details.",
  errorInputMonitor: "Could not read input or window state. Check system permissions.",
  errorWindow: "The window operation failed. Check the target window and permissions.",
  errorAction: "The action failed. Check its parameters and system permissions.",
  errorScreenCapture: "Screen capture failed. Check screen recording permissions.",
  errorOcr: "Text recognition failed. Check the installed OCR language packs.",
  errorVolume: "Could not adjust volume. Check the audio device and permissions.",
  errorBrightness: "Could not adjust brightness. Check the display’s brightness control support.",
  windowPinned: "Pinned: {title}",
  windowUnpinned: "Unpinned: {title}",
  unnamedWindow: "Window",
  trayShow: "Open settings",
  trayAutostart: "Start at login",
  trayQuit: "Quit Convenient Window",
  configImportTitle: "Import Convenient Window settings",
  configExportTitle: "Export Convenient Window settings",
  configFileType: "JSON settings",

  connected: "Connected", connecting: "Connecting", disconnected: "Disconnected", displays: " displays", displayPreview: "Waiting for helper", checkConnection: "Check connection",
  chooseArea: "Click a corner to choose a zone", chooseEdge: "Click a window edge to enable hiding", drawGesture: "Hold the trigger button and draw anywhere", chooseFeature: "Choose a feature below to configure",
  power: "Center", hotzones: "Corners", edgeHide: "Windows", gestures: "Gestures", more: "More", close: "Close",
  hotzoneSubtitle: "Corner actions", edgeSubtitle: "Hide and restore", dragSubtitle: "Move and resize", gestureSubtitle: "One stroke · Global", moreSubtitle: "Global settings", powerSubtitle: "Features and helper",
  hotzoneMaster: "Corner switch", hotzoneTitle: "Use corners for actions", hotzoneDescription: "Applies to this display. Turning it off keeps settings.",
  hotzoneSettings: "Corner settings", unifiedOn: "Master switch", keepConfig: "Settings kept", enableHotzones: "Enable corners",
  edgeMaster: "Edge switch", edgeTitle: "Hide windows at edges", edgeDescription: "Per display. Seams are disabled.",
  edgeSettings: "Edge options", enableEdge: "Enable edge hide", edgeTab: "Edge hide", dragTab: "Move / resize", dragMaster: "Drag switch", dragTitle: "Move or resize anywhere", dragDescription: "Two mouse bindings control the window. Windows Snap stays unchanged.", dragSettings: "Mouse bindings", enableDrag: "Enable dragging",
  gestureTitle: "Mouse gestures", gestureHeading: "Draw a stroke to act", gestureDescription: "Use built-ins or record your own. A short press stays unchanged.", enableGesture: "Enable gestures",
  moreGlobal: "General settings", moreDescription: "App-level settings shared by every feature", config: "Configuration", export: "Export", import: "Import", reset: "Reset", language: "Language", languageDescription: "Display language",
  hotzoneParameters: "Corner parameters", hotzoneParametersDescription: "Trigger area and response rate",
  chinese: "中文", english: "English", enabled: "Master on", disabled: "Master off", edgeEnabled: "Turn on to edit", gestureGlobal: "Recognition", allGestures: "All gestures",
  direct: "Direct", add: "Add", clickToDefine: "Click a corner to set an action", edgeCaption: "Outer edges work; seams do not", focusMonitor: "Edit one display at a time", chooseBelow: "Choose a feature below",
  themeLight: "Use light theme", themeDark: "Use dark theme", action: "Action", shortcut: "Shortcut", command: "Command", utoolsCommand: "uTools command", customAction: "Custom action", noAction: "No action", enterValue: "Enter value",
  waitingForDisplays: "Hot corner settings can be edited after the helper detects your displays.",
  hoverDelay: "Hover delay", cooldown: "Cooldown", addHotzoneVariant: "Add hot corner keys", deleteHotzoneVariant: "Delete keys", cancelHotzoneVariant: "Cancel", recordHotzoneVariant: "Record hot corner keys", cancel: "Cancel", addGestureVariant: "Add gesture keys", deleteGestureVariant: "Delete keys", recordGestureVariant: "Record gesture keys", enableCurrentGesture: "Enable gesture",
  screenshotOutput: "Region output", pinAndOcr: "Pin + local OCR", pin: "Pin", recognizeCopy: "OCR + copy", pinRecognize: "Pin + OCR", ocrLanguage: "OCR language", auto: "Auto", unavailable: " (unavailable)", simplifiedChinese: "Chinese", notInstalled: " (not installed)", englishLanguage: "English", move: "Move", dragImage: "Left-drag image", resize: "Resize", dragEdges: "Drag edges/corners", opacity: "Opacity", scrollWheel: "Use mouse wheel", textRecognition: "OCR", rightClickOcr: "Right-click image", holdLeft: "Hold left and draw", recommendTwo: "Record at least twice", recordOneMore: "One more is better", enoughSamples: "Enough samples", duplicate: "Duplicate", rerecord: "Record again", enabledState: "Enabled", disabledState: "Disabled", recognizeThen: "After match", triggerMouseButton: "Trigger button", rightButton: "Right", middleButton: "Middle", sideButton1: "Side 1", sideButton2: "Side 2", minDistance: "Min. distance", sensitivity: "Sensitivity", showTrail: "Show trail + name", showTrailDescription: "Show feedback while drawing", fullscreenPause: "Pause in fullscreen", fullscreenPauseDescription: "Avoid triggers in games/video", pausedGestureApps: "Paused apps", foreground: "Foreground: ", addCurrentApp: "Add current", noForeground: "Unavailable", remove: "Remove", noPausedGestureApps: "All apps respond. Add the foreground app to pause gestures.", pausedDragApps: "Apps that keep Alt + mouse", noPausedDragApps: "All apps respond. Add the foreground app so design software keeps Alt + mouse.",
  edgeSize: "Zone width", pollInterval: "Poll interval", pausedApps: "Paused apps", noPausedApps: "No paused apps", removeApp: "Remove", addApp: "Add",
  stageDownload: "Download", stageVerify: "Verify", stageInstall: "Install", stageReady: "Ready",

  desktopShortSubtitle: "Desktop host", pickupGroup: "Pickup", edgeHideTutorial: "Edge hide tutorial", edgeHideTutorialBody: "Drag a window past the screen edge and release to hide it; move back to the revealed strip to restore.", viewEdgeHideTutorial: "View edge hide tutorial", maximizeWindow: "Maximize window", minimizeWindow: "Minimize window", moveWindow: "Move window", resizeWindow: "Resize window", moveWindowButton: "Move window button", resizeWindowButton: "Resize window button", recordMoveModifiers: "Record move modifiers", recordResizeModifiers: "Record resize modifiers", defaultAltLeft: "Default Alt + left", defaultAltRight: "Default Alt + right", snapHint: "Show snap hint", snapHintDescription: "Show a blue line at the target edge when releasing would hide the window", expansionOutline: "Show expand outline", expansionOutlineDescription: "Hides only the pale outline; the edge still restores the window", keepExpandedWhenForeground: "Keep expanded in use", keepExpandedWhenForegroundDescription: "While the expanded window is still in use, moving the mouse away does not collapse it", collapseTriggers: "Hide triggers", collapseTriggersHint: "Both work independently; either one triggers", nearEdge: "Near the edge", nearEdgeDescription: "The window edge enters the configured pixel range", moveOutRatio: "Moved out ratio", moveOutRatioDescription: "The window moves past its own ratio off screen", enableNearEdge: "Enable near-edge trigger", enableMoveOutRatio: "Enable moved-out trigger", edgeDistance: "Edge distance", windowRatio: "Window ratio", stripSize: "Strip width", restoreDelay: "Restore delay", collapseDelay: "Collapse delay", excludedApps: "Apps that never hide", noExcludedApps: "No excluded apps", pinTooltip: "Topmost pin", pinTooltipDescription: "Follows the pinned window; click to unpin", enablePin: "Enable topmost pin", diagnostics: "Copy diagnostics", diagnosticsCopied: "Diagnostics copied", helperSection: "Helper", helperVersion: "Helper {version}", helperInstallDirUnknown: "Install directory unknown", helperInstallIncomplete: "Installation incomplete", helperFilesMissing: "Helper files missing", helperNotRunnable: "This installation cannot start system features; reinstall Convenient Window.", helperRole: "Handles system hooks and window actions", openFeature: "Open", closeFeature: "Close", publicDownload: "Public downloads", runtimeRunning: "Features running", runtimeStarting: "Starting helper", runtimeOff: "Features off", runtimeOffDetail: "The helper stops too, so nothing runs in the background", runtimeStartingDetail: "Turning the master switch on starts and connects the helper", masterState: "Master switch", masterOn: "On", masterOffState: "Off", recentAction: "Recent action", currentState: "Status", brandName: "Convenient Window", gestureLibrary: "Gesture library", currentGesture: "Current gesture", builtinGestures: "Built-in", sampleCount: "{count} samples", screenshotMode: "Screenshot mode", regionScreenshot: "Region screenshot", newGesture: "New gesture", gestureNameLabel: "Gesture name", gestureLibraryHint: "The 5 built-in templates always stay; new gestures appear in the same list.", gestureBuiltinCount: " built-in", actionGestures: "Action gestures", actionGesturesDescription: "Copy, paste, window and system actions", gestureOverview: "Mouse enhancement overview", recordPatternHint: "Record a pattern and bind any available action", builtinCount: "5 built-in starting points", gestureVariantRecording: "Click to enter keys", moveResizeConflict: "Move and resize use the same binding; change one of them", gestureTooShort: "Stroke too short; record again", gestureSamplesCleared: "Old samples cleared; record again", gestureCreated: "Gesture created; record 2–3 times", gestureDuplicateCreated: "Gesture duplicated", gestureSamplesUpdated: "Gesture samples updated", gestureDeleted: "Gesture deleted", gestureVariantDeleted: "Gesture keys deleted", gestureVariantAdded: "Gesture keys added; set an action", variantDeleted: "Keys deleted", variantAdded: "Keys added; set an action", deleteVariant: "Delete keys", hotzoneVariantAdded: "Add hot corner keys", newVariant: "Add keys", delete: "Delete", edit: "Edit", createNew: "＋ New", legacySample: "Legacy", configExported: "Settings exported", configImported: "Settings imported", configExportCancelled: "Export cancelled", configImportCancelled: "Import cancelled", configInvalid: "Invalid configuration file", configResetDone: "Defaults restored; waiting for helper", defaultConfigSent: "Default settings sent", configSavedApplying: "Saving settings…", displaysMigrated: "Display profile migrated and applied", bridgeMissingDesktop: "Desktop host bridge missing; restart the app", helperUpgradeFailed: "Helper upgrade failed; stop and reopen the app", helperUpgradeStarted: "New helper started; verifying version", helperUpgradeStartFailed: "New helper failed to start", helperStartedConnecting: "Helper started; connecting", helperRecovered: "Helper recovered; syncing settings", helperErrorTitle: "Helper error", helperRecoveryFailed: "Helper recovery failed", edgeUpdated: "Edge state updated", connectionOk: "Connected", connectionTesting: "Testing", connectionFailed: "Test failed", connectionTest: "Connection test", pickerHint: "Open the panel without picking a card", displayAndPreview: "Display and feature preview", featureNavigation: "Feature navigation", gestureRecognition: "Gesture recognized", ocrCopiedShort: "OCR · text copied",

  statusWaiting: "Waiting", statusHelperSync: "Helper connected; syncing settings", statusHelperStopped: "Helper stopped",
  statusConfigApplied: "Settings applied", statusConfigAdjusted: "Settings applied; helper adjusted some values", statusDisplaysDetected: " displays detected", statusRuntimeUpdated: "Runtime updated",
  statusActionTriggered: "Action triggered", statusOcrCopied: "Text copied", statusHelperResponse: "Helper responded",
  statusMasterOff: "Master switch off; helper is not running", statusHelperInstallNeeded: "Install the helper to enable hot corners and edge hide", statusBridgeMissing: "Secure bridge missing; reload the plugin in uTools",
  statusChecking: "Checking helper", statusRequestNotSent: "Request was not sent", statusTestTimeout: "Connection test timed out",
  statusSavedApplying: "Saved; applying", statusConfigSent: "Settings sent; waiting for confirmation", statusSavedWaiting: "Saved; waiting for helper", statusForegroundMissing: "Foreground app unavailable",
  statusGestureLimit: "Gesture limit reached", statusHelperRecovering: "Helper exited; recovering (1/1)", statusHelperRestarting: "Helper restarted; checking connection", statusHelperStarting: "Connecting to helper", statusHelperStopping: "Stopping helper…",
  statusRuntimeIdle: "Waiting for helper display data", statusNoAction: "No action yet", statusConnectionLost: "Connection lost", statusHelperRecoveringConnection: "Helper connection lost; attempting recovery", statusHelperRecoveryStopped: "Helper exited repeatedly; recovery stopped", statusHelperStartFailed: "Unable to start helper",

  moreSettings: "More settings", masterStateDetail: "Controls rules and the helper lifecycle", removeHelper: "Remove helper files", recordShortcut: "Record shortcut", cancelGestureVariant: "Cancel gesture keys", runtimeRunningDetail: "{summary}; enabled rules are active",
  triggerSlideDown: "Slide down", triggerSlideUp: "Slide up", triggerSlideLeft: "Slide left", triggerSlideRight: "Slide right",
  statusHelperProtocol: "Helper protocol incompatible; needs protocol v{min}-v{max}", statusOcrCopiedCount: "Copied {count} characters", statusConfigSaveFailed: "Could not save settings: {error}",
  statusVariantLimit: "Up to {max} key combinations", statusShortcutTaken: "Shortcut {value} is already assigned", statusAppAdded: "Added {app}",
  statusGestureLimitCount: "Gesture limit reached ({max})", statusGestureSampleSaved: "Sample {count} saved; keep recording",
  gestureCopyName: "{name} copy", gestureDeleteConfirm: "Delete the gesture “{name}”?", gestureSimilarity: "Close to “{name}” ({score}%); record again",
  helperRecoveryFailedDetail: "Helper recovery failed: {error}", helperVersionLegacy: "Legacy", helperUpgradeProgress: "Upgrading helper {current} → {expected}",
  configExportFailed: "Export failed: {error}", configImportFailed: "Import failed: {error}", diagnosticsFailed: "Could not read diagnostics: {error}",
  regionScreenshotHint: "Draw a region and pin it on the desktop", provided: "Provided", customGesture: "Custom gesture", leftButton: "Left",

  adjustmentVolume: "Volume", adjustmentBrightness: "Brightness", adjustmentPending: "Adjusting", adjustmentMuted: "Muted", adjustmentFailed: "Adjustment failed", adjustmentReading: "Reading device",
  hudUnavailable: "Indicator unavailable", hudNoWindowApi: "No window API in this uTools", hudPageNotReady: "Indicator page did not load", hudPreloadNotReady: "Indicator script did not load"
};

export type UiKey = keyof typeof zh;

export function translator(language: Language): (key: UiKey) => string {
  const dictionary: Record<UiKey, string> = language === "en-US" ? en : zh;
  return (key) => dictionary[key] ?? key;
}
