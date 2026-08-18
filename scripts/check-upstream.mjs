import { readRuntimeLock } from './runtime-config.mjs';

const npmLatestUrl = 'https://registry.npmjs.org/@deepseek-ai%2Fdsh/latest';
const githubFeedUrl = 'https://github.com/deepseek-ai/deepseek-harness/commits/master.atom';

async function request(url) {
  const response = await fetch(url, {
    headers: { 'User-Agent': 'dsh-desktop-upstream-check' },
  });
  if (!response.ok) {
    throw new Error(`上游检查失败：${response.status} ${response.statusText}`);
  }
  return response;
}

/** 只报告上游漂移，不自动改锁文件或替换运行时。 */
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
  const commitChanged = latestCommit !== runtimeLock.dsh.upstreamCommit;

  console.info(`已锁定 DSH：${runtimeLock.dsh.version}`);
  console.info(`npm 最新版：${latestVersion}`);
  console.info(`已锁定源码：${runtimeLock.dsh.upstreamCommit}`);
  console.info(`master 最新：${latestCommit}`);

  if (versionChanged || integrityChanged || commitChanged) {
    console.warn('检测到上游漂移；请完成兼容性审查后重新锁定、构建和发布。');
    process.exitCode = 10;
    return;
  }

  console.info('上游版本、完整性和源码提交均未漂移。');
}

await checkUpstream();
