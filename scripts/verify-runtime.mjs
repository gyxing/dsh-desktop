import { access, readFile, realpath } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { join } from 'node:path';

import { calculateSha256 } from './download-node.mjs';
import { inspectRuntimeTree } from './file-verification.mjs';
import { assertNativeTarget, resolveRuntimeTarget } from './platform-target.mjs';
import {
  getNodePaths,
  getRuntimeTarget,
  readRuntimeLock,
  rootDirectory,
  runtimeStageDirectory,
} from './runtime-config.mjs';
import { runCommand } from './run-command.mjs';
import { verifyDshReleaseAgeExclusions } from './supply-chain-policy.mjs';

/** 校验当前目标打包输入完整、自包含，并能由目标Node加载CLI。 */
async function verifyRuntime() {
  const target = resolveRuntimeTarget();
  assertNativeTarget(target);
  const runtimeLock = await readRuntimeLock();
  const workspaceYaml = await readFile(join(rootDirectory, 'pnpm-workspace.yaml'), 'utf8');
  verifyDshReleaseAgeExclusions(workspaceYaml, runtimeLock.dsh.version);
  const runtimeTarget = getRuntimeTarget(runtimeLock, target);
  const nodePaths = getNodePaths(runtimeLock, target);
  const manifest = JSON.parse(
    await readFile(join(runtimeStageDirectory, 'runtime-manifest.json'), 'utf8'),
  );
  const dshDirectory = join(runtimeStageDirectory, 'node_modules', '@deepseek-ai', 'dsh');
  const dshEntry = join(dshDirectory, 'lib', 'bin.js');
  const pnpmDirectory = join(runtimeStageDirectory, 'node_modules', 'pnpm');
  const pnpmEntry = join(pnpmDirectory, 'bin', 'pnpm.cjs');
  const licensePath = join(runtimeStageDirectory, 'NODE-LICENSE');
  const requireFromDsh = createRequire(await realpath(dshEntry));
  const webAppEntry = requireFromDsh.resolve('@deepseek-ai/dsh-web-app');

  await access(dshEntry);
  await access(webAppEntry);
  await access(pnpmEntry);
  await access(licensePath);
  await access(nodePaths.cacheBinary);
  await access(nodePaths.sidecarBinary);

  if (manifest.schemaVersion !== 2 || manifest.target !== target) {
    throw new Error(`运行时清单目标不一致：${String(manifest.target)} != ${target}`);
  }
  if (manifest.node?.sourceSha256 !== runtimeTarget.node.sha256) {
    throw new Error('运行时清单的Node官方制品SHA-256与版本锁不一致');
  }
  const sourceHash = await calculateSha256(nodePaths.cacheArtifact);
  if (sourceHash !== runtimeTarget.node.sha256) {
    throw new Error('Node官方制品SHA-256与版本锁不一致');
  }
  const sidecarHash = await calculateSha256(nodePaths.sidecarBinary);
  if (sidecarHash !== manifest.node?.binarySha256) {
    throw new Error('Sidecar Node SHA-256与运行时清单不一致');
  }
  const licenseHash = await calculateSha256(licensePath);
  if (
    licenseHash !== runtimeLock.node.licenseSha256 ||
    licenseHash !== manifest.node?.licenseSha256
  ) {
    throw new Error('Node许可证SHA-256与版本锁或运行时清单不一致');
  }

  const packageMetadata = JSON.parse(await readFile(join(dshDirectory, 'package.json'), 'utf8'));
  if (
    packageMetadata.version !== runtimeLock.dsh.version ||
    manifest.dsh?.version !== runtimeLock.dsh.version
  ) {
    throw new Error(`DSH版本不一致：${String(packageMetadata.version)}`);
  }
  const pnpmMetadata = JSON.parse(await readFile(join(pnpmDirectory, 'package.json'), 'utf8'));
  if (
    pnpmMetadata.version !== runtimeLock.pnpmVersion ||
    manifest.pnpmVersion !== runtimeLock.pnpmVersion
  ) {
    throw new Error(`pnpm版本不一致：${String(pnpmMetadata.version)}`);
  }

  const inspection = await inspectRuntimeTree(runtimeStageDirectory);
  if (inspection.nativeModuleCount === 0) {
    throw new Error('运行时未发现原生模块，可能遗漏PTY或平台依赖');
  }

  const nodeVersionOutput = await runCommand(nodePaths.sidecarBinary, ['--version'], {
    cwd: rootDirectory,
    capture: true,
  });
  if (String(nodeVersionOutput).trim() !== `v${runtimeLock.node.version}`) {
    throw new Error(`Node版本不一致：${String(nodeVersionOutput)}`);
  }
  const versionOutput = await runCommand(nodePaths.sidecarBinary, [dshEntry, '--version'], {
    cwd: rootDirectory,
    capture: true,
  });
  if (!String(versionOutput).includes(runtimeLock.dsh.version)) {
    throw new Error(`DSH冒烟输出异常：${String(versionOutput)}`);
  }
  const pnpmVersionOutput = await runCommand(nodePaths.sidecarBinary, [pnpmEntry, '--version'], {
    cwd: rootDirectory,
    capture: true,
  });
  if (String(pnpmVersionOutput).trim() !== runtimeLock.pnpmVersion) {
    throw new Error(`pnpm冒烟输出异常：${String(pnpmVersionOutput)}`);
  }

  await runCommand(
    process.execPath,
    [join(rootDirectory, 'scripts', 'verify-desktop-compatibility.mjs')],
    {
      cwd: rootDirectory,
    },
  );

  const sizeMiB = (inspection.bytes / 1024 / 1024).toFixed(2);
  console.info(
    `运行时校验通过：目标 ${target}，Node ${runtimeLock.node.version}，DSH ${runtimeLock.dsh.version}，pnpm ${runtimeLock.pnpmVersion}，${sizeMiB} MiB，原生模块 ${inspection.nativeModuleCount} 个`,
  );
}

await verifyRuntime();
