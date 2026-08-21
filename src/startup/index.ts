import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import {
  getErrorMessage,
  getRuntimeElements,
  renderRuntimeStatus,
  type RuntimeStatus,
} from './runtime-status';
import './index.css';

const elements = getRuntimeElements();

async function handleRetry(): Promise<void> {
  renderRuntimeStatus(elements, {
    phase: 'starting',
    message: '正在重新启动 DeepSeek Harness…',
  });

  try {
    await invoke('restart_runtime');
  } catch (error: unknown) {
    renderRuntimeStatus(elements, {
      phase: 'failed',
      code: 'RUNTIME_COMMUNICATION',
      message: `重新启动失败：${getErrorMessage(error)}`,
    });
  }
}

async function copyText(value: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(value);
    return;
  }

  const textarea = document.createElement('textarea');
  textarea.value = value;
  textarea.setAttribute('readonly', '');
  textarea.className = 'startup-card-copy-source';
  document.body.append(textarea);
  textarea.select();
  const copied = document.execCommand('copy');
  textarea.remove();
  if (!copied) {
    throw new Error('当前 WebView 不支持剪贴板写入');
  }
}

async function handleCopyDiagnostics(): Promise<void> {
  elements.diagnosticsButton.disabled = true;
  elements.actionFeedback.textContent = '正在准备诊断信息…';

  try {
    const diagnostics = await invoke<string>('runtime_diagnostics');
    await copyText(diagnostics);
    elements.actionFeedback.textContent = '诊断信息已复制，请确认内容后再发送。';
  } catch (error: unknown) {
    elements.actionFeedback.textContent = `复制失败：${getErrorMessage(error)}`;
  } finally {
    elements.diagnosticsButton.disabled = false;
  }
}

/** 建立状态订阅，并读取订阅前可能已经产生的最新状态。 */
async function initializeRuntimeStatus(): Promise<void> {
  await listen<RuntimeStatus>('runtime://status', (event) => {
    renderRuntimeStatus(elements, event.payload);
  });

  const status = await invoke<RuntimeStatus>('runtime_status');
  renderRuntimeStatus(elements, status);
}

elements.retryButton.addEventListener('click', () => {
  void handleRetry();
});

elements.diagnosticsButton.addEventListener('click', () => {
  void handleCopyDiagnostics();
});

initializeRuntimeStatus().catch((error: unknown) => {
  renderRuntimeStatus(elements, {
    phase: 'failed',
    code: 'RUNTIME_COMMUNICATION',
    message: `桌面运行时初始化失败：${getErrorMessage(error)}`,
  });
});
