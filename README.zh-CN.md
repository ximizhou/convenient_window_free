# 便捷窗口

简体中文 | [English](README.md)

便捷窗口是一款跨平台桌面增强工具，提供触发角、窗口控制、全局鼠标手势和截图贴图。Windows 保留完整能力，macOS 与 Linux X11 先提供 P0 原生边界，Wayland 只做能力探测和明确降级。

设置会在修改时自动保存并应用。独立桌面版常驻系统托盘，原生 Rust helper 会随桌面应用一同打包。

<p align="center">
  <img src="docs/images/running-center.png" alt="便捷窗口运行中心，已识别两台显示器" width="920" />
</p>

## 下载

请从[最新稳定版](https://github.com/ximizhou/convenient_window_free/releases/latest)下载 Windows 安装包或便携版压缩包；macOS/Linux 资产尚未完成 native 验收，当前不会伪装为可下载版本。

- **安装包（`setup.exe`）**：推荐普通用户使用，按当前用户安装。
- **Portable ZIP**：解压后即可运行，无需安装。
- **已发布系统要求**：Windows 11 x64。

当前发布文件尚未进行代码签名，Windows 可能显示“未知发布者”或 Microsoft Defender SmartScreen 提示。每个 Release 都提供校验和与产物清单，可用于核对下载文件。

## 主要功能

- **触发角与热区**：每台显示器的四角和四边都能独立配置，可分别响应悬停、鼠标按键、滚轮和沿边移动。
- **窗口贴边隐藏**：把窗口收纳到屏幕边缘，通过保留的边条重新唤回，支持多窗口和多显示器；淡白展开轮廓可独立关闭，不影响边缘恢复。
- **任意位置移动与缩放**：通过可配置的修饰键与鼠标组合，在窗口任意位置移动或缩放活动窗口。
- **窗口置顶控制**：让窗口保持在最前方，并可显示窗口内悬浮图钉，快速取消置顶。
- **全局鼠标手势**：把手势绑定到快捷键、系统动作、命令和窗口控制；支持录制和管理自定义手势样本。
- **截图与贴图**：截取屏幕区域并将图片置顶，可移动、缩放、调整透明度、复制或另存为 PNG。
- **本地 OCR**：使用 Windows 11 本地语言能力识别文字并复制结果，不会上传截图。
- **多显示器配置**：针对不同显示器保存独立配置，支持虚拟桌面负坐标。
- **实时保存**：大部分设置无需点击保存，修改后会立即持久化并应用。
- **浅色与深色主题**：默认跟随系统外观，也可以手动选择并记住偏好。

## 界面预览

<table>
  <tr>
    <td width="50%"><img src="docs/images/hot-zones.png" alt="按显示器独立配置触发角" /></td>
    <td width="50%"><img src="docs/images/window-enhancement.png" alt="窗口贴边隐藏配置与预览" /></td>
  </tr>
  <tr>
    <td align="center"><strong>触发角与热区</strong></td>
    <td align="center"><strong>窗口增强</strong></td>
  </tr>
  <tr>
    <td colspan="2"><img src="docs/images/mouse-gestures.png" alt="包含截图与本地 OCR 动作的鼠标手势工作台" /></td>
  </tr>
  <tr>
    <td colspan="2" align="center"><strong>鼠标手势、截图与本地 OCR</strong></td>
  </tr>
</table>

### 贴边隐藏教程

软件内教程展示了完整过程：把窗口拖到屏幕边缘，鼠标移开后窗口自动收纳，再把鼠标移回露出的边条即可恢复。

<p align="center">
  <img src="docs/media/edge-hide-tutorial.gif" alt="贴边隐藏教程动画" width="508" />
</p>

## 快速开始

1. 从[最新 Release](https://github.com/ximizhou/convenient_window_free/releases/latest)下载安装包或 Portable ZIP。
2. 安装应用，或者解压便携版后运行 `ConvenientWindow.exe`。
3. 从系统托盘打开“便捷窗口”。
4. 打开功能总开关，然后配置触发角、窗口增强或鼠标手势。

关闭设置窗口后，程序仍会在系统托盘中运行。你可以通过托盘菜单重新打开设置、控制开机启动或退出程序。

## 隐私与安全

- OCR 通过 Windows 本地 API 完成，识别过程不会上传截图。
- 配置、鉴权令牌、使用数据和日志都保存在应用的本地数据目录中。
- 本地 WebSocket 使用随机令牌保护。
- 平台原生单实例锁会阻止不同宿主集成同时控制不同的 helper 进程（Windows 使用全局 mutex，Unix 使用运行时/缓存目录锁文件）。

如需私下报告安全问题，请查看 [SECURITY.md](SECURITY.md)。

## 平台支持与限制

- Windows 11 x64：完整能力，当前发布和回归平台。
- macOS x64/arm64：支持全局输入、辅助功能窗口控制、显示器和截图；需要辅助功能与屏幕录制权限。OCR、音频、贴边隐藏和任意窗口置顶暂不支持，真实权限与窗口验收待 native runner/机器证据。
- Linux x64 X11：支持全局输入、窗口控制、显示器、截图和 EWMH 置顶；OCR、音频和贴边隐藏暂不支持，真实窗口验收待 native runner/机器证据。
- Linux Wayland：只报告能力并明确降级，不承诺全局输入或任意窗口控制。
- macOS/Linux 发布资产在 native 验收前保持未验证，不会进入下载清单。
- 当前安装包和可执行文件尚未进行代码签名。
- 暂未实现应用自动更新。

## 开发说明

本仓库是以下内容的权威源码：

- `apps/desktop/`：Tauri 2 独立桌面宿主和可复用的 Svelte 界面。
- `helper/`：独立桌面版和宿主集成共同使用的 Rust helper。
- `scripts/`：可复现的构建、打包、烟测和产物审计入口。

宿主集成通过 submodule 使用本仓库，并提供自己的宿主适配层；不会维护第二份 helper 源码。

### 开发环境

- `.node-version` 固定的 Node.js `24.14.0`
- Windows 11 x64：完整行为、NSIS 安装包和便携版
- macOS 13+（x64/arm64）：原生 helper 检查；交互运行需授予辅助功能和屏幕录制权限
- Ubuntu 22.04+ X11：Linux helper 检查；Wayland 仅做能力探测和明确降级
- Rust `1.96.0` 或更新的 stable 工具链；仓库 `rust-toolchain` 只固定 Windows GNU 目标，macOS/Linux 请显式使用 `cargo +stable`

安装依赖并运行前端检查：

```powershell
npm ci
npm run desktop:test
npm run desktop:check
npm run desktop:frontend
```

构建 Svelte 前端、Rust helper sidecar、Tauri 应用、当前用户 NSIS 安装包和便携版压缩包：

```powershell
npm run desktop:build
```

运行打包后的生命周期和产物检查：

```powershell
npm run desktop:runtime-smoke
npm run desktop:runtime-conflict-smoke
npm run desktop:runtime-force-kill-smoke
npm run desktop:install-smoke
npm run desktop:audit
npm run desktop:source-audit
```

只开发 helper 时，请在 `helper/` 目录中运行命令，以应用该目录的链接器配置：

```powershell
rustup toolchain install 1.96.0-x86_64-pc-windows-gnullvm --profile minimal --component rustfmt
cd helper
rustup run 1.96.0-x86_64-pc-windows-gnullvm cargo fmt --check
rustup run 1.96.0-x86_64-pc-windows-gnullvm cargo test
```

构建过程会根据已安装的 npm 生产依赖和锁定的 Windows Cargo 依赖图生成 `THIRD-PARTY-NOTICES.txt`。安装包和便携版必须同时包含该文件与项目许可证；许可证文本缺失或无法审计时，构建会直接失败。

macOS 原生检查：

```bash
npm ci --prefix apps/desktop
npm --prefix apps/desktop run check
npm --prefix apps/desktop test
npm --prefix apps/desktop run build
cargo +stable fmt --manifest-path helper/Cargo.toml --check
cargo +stable test --manifest-path helper/Cargo.toml --target x86_64-apple-darwin
cargo +stable check --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-apple-darwin
```

Linux X11 检查（CI 使用 Xvfb；Wayland 不承诺完整等价）：

```bash
sudo apt-get install -y pkg-config libx11-dev libxtst-dev libxi-dev libxinerama-dev libxrandr-dev libxss-dev libwayland-dev libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev xvfb
Xvfb :99 -screen 0 1920x1080x24 &
export DISPLAY=:99
export XDG_SESSION_TYPE=x11
npm ci --prefix apps/desktop
npm --prefix apps/desktop run check
npm --prefix apps/desktop test
npm --prefix apps/desktop run build
cargo +stable fmt --manifest-path helper/Cargo.toml --check
cargo +stable test --manifest-path helper/Cargo.toml --target x86_64-unknown-linux-gnu
cargo +stable check --manifest-path apps/desktop/src-tauri/Cargo.toml --target x86_64-unknown-linux-gnu
```

更多技术资料：

- [架构说明](docs/architecture.md)
- [测试说明](docs/testing.md)
- [发布流程](docs/release.md)
- [安全策略](SECURITY.md)

## 发布完整性

公开版本采用不可变验收流程：从干净的 `main` 构建一次，发布安装包和便携版进行测试，再将完全相同的资产从 Pre-release 提升为 stable，不替换文件。每个 Release 都包含 SHA-256 校验和，以及绑定源码提交的产物清单。

## 许可证

本项目采用 [PolyForm Noncommercial License 1.0.0](LICENSE)，属于源码可用项目。

你可以在许可证条款下查看、学习、修改，并将软件用于个人和其他非商业用途。任何商业使用都需要事先取得版权所有者的书面许可。

该许可证从引入它的提交开始适用于本仓库。此前已经按 MIT License 发布的版本，继续适用其发布时授予的许可证。
