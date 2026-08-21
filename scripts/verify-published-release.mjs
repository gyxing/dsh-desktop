import { pathToFileURL } from 'node:url';

const RANGE_BYTES = 1024 * 1024;
const REQUEST_TIMEOUT_MS = 30_000;

function requestOptions(extraHeaders = {}) {
  return {
    headers: {
      'Accept-Encoding': 'identity',
      'Cache-Control': 'no-cache',
      'User-Agent': 'dsh-desktop-release-smoke',
      ...extraHeaders,
    },
    redirect: 'follow',
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
  };
}

/** 从公开地址验证最终清单和Range下载能力，防止Draft通过但正式发布不可用。 */
export async function verifyPublishedManifest(
  manifestUrl,
  { expectedVersion, requiredPlatforms, fetchImplementation = fetch },
) {
  const manifestResponse = await fetchImplementation(manifestUrl, requestOptions());
  if (!manifestResponse.ok) {
    throw new Error(`公开更新清单请求失败：${manifestResponse.status}`);
  }
  const manifest = await manifestResponse.json();
  if (manifest.version !== expectedVersion) {
    throw new Error(`公开更新清单版本不一致：${String(manifest.version)}`);
  }

  const results = [];
  for (const platform of requiredPlatforms) {
    const entry = manifest.platforms?.[platform];
    if (!entry?.url || !entry?.signature) {
      throw new Error(`公开更新清单缺少平台或签名：${platform}`);
    }
    results.push(
      await verifyAsset(platform, entry.url, {
        fetchImplementation,
      }),
    );
  }
  return results;
}

async function verifyAsset(platform, url, { fetchImplementation }) {
  const head = await fetchImplementation(url, {
    ...requestOptions(),
    method: 'HEAD',
  });
  if (!head.ok) {
    throw new Error(`${platform} HEAD请求失败：${head.status}`);
  }
  const bytes = Number(head.headers.get('content-length'));
  if (!Number.isSafeInteger(bytes) || bytes <= 0) {
    throw new Error(`${platform} 缺少有效Content-Length`);
  }

  const rangeEnd = Math.min(bytes, RANGE_BYTES) - 1;
  const range = await fetchImplementation(url, {
    ...requestOptions({ Range: `bytes=0-${rangeEnd}` }),
  });
  if (range.status !== 206) {
    throw new Error(`${platform} Range请求必须返回206，实际${range.status}`);
  }
  const expectedContentRange = `bytes 0-${rangeEnd}/${bytes}`;
  if (range.headers.get('content-range') !== expectedContentRange) {
    throw new Error(
      `${platform} Content-Range不一致：${range.headers.get('content-range') ?? '<missing>'}`,
    );
  }
  const chunk = await range.arrayBuffer();
  if (chunk.byteLength !== rangeEnd + 1) {
    throw new Error(`${platform} Range正文长度不一致：${chunk.byteLength}`);
  }
  return { platform, bytes, finalUrl: head.url };
}

function parseArguments(argv) {
  const values = { requiredPlatforms: [] };
  for (let index = 0; index < argv.length; index += 2) {
    const option = argv[index];
    const value = argv[index + 1];
    if (!value) throw new Error(`参数缺少值：${option ?? '<empty>'}`);
    if (option === '--manifest') values.manifestUrl = value;
    else if (option === '--version') values.expectedVersion = value;
    else if (option === '--require') values.requiredPlatforms.push(value);
    else throw new Error(`无效参数：${option}`);
  }
  if (!values.manifestUrl || !values.expectedVersion || values.requiredPlatforms.length === 0) {
    throw new Error('必须指定 --manifest、--version 和至少一个 --require');
  }
  return values;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const options = parseArguments(process.argv.slice(2).filter((argument) => argument !== '--'));
  const results = await verifyPublishedManifest(options.manifestUrl, options);
  for (const result of results) {
    console.info(`公开更新资产可用：${result.platform}，${result.bytes}字节，${result.finalUrl}`);
  }
}
