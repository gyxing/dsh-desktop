import assert from 'node:assert/strict';
import test from 'node:test';

import { verifyDshReleaseAgeExclusions } from './supply-chain-policy.mjs';

const CURRENT_VERSION = '0.1.5-rc.2';

test('供应链例外只接受当前固定的 DSH 家族版本', () => {
  const result = verifyDshReleaseAgeExclusions(
    `packages:\n  - runtime\nminimumReleaseAgeExclude:\n  - pnpm@11.22.0\n  - '@deepseek-ai/dsh-base@0.1.5-rc.2'\n  - '@deepseek-ai/dsh@0.1.5-rc.2'\n`,
    CURRENT_VERSION,
  );

  assert.deepEqual(result, { dshExclusionCount: 2 });
});

test('供应链例外拒绝残留的旧 DSH 版本', () => {
  assert.throws(
    () =>
      verifyDshReleaseAgeExclusions(
        `minimumReleaseAgeExclude:\n  - '@deepseek-ai/dsh-base@0.1.2-rc.1'\n  - '@deepseek-ai/dsh@0.1.5-rc.2'\n`,
        CURRENT_VERSION,
      ),
    /0\.1\.2-rc\.1/u,
  );
});

test('供应链例外必须包含 DSH 根包', () => {
  assert.throws(
    () =>
      verifyDshReleaseAgeExclusions(
        `minimumReleaseAgeExclude:\n  - '@deepseek-ai/dsh-base@0.1.5-rc.2'\n`,
        CURRENT_VERSION,
      ),
    /根包/u,
  );
});
