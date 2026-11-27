<div align="center">

<img src="docs/images/app-icon.png" width="128" alt="Convenient Window icon">

# 便捷窗口 (Magic Corners)

**更顺手的 Windows 窗口管理与桌面交互体验。**

通过屏幕热区、全局手势、贴边隐藏和窗口布局，让你的鼠标操作效率翻倍。

[![Version](https://img.shields.io/github/v/release/ximizhou/convenient_window_free?label=stable&color=3f6ee4&style=flat-square)](https://github.com/ximizhou/convenient_window_free/releases/latest)
[![Platform](https://img.shields.io/badge/Platform-Windows%2011-3f6ee4?style=flat-square)](#platform-support)

</div>

---

## ✨ 功能演示

![功能演示](docs/images/demo.gif)

---

## ✨ 核心能力

### 🧠 智能热区（Hot Corners）
将屏幕的四角与四边变成你的“超级快捷键”。悬停、点击、滚轮或滑动，即可实现置顶、音量调节、亮度修改或执行系统命令。

### 🐭 高效鼠标手势
只需按住鼠标右键，简单一笔画出轨迹，即可轻松复制、粘贴、关闭窗口或进行精准 OCR（文字识别）。

### 🧊 优雅的贴边隐藏
不再需要费力手动隐藏窗口。将窗口拖拽至边缘，它们会自动收纳为纤细边条；鼠标靠近，窗口瞬间“浮现”。

### 🧩 空间管理大师
*   **一键分屏与布局记录**：一键保存当前桌面所有窗口的位置和大小，无论你是办公模式还是游戏模式，一键复位。
*   **任意位置拖拽**：按住 `Alt + 鼠标`，在窗口内任何地方即可随心移动或缩放，不用费力寻找边缘。
*   **置顶图钉**：为重要窗口钉上小图钉，保持它们永远处于最顶层。

### 🎨 桌面美化
深度自定义任务栏材质（透明、磨砂、纯色），提升你的桌面颜值。

---

## 📥 下载安装

适用于 **Windows 11 x64** 系统。

| 下载包 | 适用场景 |
| :--- | :--- |
| [安装包 (`setup.exe`)](https://github.com/ximizhou/convenient_window_free/releases/latest) | **推荐：** 标准安装，自动管理。 |
| 便携版 (`.zip`) | 解压即用，不写注册表，适合放在优盘或无需安装的场景。 |

---

## 📖 了解更多

*   [功能说明与使用文档](docs/README.md)
*   [版本更新与发布记录](docs/release-assets/README.md)

---

<details>
<summary><strong>开发者指南 (Build & Develop)</strong></summary>

此仓库包含 Tauri 2 桌面宿主前端与 Rust 后端助手。

### 构建与测试
```powershell
# 准备环境
npm ci

# 开发调试
npm run desktop:dev

# 测试与构建
npm run desktop:test
npm run desktop:build
```
详见 `CONTRIBUTING.md` 获取详细环境配置与工具链说明。
</details>

---

*本项目遵循 PolyForm Noncommercial 1.0.0 协议，仅限个人与非商业用途。*
