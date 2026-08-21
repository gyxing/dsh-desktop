function unquote(value) {
  const trimmed = value.trim();
  if (
    (trimmed.startsWith("'") && trimmed.endsWith("'")) ||
    (trimmed.startsWith('"') && trimmed.endsWith('"'))
  ) {
    return trimmed.slice(1, -1);
  }
  return trimmed;
}

function readReleaseAgeExclusions(workspaceYaml) {
  const lines = workspaceYaml.split(/\r?\n/u);
  const start = lines.findIndex((line) => line === 'minimumReleaseAgeExclude:');
  if (start < 0) {
    throw new Error('pnpm-workspace.yaml 缺少 minimumReleaseAgeExclude');
  }
  const exclusions = [];
  for (const line of lines.slice(start + 1)) {
    if (/^\S/u.test(line)) break;
    const item = line.match(/^\s{2}-\s+(.+)$/u)?.[1];
    if (item) exclusions.push(unquote(item));
  }
  return exclusions;
}

/** 校验当前运行时对应的 DSH 发布时间例外。 */
export function verifyDshReleaseAgeExclusions(workspaceYaml, runtimeVersion) {
  const dshExclusions = readReleaseAgeExclusions(workspaceYaml).filter((value) =>
    /^@deepseek-ai\/dsh(?:-|@)/u.test(value),
  );
  const expectedSuffix = `@${runtimeVersion}`;
  const stale = dshExclusions.filter((value) => !value.endsWith(expectedSuffix));
  if (stale.length > 0) {
    throw new Error(`DSH 发布时间例外包含非当前版本：${stale.join(', ')}`);
  }
  const rootPackage = `@deepseek-ai/dsh@${runtimeVersion}`;
  if (!dshExclusions.includes(rootPackage)) {
    throw new Error(`DSH 发布时间例外缺少根包：${rootPackage}`);
  }
  return { dshExclusionCount: dshExclusions.length };
}
