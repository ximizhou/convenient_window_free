# dsh-convenient-window · 便捷窗口

在 DeepSeek Harness 侧栏管理触发角、贴边隐藏、鼠标手势和窗口布局。设置页直接复用独立桌面版，Windows 助手已装在插件包里，无需再下载一次。

| 插件 | 内置窗口引擎 | 已验收环境 |
| --- | --- | --- |
| **0.2.0-beta.3** | **0.6.11** | Windows 11 x64、DSH **0.2.2-rc.5** |

[查看发布与下载](https://github.com/ximizhou/convenient_window_free/releases/tag/dsh-v0.2.0-beta.3) · [项目首页](../../README.zh-CN.md) · [反馈问题](https://github.com/ximizhou/convenient_window_free/issues/new/choose)

## 安装

### 第一次安装：推荐从 npm 获取

1. 先打开一次 DSH，让它创建 `desktop` 配置，然后从系统托盘**退出 DSH**。
2. 打开 PowerShell，粘贴下面的命令并回车。安装时保持联网。

   ```powershell
   dsh plugin --profile desktop add dsh-convenient-window@0.2.0-beta.3 --ignore-scripts --registry=https://registry.npmjs.org/
   ```

3. 安装成功后重新打开 DSH，点击左侧 **dsh-convenient-window**。
4. 按需开启右上角的功能总开关，再打开需要的功能。首次安装默认关闭总开关。

这里固定安装已验收的版本。想获取 beta 渠道时，可把 `@0.2.0-beta.3` 换成 `@beta`；先检查新版本的宿主兼容说明。请保留版本号或 `@beta`：2026-11-24 核对时，无标签的 npm `latest` 仍指向占位版。

<details>
<summary><strong>PowerShell 提示找不到 dsh，怎么办？</strong></summary>

找到 DSH 的安装目录，再打开其中的 `resources\runtime\cli\bin` 文件夹，找到 `dsh.cmd`。在 PowerShell 中用其完整路径替换命令开头的 `dsh`。下面的路径是占位示例，需要换成你自己的安装位置：

```powershell
& "你的 DSH 安装目录\resources\runtime\cli\bin\dsh.cmd" plugin --profile desktop add dsh-convenient-window@0.2.0-beta.3 --ignore-scripts --registry=https://registry.npmjs.org/
```

</details>

### 也可以安装 GitHub 下载包

从[本版 Release](https://github.com/ximizhou/convenient_window_free/releases/tag/dsh-v0.2.0-beta.3)下载 `dsh-convenient-window-0.2.0-beta.3.tgz`。退出 DSH，在下载目录打开 PowerShell：

```powershell
$pkg = (Resolve-Path -LiteralPath '.\dsh-convenient-window-0.2.0-beta.3.tgz').Path
dsh plugin --profile desktop add "file:$pkg" --ignore-scripts
```

保留这个文件，便于重装或回退。此包与 npm 上的同版本包相同。日常安装不加 `--offline`，因为 DSH 可能还需核对当前配置中其他插件的依赖。

## 升级、旧名称迁移和卸载

**已经是新名称：** 安装前退出 DSH，再执行目标版本的安装命令；使用同一个 profile 保留设置。先阅读目标版本的兼容与迁移说明。

**装过旧名 `dsh-plugin-convenient-window`：**

1. 如果开过旧插件的登录自启，先在旧页签运行中心关闭它。
2. 从托盘退出 DSH，再移除旧包：

   ```powershell
   dsh plugin --profile desktop remove dsh-plugin-convenient-window
   ```

3. 确认移除成功，再按上面的 npm 或 GitHub 方式安装新名称。
4. 重开后检查设置，并按需重新开启登录自启。最早的 `0.1.0-beta.1` 迁移时，总开关默认关闭。

旧名和新名共用当前 profile 的 `convenient-window/` 数据目录；配置、窗口布局与界面偏好在更名验收中保留。配置导出可作为额外备份，但不等于完整备份窗口布局和所有宿主偏好。

**卸载：** 先在插件里关闭登录自启（如果开启），从托盘退出 DSH，然后运行：

```powershell
dsh plugin --profile desktop remove dsh-convenient-window
```

卸载保留 profile 中的设置。回退到保存的旧版包时，先移除当前包，再安装旧包；旧版本未必能读取将来的配置格式，不把回退当成无条件兼容。

## 常见问题

- **安装成功但没有页签：** 确认安装到正在使用的 `desktop` profile，并从托盘完全退出后重开 DSH。
- **助手被占用：** 桌面版、uTools 和 DSH 共用单实例窗口引擎。先在原宿主正常关闭助手，再启用这里的总开关。
- **DSH 升级后不兼容：** 本版只声明已验收的 `0.2.2-rc.5`。记录两边版本并反馈，不要通过删除兼容限制强行启用。
- **部分功能入口没显示：** 在右上角齿轮里的功能入口设置中调整。底部空间有限，并非功能被删除。
- **切页后还能工作吗：** 开启后，切换页签和最小化不停止助手；退出 DSH 会停止本插件拥有的进程。
- **能和桌面版互相同步吗：** 两边配置独立保存，不自动同步。
- **没有找到市场条目：** npm 发布、目录审核和市场可搜索是不同状态。未查到时可先用本页固定版本命令，不要安装名称相近的包。


## 功能范围

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

## 维护者

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

## 发布与市场收录

为已验收的插件提交创建 `dsh-v<插件版本>` Tag，并将实装验收通过的 `.tgz` 附在 GitHub Pre-release 中。桌面和 DSH 插件共用[产品 Releases 入口](https://github.com/ximizhou/convenient_window_free/releases)。

将同一个 `.tgz` 提交到 npm 的 `beta` 渠道。用已登录且支持 `npm stage` 的 npm CLI 先预览包名、版本和文件清单，再创建待发布条目。将路径换成这次验收的文件。

```powershell
$pkg = (Resolve-Path -LiteralPath '<本轮验收的 .tgz>').Path
npm.cmd stage publish "$pkg" --dry-run --access public --tag beta --registry=https://registry.npmjs.org/
npm.cmd stage publish "$pkg" --access public --tag beta --registry=https://registry.npmjs.org/
```

包所有者在 npm 的 **Staged Packages** 页面批准并完成双重验证。批准后查官方源的 `beta` 指向，匿名下载 npm 和 GitHub 两边的包，核对大小及 SHA-256，再在全新的 DSH `desktop` 配置按发布页命令试装。见 [npm 暂存发布说明](https://docs.npmjs.com/cli/v11/commands/npm-stage/)。

要进入社区 `dshmarket` 搜索列表，先确认下方条目的 `main/apps/dsh-plugin` 链接可访问，给公开仓库添加 `dsh-plugin` topic，再向 `awesome-dsh-plugin` 提交子目录条目。为预览版填写带具体标签的 GitHub Release 附件地址，审核合并后在市场搜索并试装。[市场投稿说明](https://github.com/dsh-market/dsh-market/blob/main/README.zh.md) · [子目录与条目格式](https://github.com/awesome-dsh-plugin/awesome-dsh-plugin/blob/main/contributing.md)。

使用 DSH 自带「插件 → 发现」的用户可以在「索引源」中添加社区目录 `https://cdn.jsdelivr.net/npm/dsh-plugin-catalog/index.json`。目录收录并更新后，可以搜索英文包名或中文描述中的「便捷窗口」。[DSH 发现页与索引说明](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.2-rc.5/packages/boot/plugin-market/README.zh.md)。

本项目对应的投稿文件为 `data/plugins/ximizhou__convenient_window_free--apps-dsh-plugin.yml`。发布附件后可使用以下内容。

```yaml
url: https://github.com/ximizhou/convenient_window_free/tree/main/apps/dsh-plugin
name: ximizhou/convenient_window_free#dsh-convenient-window
category: tools
description:
  en: Configure Windows hot corners, edge hiding, mouse gestures and window layouts in DeepSeek Harness.
  zh: 在 DeepSeek Harness 里用便捷窗口设置 Windows 触发角、贴边隐藏、鼠标手势和窗口布局。
icon: https://raw.githubusercontent.com/ximizhou/convenient_window_free/main/docs/images/app-icon.png
tarball: https://github.com/ximizhou/convenient_window_free/releases/download/dsh-v0.2.0-beta.3/dsh-convenient-window-0.2.0-beta.3.tgz
```

另一个 [2BingLing/dsh-market](https://github.com/2BingLing/dsh-market/blob/master/CONTRIBUTING.md) 支持 topic 扫描及插件申请 Issue。填写源码子目录、插件类型 `cordis-plugin`、已验收宿主版本、固定安装包和带版本的安装命令；收录后检查它生成的命令是否错误地指向仓库根目录或无版本 npm 包。

上面的目录条目固定到本次 beta 附件。后续发版须更新条目并复核市场实际安装目标；审核通过、索引更新、搜索可见和安装成功分别确认。npm 页面中的 README 来自发布包，仓库文档更新不会回写已发布 npm 包；只改介绍时不重打同版本包。GitHub Release 正文以 [版本说明](../../docs/release-notes/dsh-0.2.0-beta.3.md) 为准。
