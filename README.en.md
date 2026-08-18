# DSH Desktop

[简体中文](README.md) | [English](README.en.md)

DSH Desktop packages the native [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) web interface as a lightweight desktop application. The current implementation targets Windows x64 and uses Tauri 2 with a pinned Node.js sidecar, while leaving room for future platform support.

## Confirmed Boundaries

- The desktop shell only manages startup, readiness checks, window navigation, and process-tree cleanup.
- Keys, URLs, models, providers, protocols, profiles, and caches continue to use upstream DSH behavior.
- The application does not set or migrate `DSH_HOME`, create a second configuration system, or store credentials in the Windows credential vault.
- The sidecar listens only on a random `127.0.0.1` port, and the remote page has no access to Tauri IPC.
- Windows Authenticode signing is not currently configured, so first-time installation may show an unknown publisher or SmartScreen warning.
- Online updates use independent Tauri Updater signature verification and install only complete packages accepted by the public key embedded in the application.

## Pinned Versions

- Node.js: 24.19.0 x64, verified with SHA-256 after download.
- `@deepseek-ai/dsh`: 0.1.0-rc.6, locked with pnpm and npm integrity metadata.
- Tauri: 2.11.x, with exact Rust and npm dependency versions.

See `runtime/runtime-lock.json` for the authoritative lock metadata. The packaged runtime uses a hoisted physical directory so Windows directory junctions are not lost while Tauri copies resources.

## Local Development

Node.js, pnpm, Rust stable, Visual Studio 2022 Build Tools with C++, and the Windows SDK are required.

```powershell
corepack pnpm install --frozen-lockfile
corepack pnpm runtime:stage
corepack pnpm tauri dev
```

`runtime:stage` uses the pinned Node.js runtime to execute pnpm, ensuring that native modules such as `node-pty` match the ABI used by the packaged sidecar.

## Desktop Behavior

- Closing the main window hides it while DSH continues running in the background.
- Clicking the tray icon restores and focuses the main window.
- The native title bar displays `DSH Desktop <version>` and remains stable after navigation to the DSH page.
- The tray menu exposes runtime status, update status, DSH PowerShell, Harness restart, update checks, and explicit exit.
- Only the tray Exit command shuts down the desktop shell, Node.js, and the complete DSH process tree.
- DSH PowerShell prefers an external PowerShell 7 installation and falls back to Windows PowerShell 5.1 when necessary.
- The dedicated shell exposes the packaged `node`, `dsh`, and `pnpm` only inside its own process and does not modify the system PATH, PowerShell profile, or `DSH_HOME`.
- The application checks for updates once about 30 seconds after startup, and users can also check manually from the tray. Background failures do not interrupt the main window.
- When an update is available, the user chooses whether to download it. The sidecar is stopped only after the complete package passes signature verification, then the installer runs in passive mode and restarts the application.
- Updates replace only the desktop application and packaged runtime; existing DSH profiles, caches, and credentials are not migrated or rewritten.

The startup page reports process startup, local HTTP probing, and page-loading status. On failure, users can retry or copy redacted in-memory diagnostics; diagnostics are not persisted or uploaded automatically.

## Checks and Builds

```powershell
corepack pnpm runtime:verify
corepack pnpm check
corepack pnpm tauri build --bundles nsis --no-sign
```

The local NSIS command uses `--no-sign` to validate ordinary packaging. Official releases are built by GitHub Releases automation with the complete installer, update manifest, and checksum assets.

NSIS output is written to `src-tauri/target/release/bundle/nsis/` and uses per-machine installation. Windows does not trust the Tauri `.sig` file; it protects only in-app updates. Because Authenticode is not configured, first-time installation may still show an unknown publisher or SmartScreen warning.

WebView2 uses `embedBootstrapper`. The installer includes Microsoft's Evergreen bootstrapper, which silently downloads WebView2 when it is missing instead of bundling a Fixed Runtime.

## Syncing Upstream

```powershell
corepack pnpm runtime:check-upstream
```

This read-only command compares the latest npm version and integrity metadata with the GitHub master SHA. It never replaces the packaged runtime automatically. When an upstream update is detected:

1. Review upstream configuration, protocol, provider, and native dependency changes.
2. Update the exact version, integrity, and commit SHA in `runtime/package.json` and `runtime/runtime-lock.json`.
3. Update `pnpm-lock.yaml`, then repeat runtime staging, the full check, and desktop smoke testing.
4. Rebuild and publish the installer.

Because DSH and Node.js are embedded in the installer, upstream updates require a new desktop release. GitHub Releases distribute only complete installers accepted by Tauri Updater signature verification; individual executables or dependencies are never hot-swapped from the network.

## POC Resource Thresholds

- The NSIS installer must remain below 180 MiB, and the installed directory below 500 MiB.
- Cold-start P50 must remain below 4 seconds and P95 below 6 seconds.
- The idle process tree must remain below 300 MiB of private working set and 1% CPU at P95; total working set including shared pages is recorded separately.
- Private working-set growth over 30 idle minutes must remain below 20 MiB.
- Within 3 seconds of a normal exit, no desktop shell, Node.js, or WebView2 process may remain.

To avoid the normal close-to-tray behavior, `resources:measure` force-terminates the desktop root process after measurement and verifies Job Object cleanup. The tray Exit path still requires the manual acceptance checklist.

Upstream DSH and dependency licenses remain in the packaged runtime, and the Node.js license is stored in `NODE-LICENSE`. Public distribution still requires a complete third-party license inventory and brand-usage review. A trusted Authenticode certificate is required to remove Windows publisher warnings.
