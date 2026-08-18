# DSH Desktop

将 [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) 原生 Web 界面封装为轻量桌面应用。当前实现面向 Windows x64，采用 Tauri 2 + 固定版本 Node Sidecar，后续可扩展其他平台。

## 已确定边界

- 桌面壳只负责启动、就绪检测、窗口跳转和进程树回收。
- Key、URL、模型、Provider、协议、Profile 和缓存全部沿用上游 DSH。
- 不设置或迁移 `DSH_HOME`，不建立第二套配置，也不把凭据写入系统凭据库。
- Sidecar 只监听随机 `127.0.0.1` 端口；远程页面不开放 Tauri IPC。
- Windows 安装器为全机 NSIS，POC 阶段不签名、不启用自动更新。

## 固定版本

- Node.js：24.19.0 x64，下载后校验 SHA-256。
- `@deepseek-ai/dsh`：0.1.0-rc.6，使用 pnpm 锁文件和 npm integrity。
- Tauri：2.11.x，各 Rust 与 npm 依赖使用精确版本。

真实锁定信息见 `runtime/runtime-lock.json`。打包运行时采用 hoisted 实体目录，避免 Windows 目录联接在 Tauri 复制资源时丢失。

## 本地开发

需要 Node.js、pnpm、Rust stable、Visual Studio 2022 Build Tools（C++）和 Windows SDK。

```powershell
corepack pnpm install --frozen-lockfile
corepack pnpm runtime:stage
corepack pnpm tauri dev
```

`runtime:stage` 会使用锁定的 Node 执行 pnpm，保证 `node-pty` 等原生模块 ABI 与发布 Sidecar 一致。

## 桌面行为

- 点击窗口右上角关闭按钮只会隐藏窗口，DSH 继续在后台运行。
- 单击系统托盘图标可重新显示并聚焦主窗口。
- 托盘菜单提供运行状态、DSH PowerShell、重新启动 Harness 和明确退出。
- 只有托盘“退出”才会回收桌面壳、Node 和完整 DSH 进程树。
- “打开 DSH PowerShell”优先使用外部 PowerShell 7；缺失时回退 Windows PowerShell 5.1。
- 专用 PowerShell 只在自己的进程内提供随包 `node`、`dsh`、`pnpm`，不会修改系统 PATH、PowerShell Profile 或 `DSH_HOME`。

启动页会依次展示进程启动、本机 HTTP 检查和页面加载状态。失败时可手动重新启动或复制当前进程内的脱敏诊断；诊断不会持久化或自动上传。

## 检查与构建

```powershell
corepack pnpm runtime:verify
corepack pnpm check
corepack pnpm tauri:build
```

NSIS 安装包输出在 `src-tauri/target/release/bundle/nsis/`。安装模式为 `perMachine`；未签名安装包会显示 Windows 信誉提示，仅适合 POC 验证。

WebView2 使用 `embedBootstrapper`：安装器内含微软 Evergreen 引导程序；目标机器缺少运行时时会联网静默下载安装，不捆绑 Fixed Runtime。

## 同步上游

```powershell
corepack pnpm runtime:check-upstream
```

检查只读比较 npm 最新版本、integrity 和 GitHub master SHA，不会自动替换运行时。检测到更新后：

1. 审查上游配置、协议、Provider 和原生依赖变化。
2. 更新 `runtime/package.json` 与 `runtime/runtime-lock.json` 的精确版本、integrity 和提交 SHA。
3. 更新 `pnpm-lock.yaml`，重新执行运行时暂存、完整检查和桌面冒烟验证。
4. 重新构建并发布安装包。

由于 DSH 和 Node 被嵌入安装包，上游更新后必须重新打包发布；当前 POC 不从网络热替换可执行代码。后续如启用 Tauri Updater，应采用签名的整包原子更新。

## POC 资源门槛

- NSIS 安装包不超过 180 MiB；安装目录不超过 500 MiB。
- 冷启动 P50 不超过 4 秒、P95 不超过 6 秒。
- 空闲进程树私有工作集不超过 300 MiB，CPU P95 不超过 1%；同时记录含共享页的总 Working Set。
- 30 分钟空闲私有工作集增长不超过 20 MiB。
- 正常退出后 3 秒内无桌面壳、Node 或 WebView2 残留进程。

`resources:measure` 为避免触发“关闭后隐藏到托盘”，测量完成后会强制结束桌面根进程并验证 Job Object 回收；托盘“退出”的正常路径仍需按人工验收清单确认。

上游 DSH 与依赖包的许可证保留在随包运行时中，Node 许可证位于 `NODE-LICENSE`。对外分发前还应完成代码签名、第三方许可证清单和品牌使用审核。
