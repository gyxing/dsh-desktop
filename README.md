# DSH Desktop

[简体中文](README.md) | [English](README.en.md)

将 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 原生 Web 界面封装为轻量桌面应用。当前支持 Windows x64、macOS ARM64/x64 和 Linux x64，采用 Tauri 2 + 各平台固定版本 Node Sidecar。

## 已确定边界

- 桌面壳只负责启动、桌面运行兼容、就绪检测、窗口跳转和进程树回收。
- Key、URL、模型、Provider、协议、Profile 和缓存全部沿用上游 DSH。
- 不设置或迁移 `DSH_HOME`，不建立第二套配置，也不把凭据写入操作系统凭据库。
- Web Profile 启用 `@struktoai/mirage-dsh` 时，Desktop 仅为 Sidecar 追加进程内兼容覆盖，保留宿主原生文件系统和平台 Shell；不会改写 Profile，外部 `dsh web` 不受影响。
- Sidecar 只监听随机 `127.0.0.1` 端口；远程页面不开放 Tauri IPC。
- Windows 当前不使用 Authenticode，首次安装可能显示未知发布者或 SmartScreen 提示。
- macOS 当前未配置 Apple Developer 签名和公证，DMG 仅作为内部测试包。
- 在线更新使用 Tauri Updater 独立签名校验；公开更新清单支持 Windows NSIS 和 Linux AppImage，macOS 与 Linux deb 暂不自动更新。

## 固定版本

- Node.js：24.19.0；按 Windows x64、macOS ARM64/x64、Linux x64 分别锁定官方制品并校验 SHA-256。
- `@deepseek-ai/dsh`：0.1.0-rc.7，使用 pnpm 锁文件和 npm integrity。
- Tauri：2.11.x，各 Rust 与 npm 依赖使用精确版本。

真实锁定信息见 `runtime/runtime-lock.json`。每个平台必须在匹配架构的原生构建机上部署自己的依赖树和原生模块；Windows 继续使用 hoisted 实体目录，避免目录联接在 Tauri 复制资源时丢失。

## 本地开发

需要 Node.js、pnpm、Rust stable，以及目标平台的原生工具链：Windows 使用 Visual Studio 2022 Build Tools（C++）与 Windows SDK，macOS 使用 Xcode Command Line Tools，Linux 使用 WebKitGTK 4.1、AppIndicator 等 Tauri 系统依赖。

```powershell
corepack pnpm install --frozen-lockfile
corepack pnpm runtime:stage
corepack pnpm tauri dev
```

`runtime:stage` 只允许暂存与当前宿主平台匹配的目标，并使用锁定的 Node 执行 pnpm，保证 `node-pty` 等原生模块 ABI 与发布 Sidecar 一致。

## 桌面行为

- 点击窗口右上角关闭按钮只会隐藏窗口，DSH 继续在后台运行。
- 单击系统托盘图标可重新显示并聚焦主窗口。
- 原生窗口标题栏以 `DSH Desktop <版本号>` 显示当前桌面版本；加载 DSH 页面后仍保持可见。
- 托盘或菜单栏提供运行状态、更新状态、DSH 终端、重新启动 Harness、检查更新和明确退出。
- 只有托盘“退出”才会回收桌面壳、Node 和完整 DSH 进程树。
- “打开 DSH 终端”在 Windows 优先使用外部 PowerShell 7，并回退到 Windows PowerShell 5.1；macOS 使用 Terminal.app；Linux 依次检测常见桌面终端。
- 专用终端只在自己的进程内提供随包 `node`、`dsh`、`pnpm`，不会修改系统 PATH、Shell Profile 或 `DSH_HOME`。
- 应用启动约 30 秒后自动检查一次更新，托盘也可以随时手动检查；后台检查失败不会阻塞主窗口。
- 发现新版本后由用户确认是否下载；完整安装包验签成功后才停止 Sidecar，以被动模式安装并重启应用。
- 更新只替换桌面应用和随包运行时，不迁移、不重写现有 DSH Profile、缓存或凭据。

启动页会依次展示进程启动、本机 HTTP 检查和页面加载状态。失败时可手动重新启动或复制当前进程内的脱敏诊断；诊断不会持久化或自动上传。

## 检查与构建

```shell
corepack pnpm runtime:verify
corepack pnpm check

# Windows x64
corepack pnpm tauri build --target x86_64-pc-windows-msvc --bundles nsis --no-sign

# macOS（在对应 ARM64 或 Intel Mac 上执行）
corepack pnpm tauri build --target <macOS目标三元组> --bundles app,dmg --no-sign

# Linux x64
corepack pnpm tauri build --target x86_64-unknown-linux-gnu --bundles appimage,deb --no-sign
```

本地命令使用 `--no-sign`，只验证普通打包流程。正式发布由 GitHub Actions 在四个原生 Runner 上构建并汇总到同一个 Draft Release，同时生成 Tauri Updater 签名与 SHA-256 校验文件。托管 macOS Runner 会先构建 `.app`，再由 `scripts/package-macos-dmg.sh` 单步生成 DMG，避免临时镜像卸载故障。

Windows NSIS 使用 `perMachine` 全机安装；WebView2 使用 `embedBootstrapper`，缺少运行时时由微软 Evergreen 引导程序联网安装。Windows 不信任 Tauri `.sig`，该签名只用于应用内更新验签。

macOS 分别提供 ARM64 和 Intel DMG。未完成 Apple Developer 签名与公证前，这些包只用于内部测试，Gatekeeper 可能阻止直接启动。

Linux 提供 AppImage 和 deb。AppImage 可进入应用内更新清单；deb 由用户手动升级。Linux 兼容基线为 Ubuntu 22.04 / Debian 12。

## 同步上游

```powershell
corepack pnpm runtime:check-upstream
```

检查只读比较 npm 最新发布版本与 integrity；GitHub master SHA 仅作为源码演进观察点，不代表 npm 包的源码证明。命令不会自动替换运行时。检测到发布更新后：

1. 审查上游配置、协议、Provider 和原生依赖变化。
2. 更新 `runtime/package.json` 与 `runtime/runtime-lock.json` 的精确版本、integrity 和 master 观察点。
3. 更新 `pnpm-lock.yaml`，重新执行运行时暂存、完整检查和桌面冒烟验证。
4. 重新构建并发布安装包。

由于 DSH 和 Node 被嵌入安装包，上游更新后必须分别重新构建各平台安装包。GitHub Releases 只分发完整安装包，不从网络热替换单个可执行文件或依赖；公开更新清单当前只包含 Windows NSIS 和 Linux AppImage。

## POC 资源门槛

以下指标为当前 Windows x64 POC 的验收基线；macOS 与 Linux 需要在对应真机补充测量。

- NSIS 安装包不超过 180 MiB；安装目录不超过 500 MiB。
- 冷启动 P50 不超过 4 秒、P95 不超过 6 秒。
- 空闲进程树私有工作集不超过 300 MiB，CPU P95 不超过 1%；同时记录含共享页的总 Working Set。
- 30 分钟空闲私有工作集增长不超过 20 MiB。
- 正常退出后 3 秒内无桌面壳、Node 或 WebView2 残留进程。

`resources:measure` 为避免触发“关闭后隐藏到托盘”，测量完成后会强制结束桌面根进程并验证 Job Object 回收；托盘“退出”的正常路径仍需按人工验收清单确认。

上游 DSH 与依赖包的许可证保留在随包运行时中，Node 许可证位于 `NODE-LICENSE`。完整第三方许可证清单和品牌使用审核仍待后续完成；若要消除系统发布者提示，还需分别配置 Windows Authenticode 以及 Apple Developer 签名与公证。
