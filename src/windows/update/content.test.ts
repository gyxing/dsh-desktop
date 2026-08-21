import assert from 'node:assert/strict';
import test from 'node:test';

import { parseReleaseNotes } from './content.ts';

test('更新说明按标题、列表和正文分块且保留不可信文本', () => {
  const blocks = parseReleaseNotes(
    '## 本版更新\n\n- 支持断点续传\n- <img src=x onerror=alert(1)>\n\n安装前请保存工作。',
  );

  assert.deepEqual(blocks, [
    { kind: 'heading', text: '本版更新' },
    { kind: 'list', items: ['支持断点续传', '<img src=x onerror=alert(1)>'] },
    { kind: 'paragraph', text: '安装前请保存工作。' },
  ]);
});

test('空更新说明显示稳定兜底文案', () => {
  assert.deepEqual(parseReleaseNotes('   '), [
    { kind: 'paragraph', text: '此版本未提供更新说明。' },
  ]);
});
