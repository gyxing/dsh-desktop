import { cp, mkdir, rm, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join, relative } from 'node:path';

import { stageNodeBinary } from './download-node.mjs';
import {
  readRuntimeLock,
  rootDirectory,
  runtimePackageName,
  runtimeStageDirectory,
  npmRegistry,
  pnpmCliPath,
} from './runtime-config.mjs';
import { runCommand } from './run-command.mjs';

async function downloadLicense(url, targetPath) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Node 许可证下载失败：${response.status}`);
  }

  await writeFile(targetPath, await response.text(), 'utf8');
}

/** 使用目标 Node 执行 pnpm，确保原生依赖针对 Sidecar ABI 安装。 */
async function stageRuntime() {
  const runtimeLock = await readRuntimeLock();
  const nodePaths = await stageNodeBinary(runtimeLock);
  const runtimeEnvironment = {
    ...process.env,
    PATH: `${nodePaths.cacheDirectory};${process.env.PATH ?? ''}`,
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
    ],
    { cwd: rootDirectory, env: runtimeEnvironment },
  );

  await downloadLicense(runtimeLock.node.licenseUrl, join(runtimeStageDirectory, 'NODE-LICENSE'));
  await cp(
    join(rootDirectory, 'runtime', 'runtime-lock.json'),
    join(runtimeStageDirectory, 'runtime-lock.json'),
  );
  await writeFile(
    join(runtimeStageDirectory, 'runtime-manifest.json'),
    `${JSON.stringify({ schemaVersion: 1, target: runtimeLock.target, pnpmVersion: runtimeLock.pnpmVersion, node: runtimeLock.node, dsh: runtimeLock.dsh }, null, 2)}\n`,
    'utf8',
  );

  await runCommand(process.execPath, [join(rootDirectory, 'scripts', 'verify-runtime.mjs')], {
    cwd: rootDirectory,
  });
}

await stageRuntime();
