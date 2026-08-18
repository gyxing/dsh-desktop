import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptsDirectory = dirname(fileURLToPath(import.meta.url));

export const rootDirectory = dirname(scriptsDirectory);
export const runtimePackageDirectory = join(rootDirectory, 'runtime');
export const runtimeLockPath = join(runtimePackageDirectory, 'runtime-lock.json');
export const runtimeStageDirectory = join(rootDirectory, 'src-tauri', 'resources', 'dsh-runtime');
export const nodeBinariesDirectory = join(rootDirectory, 'src-tauri', 'binaries');
export const pnpmCliPath = join(rootDirectory, 'node_modules', 'pnpm', 'bin', 'pnpm.cjs');
export const runtimePackageName = 'dsh-desktop-runtime';
export const npmRegistry = 'https://registry.npmjs.org';

/** 读取经过人工确认的运行时版本锁。 */
export async function readRuntimeLock() {
  const content = await readFile(runtimeLockPath, 'utf8');
  return JSON.parse(content);
}

/** 返回用于依赖安装的标准 Node 文件和最终 Sidecar 文件位置。 */
export function getNodePaths(runtimeLock) {
  const cacheDirectory = join(
    rootDirectory,
    '.cache',
    'runtime',
    `node-v${runtimeLock.node.version}-win-x64`,
  );

  return {
    cacheDirectory,
    cacheBinary: join(cacheDirectory, 'node.exe'),
    sidecarBinary: join(nodeBinariesDirectory, `node-${runtimeLock.target}.exe`),
  };
}
