import fs from 'node:fs';
import path from 'node:path';

const tag =
  process.argv.slice(2).find((argument) => argument !== '--') ?? process.env.GITHUB_REF_NAME;
if (!tag || !/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
  throw new Error(`发布标签必须是 vX.Y.Z 或预发布 SemVer，收到：${tag ?? '<empty>'}`);
}

// 发布标签必须与 package、Tauri、Cargo清单和锁文件四个版本源严格一致。
const expected = tag.slice(1);
const packageJson = JSON.parse(fs.readFileSync('package.json', 'utf8'));
const tauriConfig = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8'));
const cargoToml = fs.readFileSync('src-tauri/Cargo.toml', 'utf8');
const cargoLock = fs.readFileSync('src-tauri/Cargo.lock', 'utf8');
const cargoVersion = cargoToml.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const cargoLockVersion = cargoLock.match(
  /^name\s*=\s*"dsh-desktop"\r?\nversion\s*=\s*"([^"]+)"/m,
)?.[1];

const versions = {
  'package.json': packageJson.version,
  'src-tauri/tauri.conf.json': tauriConfig.version,
  'src-tauri/Cargo.toml': cargoVersion,
  'src-tauri/Cargo.lock': cargoLockVersion,
};
const mismatches = Object.entries(versions).filter(([, value]) => value !== expected);
if (mismatches.length > 0) {
  const detail = mismatches.map(([file, value]) => `${file}=${value ?? '<missing>'}`).join(', ');
  throw new Error(`标签 ${tag} 与项目版本 ${expected} 不一致：${detail}`);
}
const releaseNotesPath = path.join('release-notes', `${tag}.md`);
if (!fs.existsSync(releaseNotesPath)) {
  throw new Error(`缺少版本更新说明：${releaseNotesPath}`);
}
const releaseNotes = fs.readFileSync(releaseNotesPath, 'utf8');
if (!releaseNotes.includes(`DSH Desktop \`${expected}\``)) {
  throw new Error(`版本更新说明未标记当前版本：${expected}`);
}
if (/\b(?:TBD|TODO)\b/u.test(releaseNotes)) {
  throw new Error(`版本更新说明包含未完成占位内容：${releaseNotesPath}`);
}
console.log(`发布版本校验通过：${tag}`);
