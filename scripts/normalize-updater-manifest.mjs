import fs from 'node:fs';
import path from 'node:path';

const args = process.argv.slice(2).filter((argument) => argument !== '--');
const manifestPath = args.shift();
const assetsPath = args.shift();
let repository;
let tag;
for (let index = 0; index < args.length; index += 2) {
  const option = args[index];
  const value = args[index + 1];
  if (!['--repository', '--tag'].includes(option) || !value) {
    throw new Error(`无效参数：${option ?? '<empty>'}`);
  }
  if (option === '--repository') repository = value;
  if (option === '--tag') tag = value;
}
if (!manifestPath || !assetsPath || !repository || !tag) {
  throw new Error(
    '用法：normalize-updater-manifest.mjs <latest.json> <release-assets.json> --repository <owner/repo> --tag <vX.Y.Z>',
  );
}
if (!/^[0-9A-Za-z_.-]+\/[0-9A-Za-z_.-]+$/.test(repository)) {
  throw new Error(`GitHub仓库格式无效：${repository}`);
}
if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
  throw new Error(`发布标签格式无效：${tag}`);
}

const resolvedManifestPath = path.resolve(manifestPath);
const manifest = JSON.parse(fs.readFileSync(resolvedManifestPath, 'utf8'));
const assets = JSON.parse(fs.readFileSync(path.resolve(assetsPath), 'utf8'));
if (!Array.isArray(assets)) throw new Error('Release资产数据必须为数组');
if (!manifest.platforms || typeof manifest.platforms !== 'object') {
  throw new Error('更新清单缺少platforms对象');
}

const assetsByApiUrl = new Map(assets.map((asset) => [asset.url, asset]));
const assetsByBrowserUrl = new Map(assets.map((asset) => [asset.browser_download_url, asset]));
const assetsByName = new Map(assets.map((asset) => [asset.name, asset]));

/** GitHub API资产地址需要认证和限流配额，公开更新必须改用Release下载直链。 */
for (const [platform, entry] of Object.entries(manifest.platforms)) {
  let currentName;
  try {
    const currentUrl = new URL(entry.url);
    currentName = decodeURIComponent(currentUrl.pathname.split('/').at(-1));
  } catch {
    throw new Error(`平台 ${platform} 的原下载地址无效：${entry.url}`);
  }
  const asset =
    assetsByApiUrl.get(entry.url) ??
    assetsByBrowserUrl.get(entry.url) ??
    assetsByName.get(currentName);
  if (!asset?.name) {
    throw new Error(`平台 ${platform} 无法匹配GitHub Release资产：${entry.url}`);
  }
  // Draft的browser_download_url包含临时untagged路径，必须主动构造发布后的正式标签URL。
  const assetName = encodeURIComponent(asset.name);
  const downloadUrl = new URL(
    `https://github.com/${repository}/releases/download/${encodeURIComponent(tag)}/${assetName}`,
  );
  entry.url = downloadUrl.toString();
}

fs.writeFileSync(resolvedManifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
console.log(`更新清单直链规范化完成：${Object.keys(manifest.platforms).join(', ')}`);
