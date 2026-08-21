# DSH Desktop

[简体中文](README.md) | [English](README.en.md)

DSH Desktop packages the native [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) web interface as a lightweight desktop application. It supports Windows x64, macOS ARM64/x64, and Linux x64 using Tauri 2 with a pinned Node.js sidecar for each platform.

## Confirmed Boundaries

- The desktop shell only manages startup, desktop-runtime compatibility, readiness checks, window navigation, and process-tree cleanup.
- Keys, URLs, models, providers, protocols, profiles, and caches continue to use upstream DSH behavior.
- The application does not set or migrate `DSH_HOME`, create a second configuration system, or store credentials in an operating-system credential vault.
- When the Web profile enables `@struktoai/mirage-dsh`, Desktop adds a process-local compatibility overlay that retains the host-native filesystem and platform shell. It does not rewrite the profile, and external `dsh web` runs are unaffected.
- The sidecar listens only on a random `127.0.0.1` port, and the remote page has no access to Tauri IPC.
- Windows Authenticode signing is not configured, so first-time installation may show an unknown publisher or SmartScreen warning.
- Apple Developer signing and notarization are not configured; macOS DMGs are internal test packages.
- Online updates use independent Tauri Updater signature verification. The public updater manifest supports Windows NSIS and Linux AppImage; macOS and Linux deb upgrades remain manual.

## Pinned Versions

- Node.js: 24.19.0, with official artifacts pinned and SHA-256 verified for Windows x64, macOS ARM64/x64, and Linux x64.
- `@deepseek-ai/dsh`: 0.1.1-rc.2, locked with pnpm and npm integrity metadata.
- Tauri: 2.11.x, with exact Rust and npm dependency versions.

See `runtime/runtime-lock.json` for the authoritative lock metadata. Each target is deployed on a matching native build host with its own dependency tree and native modules. Windows continues to use a hoisted physical directory so directory junctions are not lost while Tauri copies resources.

## Local Development

Node.js, pnpm, Rust stable, and the target platform toolchain are required: Visual Studio 2022 Build Tools with C++ and the Windows SDK on Windows, Xcode Command Line Tools on macOS, and the Tauri WebKitGTK 4.1/AppIndicator dependencies on Linux.

```powershell
corepack pnpm install --frozen-lockfile
corepack pnpm runtime:stage
corepack pnpm tauri dev
```

`runtime:stage` accepts only the target matching the current native host and uses the pinned Node.js runtime to execute pnpm, ensuring that native modules such as `node-pty` match the ABI used by the packaged sidecar.

## Desktop Behavior

- Closing the main window hides it while DSH continues running in the background.
- Clicking the tray icon restores and focuses the main window.
- Windows/Linux use a 36px single-row title bar that combines the app icon, `DSH Desktop <version>`, Application/Edit/Update/Help menus, update status, and three window controls. Its light and dark palettes follow the system theme, and it remains visible after navigation to DSH.
- macOS retains its native title bar and system menu bar. Menu and tray actions share one state and lifecycle on every platform.
- Help > About DSH Desktop shows the build time, build id, platform architecture, and bundled DSH/Node/pnpm versions, with a copyable public version summary.
- The tray or title-bar menu exposes runtime status, update status, the DSH terminal, Harness restart, update checks, and explicit exit.
- Only an explicit Exit command from the tray or title-bar menu shuts down the desktop shell, Node.js, and the complete DSH process tree.
- The DSH terminal prefers external PowerShell 7 and falls back to Windows PowerShell 5.1 on Windows, uses Terminal.app on macOS, and detects common desktop terminals on Linux.
- The dedicated terminal exposes the packaged `node`, `dsh`, and `pnpm` only inside its own process and does not modify the system PATH, shell profile, or `DSH_HOME`.
- The application checks about 30 seconds after startup and every 24 hours while it remains in the tray. Users can also check manually from the tray or native menu.
- A fixed-size window presents version-specific notes with a scrollable content region. The sidecar stops only after the complete package passes signature verification.
- Update packages remain in the application's private cache and resume from a validated HTTP Range offset after interruption, 45 seconds without progress, or an application restart.
- The title and tray show percentage, transferred size, average speed, ETA, and resume attempt. Redacted diagnostics can be copied from the Help menu after a failure.
- Updates replace only the desktop application and packaged runtime; existing DSH profiles, caches, and credentials are not migrated or rewritten.

The startup page reports process startup, local HTTP probing, and page-loading status. On failure, users can retry or copy redacted in-memory diagnostics; diagnostics are not persisted or uploaded automatically.

## Checks and Builds

```shell
corepack pnpm runtime:verify
corepack pnpm check

# Windows x64
corepack pnpm tauri build --target x86_64-pc-windows-msvc --bundles nsis --no-sign

# macOS (run on the matching ARM64 or Intel host)
corepack pnpm tauri build --target <macOS-target-triple> --bundles app,dmg --no-sign

# Linux x64
corepack pnpm tauri build --target x86_64-unknown-linux-gnu --bundles appimage,deb --no-sign
```

Local commands use `--no-sign` to validate ordinary packaging. Official releases are built on four native GitHub Actions runners and collected in one Draft Release with Tauri Updater signatures and SHA-256 checksum files. Hosted macOS runners build the `.app` first, then `scripts/package-macos-dmg.sh` creates the DMG in one pass to avoid temporary-image detach failures.

Windows NSIS installs per machine. WebView2 uses `embedBootstrapper`, so Microsoft's Evergreen bootstrapper downloads the runtime when it is missing. Tauri `.sig` files are used only for in-app update verification and are not Authenticode signatures.

Separate ARM64 and Intel DMGs are provided for macOS. Until Apple Developer signing and notarization are configured, these packages are for internal testing and Gatekeeper may block direct launch.

Linux provides AppImage and deb packages. AppImage can participate in in-app updates, while deb upgrades remain manual. The Linux compatibility baseline is Ubuntu 22.04 / Debian 12.

## Syncing Upstream

```powershell
corepack pnpm runtime:check-upstream
```

This read-only command treats the latest npm version and integrity metadata as the release signal. The GitHub master SHA is only a source-evolution observation point, not proof of the npm package source. The command never replaces the packaged runtime automatically. When a published update is detected:

1. Review upstream configuration, protocol, provider, and native dependency changes.
2. Update the exact version, integrity, and master observation point in `runtime/package.json` and `runtime/runtime-lock.json`.
3. Update `pnpm-lock.yaml`, then repeat runtime staging, the full check, and desktop smoke testing.
4. Rebuild and publish the installer.

Because DSH and Node.js are embedded in the application, upstream updates require native packages to be rebuilt for every platform. GitHub Releases distribute complete packages only; individual executables or dependencies are never hot-swapped. The public updater manifest currently includes Windows NSIS and Linux AppImage.

## POC Resource Thresholds

The following thresholds are the current Windows x64 POC baseline. macOS and Linux measurements still require their corresponding native machines.

- The NSIS installer must remain below 180 MiB, and the installed directory below 500 MiB.
- Cold-start P50 must remain below 4 seconds and P95 below 6 seconds.
- The idle process tree must remain below 300 MiB of private working set and 1% CPU at P95; total working set including shared pages is recorded separately.
- Private working-set growth over 30 idle minutes must remain below 20 MiB.
- Within 3 seconds of a normal exit, no desktop shell, Node.js, or WebView2 process may remain.

To avoid the normal close-to-tray behavior, `resources:measure` force-terminates the desktop root process after measurement and verifies Job Object cleanup. The tray Exit path still requires the manual acceptance checklist.

Upstream DSH and dependency licenses remain in the packaged runtime, and the Node.js license is stored in `NODE-LICENSE`. A complete third-party license inventory and brand-usage review remain future work. Removing operating-system publisher warnings also requires Windows Authenticode and Apple Developer signing/notarization.
