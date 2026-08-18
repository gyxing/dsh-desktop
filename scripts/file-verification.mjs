import { lstat, opendir, stat } from 'node:fs/promises';
import { extname, resolve } from 'node:path';

/** 验证部署结果全部实体化，避免 Tauri 复制资源时丢失目录联接。 */
export async function inspectRuntimeTree(rootDirectory) {
  let bytes = 0;
  let nativeModuleCount = 0;
  const root = resolve(rootDirectory);

  async function visit(directory) {
    const entries = await opendir(directory);
    for await (const entry of entries) {
      const entryPath = resolve(directory, entry.name);
      const entryStat = await lstat(entryPath);

      if (entryStat.isSymbolicLink()) {
        throw new Error(`运行时包含未实体化链接：${entryPath}`);
      } else if (entryStat.isDirectory()) {
        await visit(entryPath);
      } else if (entryStat.isFile()) {
        bytes += (await stat(entryPath)).size;
        if (extname(entryPath) === '.node') {
          nativeModuleCount += 1;
        }
      }
    }
  }

  await visit(root);
  return { bytes, nativeModuleCount };
}
