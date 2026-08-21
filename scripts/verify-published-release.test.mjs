import assert from 'node:assert/strict';
import { Buffer } from 'node:buffer';
import { createServer } from 'node:http';
import test from 'node:test';

import { verifyPublishedManifest } from './verify-published-release.mjs';

async function startServer({ ignoreRange = false } = {}) {
  const asset = Buffer.alloc(2 * 1024 * 1024, 7);
  const server = createServer((request, response) => {
    const origin = `http://127.0.0.1:${server.address().port}`;
    if (request.url === '/latest.json') {
      response.setHeader('Content-Type', 'application/json');
      response.end(
        JSON.stringify({
          version: '0.1.3',
          platforms: {
            'windows-x86_64-nsis': {
              signature: 'signed',
              url: `${origin}/asset`,
            },
          },
        }),
      );
      return;
    }
    if (request.url === '/asset' && request.method === 'HEAD') {
      response.statusCode = 200;
      response.setHeader('Content-Length', asset.length);
      response.setHeader('Accept-Ranges', 'bytes');
      response.end();
      return;
    }
    if (request.url === '/asset') {
      if (ignoreRange) {
        response.statusCode = 200;
        response.setHeader('Content-Length', asset.length);
        response.end(asset);
        return;
      }
      const chunk = asset.subarray(0, 1024 * 1024);
      response.statusCode = 206;
      response.setHeader('Content-Length', chunk.length);
      response.setHeader('Content-Range', `bytes 0-${chunk.length - 1}/${asset.length}`);
      response.end(chunk);
      return;
    }
    response.statusCode = 404;
    response.end();
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return {
    manifestUrl: `http://127.0.0.1:${server.address().port}/latest.json`,
    close: () =>
      new Promise((resolve, reject) =>
        server.close((error) => (error ? reject(error) : resolve())),
      ),
  };
}

test('发布后烟测验证公开清单、HEAD和Range响应', async () => {
  const fixture = await startServer();
  try {
    const result = await verifyPublishedManifest(fixture.manifestUrl, {
      expectedVersion: '0.1.3',
      requiredPlatforms: ['windows-x86_64-nsis'],
    });
    assert.equal(result.length, 1);
    assert.equal(result[0].bytes, 2 * 1024 * 1024);
  } finally {
    await fixture.close();
  }
});

test('发布后烟测拒绝忽略Range的资产服务', async () => {
  const fixture = await startServer({ ignoreRange: true });
  try {
    await assert.rejects(
      verifyPublishedManifest(fixture.manifestUrl, {
        expectedVersion: '0.1.3',
        requiredPlatforms: ['windows-x86_64-nsis'],
      }),
      /Range请求必须返回206/u,
    );
  } finally {
    await fixture.close();
  }
});
