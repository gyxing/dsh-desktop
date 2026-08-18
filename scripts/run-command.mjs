import { spawn } from 'node:child_process';

/** 运行构建命令，并将真实退出码作为构建结果。 */
export async function runCommand(command, args, options = {}) {
  return await new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: options.cwd,
      env: options.env,
      stdio: options.capture ? ['ignore', 'pipe', 'pipe'] : 'inherit',
      windowsHide: true,
    });

    let output = '';
    child.stdout?.on('data', (chunk) => {
      output += chunk.toString();
    });
    child.stderr?.on('data', (chunk) => {
      output += chunk.toString();
    });
    child.on('error', reject);
    child.on('exit', (code) => {
      if (code === 0) {
        resolve(output.trim());
        return;
      }

      reject(new Error(`${command} 执行失败，退出码：${String(code)}\n${output}`));
    });
  });
}
