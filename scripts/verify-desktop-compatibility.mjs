import { spawn } from 'node:child_process';
import { access, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import {
  getNodePaths,
  readRuntimeLock,
  rootDirectory,
  runtimeStageDirectory,
} from './runtime-config.mjs';

function run(command, args, options) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { ...options, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.setEncoding('utf8');
    child.stderr.setEncoding('utf8');
    child.stdout.on('data', (chunk) => (stdout += chunk));
    child.stderr.on('data', (chunk) => (stderr += chunk));
    child.once('error', reject);
    child.once('close', (code) => {
      if (code === 0) resolve(stdout);
      else reject(new Error(`兼容补丁组合验证失败（${code}）：${stderr || stdout}`));
    });
  });
}

function rowOf(config, id) {
  const marker = `- id: ${id}\n`;
  const start = config.indexOf(marker);
  if (start < 0) throw new Error(`组合结果缺少 ${id}`);
  const nextRow = config.indexOf('\n- id: ', start + marker.length);
  const nextSection = config.indexOf('\n# ==', start + marker.length);
  const ends = [nextRow, nextSection].filter((value) => value >= 0);
  return config.slice(start, ends.length > 0 ? Math.min(...ends) : undefined);
}

function requireRow(config, id, expected) {
  const row = rowOf(config, id);
  if (!row.includes(expected)) {
    throw new Error(`${id} 未包含 ${JSON.stringify(expected)}：\n${row}`);
  }
}

const compatibilityPatch = join(
  rootDirectory,
  'src-tauri',
  'resources',
  'dsh-desktop',
  'cordis.patch.yml',
);
await access(compatibilityPatch);
const temporaryHome = await mkdtemp(join(tmpdir(), 'dsh-desktop-compat-'));
const conflictPatch = join(temporaryHome, 'mirage-conflict.patch.yml');
const conflict = `- id: fs-sandbox\n  disabled: true\n- id: bash-sandbox\n  disabled: true\n- id: pwsh-sandbox\n  disabled: true\n- insert:\n    - id: mirage\n      name: '@struktoai/mirage-dsh/service'\n    - id: mirage-fs\n      name: '@struktoai/mirage-dsh/fs'\n    - id: mirage-shell\n      name: '@struktoai/mirage-dsh/shell'\n`;
await writeFile(conflictPatch, conflict, 'utf8');
try {
  const runtimeLock = await readRuntimeLock();
  const node = getNodePaths(runtimeLock).sidecarBinary;
  const dshEntry = join(
    runtimeStageDirectory,
    'node_modules',
    '@deepseek-ai',
    'dsh',
    'lib',
    'bin.js',
  );
  const config = await run(
    node,
    [dshEntry, 'web', '--patch', conflictPatch, '--patch', compatibilityPatch, '--dump-config'],
    {
      cwd: rootDirectory,
      env: { ...process.env, DSH_HOME: temporaryHome },
    },
  );
  requireRow(config, 'fs-sandbox', 'disabled: false');
  requireRow(config, 'bash-sandbox', "process.platform === 'win32'");
  requireRow(config, 'pwsh-sandbox', "process.platform !== 'win32'");
  for (const id of ['mirage', 'mirage-fs', 'mirage-shell'])
    requireRow(config, id, 'disabled: true');
  process.stdout.write('Desktop compatibility patch: OK\n');
} finally {
  await rm(temporaryHome, { recursive: true, force: true });
}
