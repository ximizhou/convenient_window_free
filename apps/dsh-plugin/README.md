# dsh-convenient-window · 便捷窗口

在 Windows 11 x64 的 DeepSeek Harness 侧栏打开 `dsh-convenient-window`，即可设置触发角、贴边隐藏、鼠标手势和窗口布局。设置页使用独立桌面版的界面，插件包内置 Windows 助手。本版面向 DSH `0.2.2-rc.5`。

### 安装与更名升级

1. 准备 `dsh-convenient-window-0.2.0-beta.3.tgz`。旧包名为 `dsh-plugin-convenient-window`。如果此前开启了旧包的 Windows 登录自启，先在旧页签的运行中心关闭它，切换完成后可在新页签重新开启。
2. 从系统托盘退出 DSH，在安装包所在目录打开 PowerShell。第一次使用 DSH 时先打开一次，创建 `desktop` 配置。已装旧名的用户先执行 `dsh plugin --profile desktop remove dsh-plugin-convenient-window`。卸载成功后继续。首次安装直接继续。
3. 安装预构建包。若系统找不到 `dsh`，改用 DSH 安装目录内 `resources/runtime/cli/bin/dsh.cmd` 的完整路径。

   ```powershell
   $pkg = (Resolve-Path '.\dsh-convenient-window-0.2.0-beta.3.tgz').Path
   dsh plugin --profile desktop add "file:$pkg" --ignore-scripts
   ```

4. 打开 DSH，点击左侧 `dsh-convenient-window`。旧名插件保存的界面偏好、窗口布局和功能配置继续使用当前 profile 中的 `convenient-window/`。最初的 `0.1.0-beta.1` 首次迁移时总开关默认关闭。

日常安装保持联网，DSH 可能要核对原有插件依赖。使用本地包安装后请保留 `.tgz`。切页和最小化时助手继续运行。回退时先移除当前包，再用同样的安装命令指定保存的旧版包。卸载命令为 `dsh plugin --profile desktop remove dsh-convenient-window`。

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

插件的设置窗口、最小化和托盘入口由 DSH 提供。日常操作都在左侧 `dsh-convenient-window` 页签内完成。安装和升级时从托盘退出整个 DSH。

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
- 产品/助手 **0.6.11**，DSH 适配包 **0.2.0-beta.3**；只声明已验证的 Windows DSH **0.2.2-rc.5**。仅 DSH 变化，先改宿主适配并升级适配包号。不要改已发布的产品版本或 stable 资产。
- 在 `apps/dsh-plugin` 运行 `npm ci`、`npm test`。按 `runtime-assets.json` 准备正式资产后执行 `npm run stage:helper -- <解压目录>`。使用 native 工具链执行 `npm run build:native -- <与桌面 Cargo.lock 对应的 THIRD-PARTY-NOTICES.txt>`，再执行 `npm run test:native`；测试不抢占其他助手。`npm pack` 生成单包。`npm run test:desktop -- "<DSH.exe>" "<新版.tgz>" ["<已验收的旧版.tgz>"]` 在隔离配置验最终包（可验证升级）；同时保留日常有其他插件时不加 `--offline` 的安装回归。测试不替用户修改登录设置、操作真实窗口布局或自动发布。

### 发布与市场收录

首次公开预览使用 `beta` 标签。先将插件子目录合入公开产品仓库，再在该仓库创建标签 `dsh-v0.2.0-beta.3` 的 GitHub Pre-release，附上通过安装验收的 `.tgz`。桌面版和插件共享[产品 Releases 入口](https://github.com/ximizhou/convenient_window_free/releases)。

在通过安装验收的 `.tgz` 所在目录打开 PowerShell。用自己的 npm 账号登录，按照 npm 的提示完成双重验证。先用 `--dry-run` 预览包名、版本和文件清单，核对后再发布同一个文件。

```powershell
npm.cmd login --registry=https://registry.npmjs.org
$pkg = (Resolve-Path .\dsh-convenient-window-0.2.0-beta.3.tgz).Path
npm.cmd publish "$pkg" --dry-run --access public --tag beta --registry=https://registry.npmjs.org
npm.cmd publish "$pkg" --access public --tag beta --registry=https://registry.npmjs.org
npm.cmd view dsh-convenient-window@beta version dist.integrity --registry=https://registry.npmjs.org
```

发布完成后，用户可以运行 `dsh plugin --profile desktop add dsh-convenient-window@beta --ignore-scripts`。发布方式见 [npm 发布文档](https://docs.npmjs.com/cli/v11/commands/npm-publish/)。

要进入社区 `dshmarket` 搜索列表，给公开仓库添加 `dsh-plugin` topic，再向 `awesome-dsh-plugin` 提交子目录条目。为预览版填写带具体标签的 GitHub Release 附件地址，审核合并后在市场搜索并试装。[市场投稿说明](https://github.com/dsh-market/dsh-market/blob/main/README.zh.md) · [子目录与条目格式](https://github.com/awesome-dsh-plugin/awesome-dsh-plugin/blob/main/contributing.md)。

使用 DSH 自带「插件 → 发现」的用户可以在「索引源」中添加社区目录 `https://cdn.jsdelivr.net/npm/dsh-plugin-catalog/index.json`。目录收录并更新后，可以搜索英文包名或中文描述中的「便捷窗口」。[DSH 发现页与索引说明](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.2-rc.5/packages/boot/plugin-market/README.zh.md)。

本项目对应的投稿文件为 `data/plugins/ximizhou__convenient_window_free--apps-dsh-plugin.yml`。发布附件后可使用以下内容。

```yaml
url: https://github.com/ximizhou/convenient_window_free/tree/main/apps/dsh-plugin
name: ximizhou/convenient_window_free#dsh-convenient-window
category: tools
description:
  en: Configure Windows hot corners, edge hiding, mouse gestures and window layouts in DeepSeek Harness.
  zh: 在 DeepSeek Harness 里用便捷窗口设置 Windows 触发角、贴边隐藏、鼠标手势和窗口布局。
tarball: https://github.com/ximizhou/convenient_window_free/releases/download/dsh-v0.2.0-beta.3/dsh-convenient-window-0.2.0-beta.3.tgz
```
