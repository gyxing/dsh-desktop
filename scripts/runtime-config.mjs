import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { resolveRuntimeTarget } from './platform-target.mjs';

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));

export const rootDirectory = dirname(scriptsDirectory);
export const runtimePackageDirectory = join(rootDirectory, 'runtime');
export const runtimeLockPath = join(runtimePackageDirectory, 'runtime-lock.json');
export const runtimeStageDirectory = join(rootDirectory, 'src-tauri', 'resources', 'dsh-runtime');
export const nodeBinariesDirectory = join(rootDirectory, 'src-tauri', 'binaries');
export const runtimeCacheDirectory = join(rootDirectory, '.cache', 'runtime');
export const pnpmCliPath = join(rootDirectory, 'node_modules', 'pnpm', 'bin', 'pnpm.cjs');
export const runtimePackageName = 'dsh-desktop-runtime';
export const npmRegistry = 'https://registry.npmjs.org';

/** 读取经过人工确认的运行时版本锁。 */
export async function readRuntimeLock() {
  const content = await readFile(runtimeLockPath, 'utf8');
  return JSON.parse(content);
}

/** 返回目标三元组对应的Node制品配置。 */
export function getRuntimeTarget(runtimeLock, target = resolveRuntimeTarget()) {
  if (runtimeLock.schemaVersion !== 2) {
    throw new Error(`不支持的运行时锁版本：${String(runtimeLock.schemaVersion)}`);
  }
  const runtimeTarget = runtimeLock.targets?.[target];
  if (!runtimeTarget?.node) {
    throw new Error(`运行时锁缺少目标：${target}`);
  }
  return runtimeTarget;
}

/** 返回目标Node缓存、解压目录和最终Sidecar位置。 */
export function getNodePaths(runtimeLock, target = resolveRuntimeTarget()) {
  const runtimeTarget = getRuntimeTarget(runtimeLock, target);
  const nodeSpec = {
    version: runtimeLock.node.version,
    licenseUrl: runtimeLock.node.licenseUrl,
    ...runtimeTarget.node,
  };
  if (!['binary', 'tar.gz'].includes(nodeSpec.format)) {
    throw new Error(`不支持的Node制品格式：${String(nodeSpec.format)}`);
  }

  const cacheDirectory = join(runtimeCacheDirectory, `node-v${nodeSpec.version}-${target}`);
  const extractedDirectory = join(cacheDirectory, 'runtime');
  const executableParts = String(nodeSpec.executable).split('/');
  const cacheArtifact =
    nodeSpec.format === 'binary'
      ? join(cacheDirectory, ...executableParts)
      : join(cacheDirectory, 'source.tar.gz');
  const cacheBinary =
    nodeSpec.format === 'binary' ? cacheArtifact : join(extractedDirectory, ...executableParts);
  const sidecarExtension = target.includes('windows') ? '.exe' : '';

  return {
    target,
    nodeSpec,
    cacheDirectory,
    cacheArtifact,
    extractedDirectory,
    cacheBinary,
    sidecarBinary: join(nodeBinariesDirectory, `node-${target}${sidecarExtension}`),
  };
}
