import { createHash } from 'node:crypto';
import { createReadStream, createWriteStream } from 'node:fs';
import { access, chmod, copyFile, mkdir, rename, rm } from 'node:fs/promises';
import { pipeline } from 'node:stream/promises';
import { Readable } from 'node:stream';

import { getNodePaths } from './runtime-config.mjs';
import { runCommand } from './run-command.mjs';

/** 计算大文件SHA-256，避免一次性加载Node制品。 */
export async function calculateSha256(filePath) {
  const hash = createHash('sha256');
  await pipeline(createReadStream(filePath), hash);
  return hash.digest('hex');
}

async function hashOrEmpty(filePath) {
  try {
    return await calculateSha256(filePath);
  } catch {
    return '';
  }
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

async function extractTarGz(paths) {
  const temporaryDirectory = `${paths.extractedDirectory}.extracting`;
  await rm(temporaryDirectory, { recursive: true, force: true });
  await mkdir(temporaryDirectory, { recursive: true });
  try {
    await runCommand(
      'tar',
      ['-xzf', paths.cacheArtifact, '-C', temporaryDirectory, '--strip-components=1'],
      { cwd: paths.cacheDirectory },
    );
    const temporaryBinary = paths.cacheBinary.replace(paths.extractedDirectory, temporaryDirectory);
    await access(temporaryBinary);
    await rm(paths.extractedDirectory, { recursive: true, force: true });
    await rename(temporaryDirectory, paths.extractedDirectory);
  } catch (error) {
    await rm(temporaryDirectory, { recursive: true, force: true });
    throw error;
  }
}

/** 下载并校验目标Node，随后复制为Tauri目标三元组文件名。 */
export async function stageNodeBinary(runtimeLock, target) {
  const paths = getNodePaths(runtimeLock, target);
  await mkdir(paths.cacheDirectory, { recursive: true });
  await mkdir(new URL('../src-tauri/binaries/', import.meta.url), { recursive: true });

  let sourceHash = await hashOrEmpty(paths.cacheArtifact);
  let sourceChanged = false;
  if (sourceHash !== paths.nodeSpec.sha256 && paths.nodeSpec.format === 'binary') {
    const existingSidecarHash = await hashOrEmpty(paths.sidecarBinary);
    if (existingSidecarHash === paths.nodeSpec.sha256) {
      await copyFile(paths.sidecarBinary, paths.cacheArtifact);
      sourceHash = existingSidecarHash;
    }
  }

  if (sourceHash !== paths.nodeSpec.sha256) {
    await downloadFile(paths.nodeSpec.url, paths.cacheArtifact);
    sourceHash = await calculateSha256(paths.cacheArtifact);
    sourceChanged = true;
  }
  if (sourceHash !== paths.nodeSpec.sha256) {
    throw new Error(`Node官方制品SHA-256校验失败：${paths.target}`);
  }

  if (paths.nodeSpec.format === 'tar.gz') {
    const binaryMissing = (await hashOrEmpty(paths.cacheBinary)) === '';
    if (sourceChanged || binaryMissing) {
      await extractTarGz(paths);
    }
    await chmod(paths.cacheBinary, 0o755);
  }

  await copyFile(paths.cacheBinary, paths.sidecarBinary);
  if (paths.nodeSpec.format !== 'binary') {
    await chmod(paths.sidecarBinary, 0o755);
  }
  const binarySha256 = await calculateSha256(paths.sidecarBinary);
  return { ...paths, sourceSha256: sourceHash, binarySha256 };
}
