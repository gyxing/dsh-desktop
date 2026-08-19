const targetByHost = new Map([
  ['win32:x64', 'x86_64-pc-windows-msvc'],
  ['darwin:arm64', 'aarch64-apple-darwin'],
  ['darwin:x64', 'x86_64-apple-darwin'],
  ['linux:x64', 'x86_64-unknown-linux-gnu'],
]);

export const supportedTargets = Object.freeze([...targetByHost.values()]);

/** 从Node宿主平台和架构解析Tauri目标三元组。 */
export function inferHostTarget(platform = process.platform, arch = process.arch) {
  const target = targetByHost.get(`${platform}:${arch}`);
  if (!target) {
    throw new Error(`不支持的宿主平台：${platform}/${arch}`);
  }
  return target;
}

/** 读取一次可选的 --target 参数，拒绝重复值和缺失值。 */
export function parseTargetArgument(args = process.argv.slice(2)) {
  let target;
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    let value;
    if (argument === '--target') {
      value = args[index + 1];
      index += 1;
    } else if (argument.startsWith('--target=')) {
      value = argument.slice('--target='.length);
    } else {
      continue;
    }
    if (!value) {
      throw new Error('--target 需要目标三元组');
    }
    if (target) {
      throw new Error('--target 只能指定一次');
    }
    target = value;
  }
  return target;
}

/** 按命令参数、环境变量、宿主平台的优先级解析构建目标。 */
export function resolveRuntimeTarget({
  args = process.argv.slice(2),
  env = process.env,
  platform = process.platform,
  arch = process.arch,
} = {}) {
  const target =
    parseTargetArgument(args) ?? env.DSH_DESKTOP_TARGET ?? inferHostTarget(platform, arch);
  if (!supportedTargets.includes(target)) {
    throw new Error(`不支持的运行时目标：${target}`);
  }
  return target;
}

/** 当前阶段只允许在匹配目标的原生宿主上部署原生依赖。 */
export function assertNativeTarget(target, platform = process.platform, arch = process.arch) {
  const hostTarget = inferHostTarget(platform, arch);
  if (target !== hostTarget) {
    throw new Error(`运行时目标 ${target} 与宿主 ${hostTarget} 不一致，请使用原生构建机`);
  }
}
