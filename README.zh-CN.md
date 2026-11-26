<div align="center">

<img src="docs/images/app-icon.png" width="72" alt="便捷窗口图标">

<h1>便捷窗口</h1>

<p><strong>窗口靠边收起，常用操作随手触发。</strong></p>
<p>为 Windows 桌面提供贴边隐藏、触发角、鼠标手势与窗口布局。</p>

<p>
  <a href="https://github.com/ximizhou/convenient_window_free/releases/tag/v0.6.11"><img src="https://img.shields.io/badge/桌面版-0.6.11-3f6ee4?style=flat-square" alt="桌面正式版 0.6.11"></a>
  <img src="https://img.shields.io/badge/Windows_11-x64-59636e?style=flat-square" alt="Windows 11 x64">
</p>

<p>
  <a href="https://github.com/ximizhou/convenient_window_free/releases/download/v0.6.11/convenient-window-0.6.11-windows-x64-setup.exe"><strong>下载 Windows 安装版</strong></a> ·
  <a href="https://github.com/ximizhou/convenient_window_free/releases/download/v0.6.11/convenient-window-0.6.11-windows-x64-portable.zip">便携版</a> ·
  <a href="docs/release-notes/0.6.11.md">更新说明</a>
</p>

<p><strong>简体中文</strong> · <a href="README.md">English</a></p>

</div>

![便捷窗口桌面版，浅色运行中心与多显示器设置](docs/images/running-center.png)

<p align="center"><sub>独立运行 · 系统托盘常驻 · 中英双语 · 深浅主题</sub></p>

## 少一点重复操作

| 功能 | 可以做什么 |
| --- | --- |
| **贴边隐藏** | 把暂时不用的窗口拖到屏幕边缘收起，鼠标移到边条即可展开。 |
| **触发角与热区** | 为每块屏幕的四角和四边设置动作：显示桌面、调节音量、执行快捷键。 |
| **鼠标手势** | 按住鼠标画出手势，执行窗口操作、快捷键或自定义动作。 |
| **窗口布局** | 记录窗口的位置、大小与状态，需要时一键恢复排列。 |
| **拖动与置顶** | 从窗口任意位置拖动、缩放，把需要对照的窗口保持在前面。 |
| **截图、贴图与识字** | 截取区域并悬浮显示，使用 Windows 本地 OCR 提取文字。 |

## 桌面功能一览

### 每块屏幕，单独设置

选择显示器，为边角分配动作，按习惯调整热区大小和触发方式。

![桌面版触发角与热区设置](docs/images/hot-zones.png)

### 留住常用的窗口排列

保存当前布局，之后恢复仍在运行的窗口；已关闭或无权操作的窗口会跳过。

![桌面版窗口布局](docs/images/window-layouts.png)

<details>
<summary>查看鼠标手势设置</summary>

![桌面版鼠标手势设置](docs/images/mouse-gestures.png)

</details>

## 安装后怎么用

1. 下载上方的 **Windows 安装版**并安装，或解压便携版运行。
2. 从系统托盘打开便捷窗口，开启总开关，再启用需要的功能。
3. 试试贴边隐藏：把未最大化的普通窗口拖向已启用的屏幕边缘，出现蓝线后松手；顶部需要等蓝线铺满。鼠标移回边条即可展开。

关闭设置窗口后，功能继续在托盘运行；从托盘菜单退出才会结束。助手已包含在安装包里，无需另行下载。

<details>
<summary>支持范围、更新与使用说明</summary>

- 当前已发布并验收的桌面版面向 **Windows 11 x64**；macOS/Linux 暂无本版可交付安装包。
- 窗口操作、设置和 OCR 在本机完成；OCR 不上传截图，识别语言取决于 Windows 语言包。
- 更新需手动下载。便携版替换程序文件即可，设置保存在本机应用数据目录。
- 当前程序未签名，Windows 可能提示未知发布者。[发布页](https://github.com/ximizhou/convenient_window_free/releases/tag/v0.6.11)提供校验值和文件清单，可核对来源与完整性。
- 任务栏材质为默认关闭的实验功能，效果和恢复行为需按系统验证。

[历史版本](https://github.com/ximizhou/convenient_window_free/releases) · [0.6.10 回退版本](https://github.com/ximizhou/convenient_window_free/releases/tag/v0.6.10)

</details>

## 也可以在 DSH 中使用

如果你已经使用 DeepSeek Harness，可以从侧栏打开便捷窗口，使用同一套设置界面。切换页签或最小化 DSH 后，已开启的助手继续工作。

![DSH 侧栏中的便捷窗口](docs/images/dsh-panel.png)

**DSH 插件 0.2.0-beta.3** · 已验收宿主 **0.2.2-rc.5** · Windows 11 x64

[安装与升级](apps/dsh-plugin/README.md#安装) · [插件下载](https://github.com/ximizhou/convenient_window_free/releases/tag/dsh-v0.2.0-beta.3)

插件内置助手，无需单独下载。桌面版和 DSH 的配置独立保存；同时使用时，只开启其中一个版本的窗口助手。其他 DSH 版本需另行验证。

## 反馈与开发

[报告问题](https://github.com/ximizhou/convenient_window_free/issues/new/choose)时，请注明使用哪个版本、DSH/插件/Windows 版本，以及复现步骤。安全问题请按 [SECURITY.md](SECURITY.md) 私下报告。

<details>
<summary><strong>源码结构、构建与验证</strong></summary>

- `apps/desktop/`：独立桌面宿主及共用的 Svelte 设置页。
- `apps/dsh-plugin/`：DSH 接入、原生适配、打包和回归测试。
- `helper/`：唯一 Rust 窗口引擎源码。
- `scripts/`：桌面构建、校验和发布工具。

桌面构建使用 Node.js 24.14.0 和仓库指定的 Rust 工具链；Windows 原生构建需 Visual Studio C++ Build Tools 与 Windows SDK。

```powershell
npm ci
npm run desktop:install
npm run desktop:test
npm run desktop:check
npm run desktop:build
```

[架构](docs/architecture.md) · [测试](docs/testing.md) · [桌面发布](docs/release.md) · [DSH 构建与发布](apps/dsh-plugin/README.md#维护者)

桌面正式资产保持不可变。DSH 的 npm 包与 GitHub 附件使用同一份已验收文件；只修改介绍文档不会替换已发布安装包。

</details>

## 许可证

源码按 [PolyForm Noncommercial 1.0.0](LICENSE) 提供，具体使用条件以许可证为准。此前以 MIT 发布的版本继续保留当时的许可。
