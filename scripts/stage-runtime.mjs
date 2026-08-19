import { cp, copyFile, mkdir, readdir, rename, rm, writeFile } from 'node:fs/promises';
import { setTimeout as delay } from 'node:timers/promises';
import { delimiter, dirname, isAbsolute, join, relative } from 'node:path';

import { calculateSha256, stageNodeBinary } from './download-node.mjs';
import { assertNativeTarget, resolveRuntimeTarget } from './platform-target.mjs';
import {
  readRuntimeLock,
  rootDirectory,
  runtimeCacheDirectory,
  runtimePackageName,
  runtimeStageDirectory,
  npmRegistry,
  pnpmCliPath,
} from './runtime-config.mjs';
import { runCommand } from './run-command.mjs';

async function hashOrEmpty(filePath) {
  try {
    return await calculateSha256(filePath);
  } catch {
    return '';
  }
}

/** 缓存并校验既有Node许可证，避免每次暂存都依赖外部网络。 */
async function prepareNodeLicense(nodeLock) {
  const cachePath = join(runtimeCacheDirectory, `node-v${nodeLock.version}-LICENSE`);
  await mkdir(runtimeCacheDirectory, { recursive: true });
  if ((await hashOrEmpty(cachePath)) === nodeLock.licenseSha256) {
    return cachePath;
  }

  const reusableCandidates = [
    join(runtimeStageDirectory, 'NODE-LICENSE'),
    join(
      rootDirectory,
      'src-tauri',
      'target',
      'release',
      'resources',
      'dsh-runtime',
      'NODE-LICENSE',
    ),
    join(rootDirectory, 'src-tauri', 'target', 'debug', 'resources', 'dsh-runtime', 'NODE-LICENSE'),
  ];
  for (const candidate of reusableCandidates) {
    if ((await hashOrEmpty(candidate)) === nodeLock.licenseSha256) {
      await copyFile(candidate, cachePath);
      return cachePath;
    }
  }

  const temporaryPath = `${cachePath}.download`;
  let lastError;
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      await rm(temporaryPath, { force: true });
      const response = await fetch(nodeLock.licenseUrl);
      if (!response.ok) {
        throw new Error(`Node许可证下载失败：${response.status}`);
      }
      await writeFile(temporaryPath, await response.text(), 'utf8');
      const hash = await calculateSha256(temporaryPath);
      if (hash !== nodeLock.licenseSha256) {
        throw new Error('Node许可证SHA-256与版本锁不一致');
      }
      await rm(cachePath, { force: true });
      await rename(temporaryPath, cachePath);
      return cachePath;
    } catch (error) {
      lastError = error;
      await rm(temporaryPath, { force: true });
      if (attempt < 3) {
        await delay(attempt * 1_000);
      }
    }
  }
  throw lastError;
}

const nodePtyPrebuildByTarget = new Map([
  ['x86_64-pc-windows-msvc', 'win32-x64'],
  ['aarch64-apple-darwin', 'darwin-arm64'],
  ['x86_64-apple-darwin', 'darwin-x64'],
  ['x86_64-unknown-linux-gnu', 'linux-x64'],
]);

/** 删除依赖包附带的非目标原生制品，避免打包器扫描错误架构或ABI。 */
async function pruneUnusedNativeArtifacts(target) {
  const nodePtyPrebuildRoot = join(runtimeStageDirectory, 'node_modules', 'node-pty', 'prebuilds');
  const expectedNodePtyPrebuild = nodePtyPrebuildByTarget.get(target);
  if (!expectedNodePtyPrebuild) {
    throw new Error(`缺少node-pty目标映射：${target}`);
  }
  const nodePtyEntries = await readdir(nodePtyPrebuildRoot, { withFileTypes: true });
  if (
    !nodePtyEntries.some((entry) => entry.isDirectory() && entry.name === expectedNodePtyPrebuild)
  ) {
    throw new Error(`node-pty缺少目标预构建：${expectedNodePtyPrebuild}`);
  }
  await Promise.all(
    nodePtyEntries
      .filter((entry) => entry.isDirectory() && entry.name !== expectedNodePtyPrebuild)
      .map((entry) => rm(join(nodePtyPrebuildRoot, entry.name), { recursive: true, force: true })),
  );

  if (target === 'x86_64-unknown-linux-gnu') {
    // Koffi 的Linux x64包同时携带glibc与musl制品，本目标只保留glibc版本。
    await rm(
      join(runtimeStageDirectory, 'node_modules', '@koromix', 'koffi-linux-x64', 'musl_x64'),
      { recursive: true, force: true },
    );
  }
}

/** 使用目标Node执行pnpm，确保原生依赖针对Sidecar平台和ABI安装。 */
async function stageRuntime() {
  const target = resolveRuntimeTarget();
  assertNativeTarget(target);
  const runtimeLock = await readRuntimeLock();
  const nodePaths = await stageNodeBinary(runtimeLock, target);
  const licenseCachePath = await prepareNodeLicense(runtimeLock.node);
  const runtimeEnvironment = {
    ...process.env,
    PATH: `${dirname(nodePaths.cacheBinary)}${delimiter}${process.env.PATH ?? ''}`,
    npm_config_registry: npmRegistry,
  };

  await runCommand(
    nodePaths.cacheBinary,
    [pnpmCliPath, '--filter', runtimePackageName, 'install', '--frozen-lockfile'],
    { cwd: rootDirectory, env: runtimeEnvironment },
  );

  const stageRelativePath = relative(rootDirectory, runtimeStageDirectory);
  if (stageRelativePath.startsWith('..') || isAbsolute(stageRelativePath)) {
    throw new Error('运行时暂存目录超出工作区，已拒绝清理');
  }
  await rm(runtimeStageDirectory, { recursive: true, force: true });
  await mkdir(dirname(runtimeStageDirectory), { recursive: true });
  await runCommand(
    nodePaths.cacheBinary,
    [
      pnpmCliPath,
      '--filter',
      runtimePackageName,
      'deploy',
      runtimeStageDirectory,
      '--prod',
      '--config.node-linker=hoisted',
      '--config.package-import-method=copy',
      // POSIX 默认使用符号链接生成 .bin；改为实体命令 shim 以保持打包运行时无链接。
      '--config.prefer-symlinked-executables=false',
    ],
    { cwd: rootDirectory, env: runtimeEnvironment },
  );
  await pruneUnusedNativeArtifacts(target);

  await copyFile(licenseCachePath, join(runtimeStageDirectory, 'NODE-LICENSE'));
  await cp(
    join(rootDirectory, 'runtime', 'runtime-lock.json'),
    join(runtimeStageDirectory, 'runtime-lock.json'),
  );
  const manifest = {
    schemaVersion: 2,
    target,
    pnpmVersion: runtimeLock.pnpmVersion,
    node: {
      version: nodePaths.nodeSpec.version,
      url: nodePaths.nodeSpec.url,
      format: nodePaths.nodeSpec.format,
      sourceSha256: nodePaths.sourceSha256,
      binarySha256: nodePaths.binarySha256,
      licenseUrl: nodePaths.nodeSpec.licenseUrl,
      licenseSha256: runtimeLock.node.licenseSha256,
    },
    dsh: runtimeLock.dsh,
  };
  await writeFile(
    join(runtimeStageDirectory, 'runtime-manifest.json'),
    `${JSON.stringify(manifest, null, 2)}\n`,
    'utf8',
  );

  await runCommand(
    process.execPath,
    [join(rootDirectory, 'scripts', 'verify-runtime.mjs'), '--target', target],
    { cwd: rootDirectory },
  );
}

await stageRuntime();
