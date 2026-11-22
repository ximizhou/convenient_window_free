# 便捷窗口 · DSH 桌面控制台（Windows 11 x64 测试版）

打开 DeepSeek Harness 左侧的 **便捷窗口**，会看到独立桌面版的显示器预览、底部功能入口和右侧设置抽屉。不是打开外部窗口，也不需要安装桌面版、uTools、模型或 API Key。包内自带正式 Rust 助手。

### 安装或从旧插件升级

1. DSH 第一次使用时先打开一次。然后从**系统托盘彻底退出 DSH**，只关窗口不一定是退出。准备好 `.tgz`，不用解压，不用先卸载旧版。
2. 在安装包所在目录打开 PowerShell（`dsh` 不在 PATH 时用 DSH 安装目录下 `resources/runtime/cli/bin/dsh.cmd` 的完整路径）：

   ```powershell
   $pkg = (Resolve-Path '.\dsh-plugin-convenient-window-0.2.0-beta.2.tgz').Path
   dsh plugin --profile desktop add "file:$pkg" --ignore-scripts
   ```

   日常安装保持联网：DSH 可能核对原有插件的依赖，不是另下便捷窗口助手。不要默认加 `--offline`；已有插件的版本清单没缓存全时会报 `ERR_PNPM_NO_MATCHING_VERSION`，不需要清缓存或降级。
3. 重新打开 DSH → **便捷窗口**。首次安装或从 `0.1.0-beta.1` 升级，先检查配置，再打开右上角的**功能总开关**。切到其他 DSH 页签或最小化不停止助手；退出 DSH 会退出本插件的助手。下次启动 DSH 时按保存的总开关状态运行，后台也能恢复设置。如果另一个版本的助手正在运行，请先在那个版本里关闭总开关；本插件不会抢占端口。

旧版保存的贴边开关、四边、四角动作和停留时间自动带入。第一次保存新版设置时写入当前 DSH profile 的 `convenient-window/panel-settings.json`，**不覆盖旧版的 `settings.json`**，也不读写独立桌面版或 uTools 的配置。回退可用同一条命令改指旧包；旧版读旧设置，不会理解新版新增选项。保留所用的本地安装包，安装后依赖可能仍引用这个文件。卸载前在电源页关闭本插件的登录自启，再关闭助手并退出 DSH，运行 `dsh plugin --profile desktop remove dsh-plugin-convenient-window`；个人设置保留。

### 对齐了什么？

| 桌面功能 | DSH 本版 |
| --- | --- |
| 显示器预览、屏幕选择、可拖动分栏、深浅主题、中英切换、功能入口编排 | 直接编译桌面版的 Svelte 组件和样式；不重画 |
| 触发角及各边热区、按键与修饰键、贴边收纳、拖拽缩放、置顶图钉 | 使用同一配置模型和 Windows 助手。默认以普通权限运行，电源页可切换助手权限 |
| 鼠标手势、截图贴图、OCR、音量/亮度、任务栏外观 | 同一设置页和运行引擎；OCR 取决于系统语言包，任务栏效果取决于系统与其他美化工具。音量和亮度使用桌面版的全局提示窗，切走页签后仍可显示 |
| 窗口布局记录/恢复、布局快捷键与排除名单 | 走实际助手；页签关闭时，插件宿主仍处理已登记的布局快捷键 |
| 导入/导出、诊断、布局和界面偏好 | JSON 通过本机文件选择/保存；诊断来自本插件助手，设置与布局按 DSH profile 保存 |
| Windows 启动项管理 | 同一套列表、筛选、查看命令、启停和添加程序。添加时使用 Windows 文件选择器，修改前显示相应程序的确认框 |
| 登录自启与管理员模式 | 电源页管理 `Convenient Window (DSH)` 登录项。可以为本次会话或登录时的助手请求 Windows 授权，DSH 本身保持普通权限 |
| 自定义「运行命令」 | 新命令保存前列出内容供确认。触发已配置的动作时由桌面版的普通权限代理执行 |

插件的设置窗口、最小化和托盘入口由 DSH 提供。日常操作都在左侧「便捷窗口」页签内完成。安装和升级时从托盘退出整个 DSH。

底部放不下的入口在右上齿轮的功能入口设置里。贴边体验请用未最大化的普通窗口；多屏 DPI、手势手感、截图/OCR 和任务栏实际效果需在目标机器分别体验，自动化的“设置已应用”不能代替真人验收。

### 维护者

```text
DSH 侧栏 → 独立文档内的桌面 App.svelte + styles.css（同一份源文件）
              ├─ DSH HostBridge：设置、偏好、文件、诊断
              └─ 桌面 HelperClient → 本机适配层 → 同一个正式 Rust 助手
                                 └─ DSH 原生宿主复用桌面监管、权限、命令、启动项与全局提示窗
```

- `src/client.js` 只注册 DSH 插槽；隔离文档避免桌面样式、事件和主题污染 DSH。`src/panel.ts` 接宿主功能；`src/globals.js` 仅对本次构建注入传输和偏好适配，不替换 DSH 的全局对象。构建检查桌面组件确实进入成品，拒绝打入 Tauri 渲染 API。
- `host/panel-controller.js` 复用构建自桌面源码的模型，对写入核对 session/revision，确认回执才显示已应用；不向页面发助手 token，不开放计算任意命令或访问任意文件的接口。旧设置仅用于首次迁移。两个版本使用同一个 Rust helper 源码，桌面版与 uTools 无需随此版重发。
- 产品/助手 **0.6.11**，DSH 适配包 **0.2.0-beta.2**；只声明已验证的 Windows DSH **0.2.2-rc.5**。仅 DSH 变化，先改宿主适配并升级适配包号。不要改已发布的产品版本或 stable 资产。
- 在 `apps/dsh-plugin` 运行 `npm ci`、`npm test`。按 `runtime-assets.json` 准备正式资产后执行 `npm run stage:helper -- <解压目录>`。使用 native 工具链执行 `npm run build:native -- <与桌面 Cargo.lock 对应的 THIRD-PARTY-NOTICES.txt>`，再执行 `npm run test:native`；测试不抢占其他助手。`npm pack` 生成单包。`npm run test:desktop -- "<DSH.exe>" "<新版.tgz>" ["<已验收的旧版.tgz>"]` 在隔离配置验最终包（可验证升级）；同时保留日常有其他插件时不加 `--offline` 的安装回归。测试不替用户修改登录设置、操作真实窗口布局或自动发布。
- 对外分发仍未进行。后续经确认，使用同一份验收过的包上传[统一的产品 Releases](https://github.com/ximizhou/convenient_window_free/releases)和 npm，预览版用预览 dist-tag。DSH「发现」还需要可信索引收录，不因 npm 发布自动出现；参见[对应版本的市场文档](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.2-rc.5/packages/boot/plugin-market/README.zh.md)。
