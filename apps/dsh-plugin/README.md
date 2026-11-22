# 便捷窗口 · DSH 插件（Windows 11 x64 测试版）

在 DeepSeek Harness 左侧打开 **便捷窗口**，可以让窗口贴到屏幕边缘后自动收起，也可以用鼠标停留在屏幕角落来显示桌面。不需要模型、API Key、uTools 或独立桌面版。助手已经装在插件里；不需要另下 EXE，也不用 `npm install -g`。

### 怎么安装和试用

1. 至少打开一次 DSH 桌面版，然后从系统托盘彻底退出（只关窗口不一定是退出）。准备本版 `.tgz` 文件，不用解压。在它所在的文件夹打开 PowerShell，运行下方命令。`dsh` 指 DSH 自带的命令；不在 PATH 时，用安装目录下的 `resources/runtime/cli/bin/dsh.cmd` 代替。

   ```powershell
   $pkg = (Resolve-Path '.\dsh-plugin-convenient-window-0.1.0-beta.1.tgz').Path
   dsh plugin --profile desktop add "file:$pkg" --offline --ignore-scripts
   ```

   命令会先取安装包的绝对路径。安装到 `desktop`，不要选 `web`。此安装方式针对已安装并初始化的 DSH 桌面配置。
2. **重新打开 DSH 桌面版**，点左侧「便捷窗口」。首次使用 DSH 若见登录引导，可选「添加 API Key」→「稍后配置」，无需填写。
3. 在「窗口增强」打开「贴边隐藏」，勾选左边，点「保存设置」。在「触发角」选一个角、把「停留动作」设为「显示桌面」，再保存。回「运行中心」点 **启用助手**。角和边均默认不开启。
4. 用一个普通、未最大化的记事本窗口试：拖住标题栏把窗口拖出左边一小段并松手，移开鼠标让它收起，将鼠标移回露出的边条展开。把鼠标停在刚才选的角（默认 350 毫秒）试显示桌面。**切换页签不等于停用**；停止请点「运行中心 → 停止助手」。重启 DSH 后设置还在，但需要手动启用。

**共存：** 三种版本可以同时安装，但同一时刻只能有一个窗口助手运行。看到「另一个宿主或程序正在使用助手端口」就先在另一个版本里手动关掉助手，再回来启用。插件不会抢占、关闭对方或导入对方的配置。请用普通身份在本机打开 DSH，不提供管理员模式或远程控制。

**卸载：** 先停止助手并退出 DSH，运行 `dsh plugin --profile desktop remove dsh-plugin-convenient-window`。插件的设置默认保留，重装可继续使用。修复损坏设置时先停止助手并退出 DSH，备份当前 DSH 配置目录下 `convenient-window/settings.json`，将原文件改名后重开即可恢复默认。本插件不写入其他宿主的数据。

### 维护与发布

```text
DSH 侧栏 → Svelte 设置页 → DSH 的本机连接器 → 包内的正式 Rust 助手 → Windows
                                 └→ 当前 DSH 配置目录/convenient-window/
```

- `src/client.js` 是 DSH 插槽与 React 挂载桥；`src/Panel.svelte` 负责界面；`host/index.js` 负责鉴权、排队、状态及插件卸载；`host/settings.js` 只将允许的参数映射到既有 schema。主进程校验本地资产、启动自有子进程并核对进程、版本和配置回执；页签本身不持有助手。暂未移植手势、布局、提权和 uTools 专属动作。
- 产品 `0.6.11` / 内置助手 `0.6.11` / 插件包 `0.1.0-beta.1` / 本轮 DSH `0.2.2-rc.5`（精确范围）。仅 DSH 接口变化，优先修侧栏桥或连接器，升级插件包号并回归；不重发其他产品。产品升级时再核对 schema、protocol、资产哈希和公开源码提交。面板默认配置在构建时取自桌面侧源码，安装后不依赖桌面版。
- 在公开源码仓库的 `apps/dsh-plugin/` 目录：`npm ci` → `npm test`。取得 `runtime-assets.json` 指定的**原版发布资产**后，执行 `npm run stage:helper -- <解压目录>`，再在空闲的 Windows 11 x64 上运行 `npm run test:native`。原生门禁只在显式调用时启动本包助手，不关闭别的程序。最后 `npm pack` 生成单个预构建包；包不含开发依赖、安装钩子或远程下载步骤。`LICENSE` 来自仓库根目录；`payload/HELPER-LICENSE.txt` 原样随助手分发；`lib/THIRD-PARTY.txt` 包含实际打入前端的依赖许可。
- 桌面回归命令：`npm run test:desktop -- "<已安装的 DSH 可执行文件>" "<最终 .tgz 完整路径>"`。它只操作自己新建的测试配置，会打开测试窗口，保留本地测试现场，不移动全局鼠标。每个候选须用**最终 tarball**在全新桌面配置试装，验证两种语言、开/停、切页/最小化、重启、卸载、占用保护、真实贴边及角落动作。管理者确认后，可把**同一份字节**作为新产品 GitHub Release 的附件及 `npm publish <已验收的.tgz>` 的输入；不要重新打包或改写已有 stable。预览版在 npm 明确使用预览 dist-tag；回读两个下载并核对 SHA-256。
- **怎样让别人搜到：** 发布后，用户可按 `dsh-plugin-convenient-window` 或中英文关键词在 npm 上搜索。GitHub 项目的统一下载入口是 [产品 Releases](https://github.com/ximizhou/convenient_window_free/releases)。要在 DSH「发现」里搜到，还要让可信的 DSH 插件索引收录**已发布的精确版本**，用户也要启用相应索引；上传 npm 不会自动完成这一步。这里未发布到 npm、未新增 GitHub Release，也未申请索引收录。机制参考 [DSH 对应版本的市场文档](https://github.com/deepseek-ai/deepseek-harness/blob/dsh-v0.2.2-rc.5/packages/boot/plugin-market/README.zh.md)。
