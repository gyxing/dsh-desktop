import { readRuntimeLock } from './runtime-config.mjs';
import { setTimeout as delay } from 'node:timers/promises';

const npmLatestUrl = 'https://registry.npmjs.org/@deepseek-ai%2Fdsh/latest';
const githubFeedUrl = 'https://github.com/deepseek-ai/deepseek-harness/commits/master.atom';

/** 对瞬时网络错误和服务端错误做有限重试，客户端错误立即返回。 */
async function request(url) {
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
/** 区分npm发布漂移与master源码演进，不自动改锁文件或替换运行时。 */
async function checkUpstream() {
  const runtimeLock = await readRuntimeLock();
  const [npmResponse, githubResponse] = await Promise.all([
    request(npmLatestUrl),
    request(githubFeedUrl),
  ]);
  const npmMetadata = await npmResponse.json();
  const githubFeed = await githubResponse.text();
  const commitMatch = githubFeed.match(/Grit::Commit\/([0-9a-f]{40})/i);
  if (!commitMatch) {
    throw new Error('无法从 GitHub 提交源解析 master SHA');
  }

  const latestVersion = String(npmMetadata.version ?? '');
  const latestIntegrity = String(npmMetadata.dist?.integrity ?? '');
  const latestCommit = commitMatch[1].toLowerCase();
  const versionChanged = latestVersion !== runtimeLock.dsh.version;
  const integrityChanged = !versionChanged && latestIntegrity !== runtimeLock.dsh.integrity;
  const commitChanged = latestCommit !== runtimeLock.dsh.observedMasterCommit;

  console.info(`已锁定 npm 版本：${runtimeLock.dsh.version}`);
  console.info(`npm 最新发布版本：${latestVersion}`);
  console.info(`上次审计 master：${runtimeLock.dsh.observedMasterCommit}`);
  console.info(`当前 master：${latestCommit}`);

  if (versionChanged || integrityChanged) {
    console.warn('检测到 npm 发布漂移；请完成兼容性审查后重新锁定、构建和发布。');
    process.exitCode = 10;
    return;
  }

  if (commitChanged) {
    console.warn('master 已继续演进；当前打包仍以已锁定的 npm 发布包为准。');
    return;
  }

  console.info('npm 发布版本、完整性和 master 观察点均未漂移。');
}

await checkUpstream();
