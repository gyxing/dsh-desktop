import fs from 'node:fs';
import path from 'node:path';

const args = process.argv.slice(2).filter((argument) => argument !== '--');
const manifestPath = args.shift();
if (!manifestPath) {
  throw new Error(
    '用法：verify-updater-manifest.mjs <latest.json> --version <版本> --require <平台键>',
  );
}

let expectedVersion;
const requiredKeys = [];
const forbiddenPrefixes = [];
for (let index = 0; index < args.length; index += 1) {
  const option = args[index];
  const value = args[index + 1];
  if (!['--version', '--require', '--forbid-prefix'].includes(option) || !value) {
    throw new Error(`无效参数：${option ?? '<empty>'}`);
  }
  index += 1;
  if (option === '--version') expectedVersion = value;
  if (option === '--require') requiredKeys.push(value);
  if (option === '--forbid-prefix') forbiddenPrefixes.push(value);
}
if (!expectedVersion || requiredKeys.length === 0) {
  throw new Error('必须指定 --version 和至少一个 --require');
}

const resolvedPath = path.resolve(manifestPath);
const manifest = JSON.parse(fs.readFileSync(resolvedPath, 'utf8'));
if (manifest.version !== expectedVersion) {
  throw new Error(
    `更新清单版本不一致：期望 ${expectedVersion}，实际 ${manifest.version ?? '<missing>'}`,
  );
}
if (Number.isNaN(Date.parse(manifest.pub_date))) {
  throw new Error('更新清单缺少有效的 pub_date');
}
if (
  !manifest.platforms ||
  typeof manifest.platforms !== 'object' ||
  Array.isArray(manifest.platforms)
) {
  throw new Error('更新清单缺少 platforms 对象');
}

const platformKeys = Object.keys(manifest.platforms);
for (const key of requiredKeys) {
  if (!platformKeys.includes(key)) throw new Error(`更新清单缺少平台：${key}`);
}
for (const prefix of forbiddenPrefixes) {
  const forbidden = platformKeys.filter((key) => key.startsWith(prefix));
  if (forbidden.length > 0) throw new Error(`更新清单包含禁止公开的平台：${forbidden.join(', ')}`);
}
for (const [key, entry] of Object.entries(manifest.platforms)) {
  if (!entry || typeof entry !== 'object') throw new Error(`平台 ${key} 的内容无效`);
  if (typeof entry.signature !== 'string' || entry.signature.trim() === '') {
    throw new Error(`平台 ${key} 缺少签名`);
  }
  let url;
  try {
    url = new URL(entry.url);
  } catch {
    throw new Error(`平台 ${key} 的下载地址无效`);
  }
  if (url.protocol !== 'https:') throw new Error(`平台 ${key} 的下载地址必须使用 HTTPS`);
  if (url.hostname === 'api.github.com') {
    throw new Error(`平台 ${key} 不得使用受限流影响的GitHub API资产地址`);
  }
  if (url.hostname !== 'github.com' || !url.pathname.includes('/releases/download/')) {
    throw new Error(`平台 ${key} 必须使用GitHub Release公开下载直链`);
  }
}

console.log(`更新清单校验通过：${manifest.version}，平台 ${platformKeys.join(', ')}`);
