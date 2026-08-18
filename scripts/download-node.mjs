import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream } from 'node:fs';
import { copyFile, mkdir, rename, rm } from 'node:fs/promises';
import { pipeline } from 'node:stream/promises';
import { Readable } from 'node:stream';

import { getNodePaths } from './runtime-config.mjs';

/** 计算大文件 SHA-256，避免一次性加载 Node 二进制。 */
export async function calculateSha256(filePath) {
  const hash = createHash('sha256');
  await pipeline(createReadStream(filePath), hash);
  return hash.digest('hex');
}

async function downloadFile(url, targetPath) {
  const temporaryPath = `${targetPath}.download`;
  await rm(temporaryPath, { force: true });

  const response = await fetch(url);
  if (!response.ok || !response.body) {
    throw new Error(`下载失败：${response.status} ${response.statusText}`);
  }

  await pipeline(Readable.fromWeb(response.body), createWriteStream(temporaryPath));
  await rename(temporaryPath, targetPath);
}

/** 下载并校验官方 Node，随后复制为 Tauri 目标三元组文件名。 */
export async function stageNodeBinary(runtimeLock) {
  const paths = getNodePaths(runtimeLock);
  await mkdir(paths.cacheDirectory, { recursive: true });
  await mkdir(new URL('../src-tauri/binaries/', import.meta.url), { recursive: true });

  let currentHash;
  try {
    currentHash = await calculateSha256(paths.cacheBinary);
  } catch {
    // 首次执行或缓存损坏时统一进入官方下载流程。
    currentHash = '';
  }

  if (currentHash !== runtimeLock.node.sha256) {
    await downloadFile(runtimeLock.node.url, paths.cacheBinary);
    currentHash = await calculateSha256(paths.cacheBinary);
  }

  if (currentHash !== runtimeLock.node.sha256) {
    throw new Error('Node 二进制 SHA-256 校验失败');
  }

  await copyFile(paths.cacheBinary, paths.sidecarBinary);
  return paths;
}
