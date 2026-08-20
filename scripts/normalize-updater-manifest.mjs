import fs from 'node:fs';
import path from 'node:path';

const [manifestPath, assetsPath] = process.argv.slice(2).filter((argument) => argument !== '--');
if (!manifestPath || !assetsPath) {
  throw new Error('用法：normalize-updater-manifest.mjs <latest.json> <release-assets.json>');
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

/** GitHub API资产地址需要认证和限流配额，公开更新必须改用Release下载直链。 */
for (const [platform, entry] of Object.entries(manifest.platforms)) {
  const asset = assetsByApiUrl.get(entry.url) ?? assetsByBrowserUrl.get(entry.url);
  if (!asset?.browser_download_url) {
    throw new Error(`平台 ${platform} 无法匹配GitHub Release资产：${entry.url}`);
  }
  const downloadUrl = new URL(asset.browser_download_url);
  if (
    downloadUrl.protocol !== 'https:' ||
    downloadUrl.hostname !== 'github.com' ||
    !downloadUrl.pathname.includes('/releases/download/')
  ) {
    throw new Error(`平台 ${platform} 的公开下载地址无效：${downloadUrl}`);
  }
  entry.url = downloadUrl.toString();
}

fs.writeFileSync(resolvedManifestPath, `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
console.log(`更新清单直链规范化完成：${Object.keys(manifest.platforms).join(', ')}`);
