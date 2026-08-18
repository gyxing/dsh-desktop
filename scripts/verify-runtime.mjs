import { access, readFile, realpath } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { join } from 'node:path';

import { calculateSha256 } from './download-node.mjs';
import { inspectRuntimeTree } from './file-verification.mjs';
import {
  getNodePaths,
  readRuntimeLock,
  rootDirectory,
  runtimeStageDirectory,
} from './runtime-config.mjs';
import { runCommand } from './run-command.mjs';

/** 校验打包输入完整、自包含，并能由目标 Node 正常加载 CLI。 */
async function verifyRuntime() {
  const runtimeLock = await readRuntimeLock();
  const nodePaths = getNodePaths(runtimeLock);
  const dshDirectory = join(runtimeStageDirectory, 'node_modules', '@deepseek-ai', 'dsh');
  const dshEntry = join(dshDirectory, 'lib', 'bin.js');
  const pnpmDirectory = join(runtimeStageDirectory, 'node_modules', 'pnpm');
  const pnpmEntry = join(pnpmDirectory, 'bin', 'pnpm.cjs');
  // 按真实入口创建 require，兼容 pnpm 在不同版本中的部署布局。
  const requireFromDsh = createRequire(await realpath(dshEntry));
  const webAppEntry = requireFromDsh.resolve('@deepseek-ai/dsh-web-app');

  await access(dshEntry);
  await access(webAppEntry);
  await access(pnpmEntry);

  const sidecarHash = await calculateSha256(nodePaths.sidecarBinary);
  if (sidecarHash !== runtimeLock.node.sha256) {
    throw new Error('Sidecar Node 校验值与版本锁不一致');
  }

  const packageMetadata = JSON.parse(await readFile(join(dshDirectory, 'package.json'), 'utf8'));
  if (packageMetadata.version !== runtimeLock.dsh.version) {
    throw new Error(`DSH 版本不一致：${String(packageMetadata.version)}`);
  }
  const pnpmMetadata = JSON.parse(await readFile(join(pnpmDirectory, 'package.json'), 'utf8'));
  if (pnpmMetadata.version !== runtimeLock.pnpmVersion) {
    throw new Error(`pnpm 版本不一致：${String(pnpmMetadata.version)}`);
  }

  const inspection = await inspectRuntimeTree(runtimeStageDirectory);
  if (inspection.nativeModuleCount === 0) {
    throw new Error('运行时未发现原生模块，可能遗漏 PTY 或平台依赖');
  }

  const versionOutput = await runCommand(nodePaths.sidecarBinary, [dshEntry, '--version'], {
    cwd: rootDirectory,
    capture: true,
  });
  if (!String(versionOutput).includes(runtimeLock.dsh.version)) {
    throw new Error(`DSH 冒烟输出异常：${String(versionOutput)}`);
  }
  const pnpmVersionOutput = await runCommand(nodePaths.sidecarBinary, [pnpmEntry, '--version'], {
    cwd: rootDirectory,
    capture: true,
  });
  if (String(pnpmVersionOutput).trim() !== runtimeLock.pnpmVersion) {
    throw new Error(`pnpm 冒烟输出异常：${String(pnpmVersionOutput)}`);
  }

  const sizeMiB = (inspection.bytes / 1024 / 1024).toFixed(2);
  console.info(
    `运行时校验通过：DSH ${runtimeLock.dsh.version}，pnpm ${runtimeLock.pnpmVersion}，${sizeMiB} MiB，原生模块 ${inspection.nativeModuleCount} 个`,
  );
}

await verifyRuntime();
