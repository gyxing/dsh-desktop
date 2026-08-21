import assert from 'node:assert/strict';
import test from 'node:test';

import {
  buildAboutSections,
  createPublicVersionText,
  formatCopyright,
  formatBuildTime,
  type AboutDialogPayload,
} from './content.ts';

const PAYLOAD: AboutDialogPayload = {
  appName: 'DSH Desktop',
  version: '0.1.3',
  description: 'DeepSeek Harness 的第三方跨平台桌面封装',
  disclaimer: '非 DeepSeek 官方产品',
  buildTimestampMs: Date.UTC(2026, 7, 21, 13, 35),
  buildId: '02c7d56',
  platform: 'Windows x64',
  dshVersion: '0.1.1-rc.2',
  nodeVersion: '24.19.0',
  pnpmVersion: '11.22.0',
  website: 'https://github.com/gyxing/dsh-desktop',
  author: 'gyxing',
};

test('构建时间使用明确的本地偏移，避免日期含义不清', () => {
  assert.equal(formatBuildTime(PAYLOAD.buildTimestampMs, 480), '2026-08-21 21:35 UTC+8');
  assert.equal(formatBuildTime(PAYLOAD.buildTimestampMs, 330), '2026-08-21 19:05 UTC+5:30');
});

test('缺少有效构建时间时不显示 1970 年', () => {
  assert.equal(formatBuildTime(0, 480), '未知');
  assert.equal(formatCopyright(0, 'gyxing'), '© gyxing');
});

test('关于窗口分组展示公开的版本、平台和内置运行时信息', () => {
  assert.deepEqual(buildAboutSections(PAYLOAD, 480), [
    {
      title: '版本信息',
      entries: [
        { label: '应用版本', value: '0.1.3' },
        { label: '构建时间', value: '2026-08-21 21:35 UTC+8' },
        { label: '构建标识', value: '02c7d56' },
        { label: '运行平台', value: 'Windows x64' },
      ],
    },
    {
      title: '内置运行时',
      entries: [
        { label: 'DeepSeek Harness', value: '0.1.1-rc.2' },
        { label: 'Node.js', value: '24.19.0' },
        { label: 'pnpm', value: '11.22.0' },
      ],
    },
  ]);
});

test('复制文本包含排障所需公开信息且不包含设备路径', () => {
  assert.equal(
    createPublicVersionText(PAYLOAD, 480),
    [
      'DSH Desktop 0.1.3',
      '构建时间：2026-08-21 21:35 UTC+8',
      '构建标识：02c7d56',
      '运行平台：Windows x64',
      'DeepSeek Harness：0.1.1-rc.2',
      'Node.js：24.19.0',
      'pnpm：11.22.0',
      '项目主页：https://github.com/gyxing/dsh-desktop',
    ].join('\n'),
  );
});
