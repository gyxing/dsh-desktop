import { readRuntimeLock, npmRegistry } from './runtime-config.mjs';
import { setTimeout as delay } from 'node:timers/promises';

const npmDistTagsUrl = `${npmRegistry}/-/package/@deepseek-ai%2Fdsh/dist-tags`;
const npmVersionUrl = (version) =>
  `${npmRegistry}/@deepseek-ai%2Fdsh/${encodeURIComponent(version)}`;
const githubFeedUrl = 'https://github.com/deepseek-ai/deepseek-harness/commits/master.atom';
const releaseDriftExitCode = 10;

/** 对瞬时网络错误和服务端错误做有限重试，客户端错误立即返回。 */
async function request(url, { allowNotFound = false } = {}) {
  let lastError;
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    let response;
    try {
      response = await fetch(url, {
        headers: { 'User-Agent': 'dsh-desktop-upstream-check' },
      });
    } catch (error) {
      lastError = error;
    }
    if (response?.ok) {
      return response;
    }
    if (response?.status === 404 && allowNotFound) {
      return null;
    }
    if (response && response.status < 500) {
      throw new Error(`上游检查失败：${response.status} ${response.statusText}`);
    }
    if (response) {
      lastError = new Error(`上游检查失败：${response.status} ${response.statusText}`);
    }
    if (attempt < 3) {
      await delay(attempt * 1_000);
    }
  }
  throw lastError;
}

/** 解析规范的 SemVer；运行时锁只使用 X.Y.Z 或 X.Y.Z-预发布 形式。 */
function parseVersion(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(String(value).trim());
  if (!match) {
    return null;
  }
  return {
    numbers: [Number(match[1]), Number(match[2]), Number(match[3])],
    prerelease: match[4] ? match[4].split('.') : [],
  };
}

/** 按 SemVer 2.0 比较版本，返回 -1、0 或 1；无法解析时抛错，避免静默放行未知版本。 */
function compareVersions(left, right) {
  const leftVersion = parseVersion(left);
  const rightVersion = parseVersion(right);
  if (!leftVersion || !rightVersion) {
    throw new Error(`无法比较版本：${left} / ${right}`);
  }
  for (let index = 0; index < 3; index += 1) {
    if (leftVersion.numbers[index] !== rightVersion.numbers[index]) {
      return leftVersion.numbers[index] < rightVersion.numbers[index] ? -1 : 1;
    }
  }
  if (leftVersion.prerelease.length === 0 || rightVersion.prerelease.length === 0) {
    if (leftVersion.prerelease.length === rightVersion.prerelease.length) {
      return 0;
    }
    // 带预发布标识的版本低于同号正式版。
    return leftVersion.prerelease.length === 0 ? 1 : -1;
  }
  const length = Math.max(leftVersion.prerelease.length, rightVersion.prerelease.length);
  for (let index = 0; index < length; index += 1) {
    const leftIdentifier = leftVersion.prerelease[index];
    const rightIdentifier = rightVersion.prerelease[index];
    if (leftIdentifier === undefined || rightIdentifier === undefined) {
      return leftIdentifier === undefined ? -1 : 1;
    }
    if (leftIdentifier === rightIdentifier) {
      continue;
    }
    const leftNumeric = /^\d+$/.test(leftIdentifier);
    const rightNumeric = /^\d+$/.test(rightIdentifier);
    if (leftNumeric && rightNumeric) {
      return Number(leftIdentifier) < Number(rightIdentifier) ? -1 : 1;
    }
    if (leftNumeric !== rightNumeric) {
      // 数字标识符优先级低于字母标识符。
      return leftNumeric ? -1 : 1;
    }
    return leftIdentifier < rightIdentifier ? -1 : 1;
  }
  return 0;
}

/** 区分同版本重新发布漂移、上游存在更新发布与 master 源码演进，不自动改锁文件或替换运行时。 */
async function checkUpstream() {
  const runtimeLock = await readRuntimeLock();
  const [tagsResponse, githubResponse] = await Promise.all([
    request(npmDistTagsUrl),
    request(githubFeedUrl),
  ]);
  const distTags = await tagsResponse.json();
  const githubFeed = await githubResponse.text();
  const commitMatch = githubFeed.match(/Grit::Commit\/([0-9a-f]{40})/i);
  if (!commitMatch) {
    throw new Error('无法从 GitHub 提交源解析 master SHA');
  }

  const lockedVersion = String(runtimeLock.dsh.version);
  const latestVersion = String(distTags.latest ?? '');
  const nextVersion = String(distTags.next ?? '');
  // 锁定版本可能取自 next 预发布标签，因此上游发布线以 latest 与 next 中更高者为准。
  const publishedVersions = [latestVersion, nextVersion].filter(
    (version, index, all) => version && all.indexOf(version) === index,
  );
  const newestPublishedVersion = publishedVersions.reduce(
    (newest, version) => (!newest || compareVersions(version, newest) > 0 ? version : newest),
    '',
  );
  const currentMasterCommit = commitMatch[1].toLowerCase();

  console.info(`已锁定 npm 版本：${lockedVersion}`);
  console.info(`npm latest：${latestVersion || '<缺失>'}`);
  console.info(`npm next：${nextVersion || '<缺失>'}`);
  console.info(`上次审计 master：${runtimeLock.dsh.observedMasterCommit}`);
  console.info(`当前 master：${currentMasterCommit}`);

  const lockedMetadataResponse = await request(npmVersionUrl(lockedVersion), {
    allowNotFound: true,
  });
  if (!lockedMetadataResponse) {
    console.warn(`锁定版本 ${lockedVersion} 已从 npm 撤回；请重新审查并锁定可用版本。`);
    process.exitCode = releaseDriftExitCode;
    return;
  }
  const lockedMetadata = await lockedMetadataResponse.json();
  const lockedIntegrity = String(lockedMetadata.dist?.integrity ?? '');
  const lockedShasum = String(lockedMetadata.dist?.shasum ?? '');
  if (
    lockedIntegrity !== runtimeLock.dsh.integrity ||
    (lockedShasum && lockedShasum !== runtimeLock.dsh.shasum)
  ) {
    console.warn(`锁定版本 ${lockedVersion} 在 npm 上被重新发布；请完成兼容性审查后重新锁定。`);
    process.exitCode = releaseDriftExitCode;
    return;
  }

  if (newestPublishedVersion && compareVersions(newestPublishedVersion, lockedVersion) > 0) {
    console.warn(
      `上游已发布更新版本 ${newestPublishedVersion}（大于锁定的 ${lockedVersion}）；请完成兼容性审查后重新锁定、构建和发布。`,
    );
    process.exitCode = releaseDriftExitCode;
    return;
  }

  if (currentMasterCommit !== runtimeLock.dsh.observedMasterCommit) {
    console.warn('master 已继续演进；当前打包仍以已锁定的 npm 发布包为准。');
    return;
  }

  console.info('npm 发布版本、完整性和 master 观察点均未漂移。');
}

await checkUpstream();
