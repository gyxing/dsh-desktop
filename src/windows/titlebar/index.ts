import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import { requireElement } from '../../shared/dom';
import './index.css';

type ChromeMenuKind = 'application' | 'edit' | 'update' | 'help';
type WindowChromeAction = 'startDragging' | 'toggleMaximize' | 'minimize' | 'close';

interface WindowChromeState {
  maximized: boolean;
  title: string;
  updateText?: string;
}

const title = requireElement('#chrome-title', HTMLSpanElement);
const status = requireElement('#chrome-status-text', HTMLSpanElement);
const maximizeButton = requireElement('#chrome-maximize', HTMLButtonElement);

function renderState(state: WindowChromeState): void {
  title.textContent = state.title;
  status.textContent = state.updateText ?? '';
  status.title = state.updateText ?? '';
  maximizeButton.dataset.maximized = String(state.maximized);
  maximizeButton.setAttribute('aria-label', state.maximized ? '还原窗口' : '最大化');
  maximizeButton.title = state.maximized ? '还原窗口' : '最大化';
  document.title = state.title;
}

async function runWindowAction(action: WindowChromeAction): Promise<void> {
  try {
    await invoke('window_chrome_action', { action });
  } catch (error: unknown) {
    status.textContent = `窗口操作失败：${error instanceof Error ? error.message : String(error)}`;
  }
}

document.querySelectorAll<HTMLElement>('[data-chrome-drag]').forEach((element) => {
  element.addEventListener('mousedown', (event) => {
    if (event.button !== 0) return;
    void runWindowAction(event.detail >= 2 ? 'toggleMaximize' : 'startDragging');
  });
});

document.querySelectorAll<HTMLButtonElement>('[data-window-action]').forEach((button) => {
  button.addEventListener('click', () => {
    const action = button.dataset.windowAction as WindowChromeAction | undefined;
    if (action) void runWindowAction(action);
  });
});

document.querySelectorAll<HTMLButtonElement>('[data-menu]').forEach((button) => {
  button.addEventListener('click', async () => {
    const menu = button.dataset.menu as ChromeMenuKind | undefined;
    if (!menu) return;
    const bounds = button.getBoundingClientRect();
    try {
      await invoke('show_chrome_menu', {
        menu,
        x: bounds.left,
        y: bounds.bottom,
      });
    } catch (error: unknown) {
      status.textContent = `菜单打开失败：${error instanceof Error ? error.message : String(error)}`;
    }
  });
});

async function initialize(): Promise<void> {
  await listen<WindowChromeState>('window-chrome://state', (event) => {
    renderState(event.payload);
  });
  renderState(await invoke<WindowChromeState>('window_chrome_state'));
}

initialize().catch((error: unknown) => {
  status.textContent = `标题栏初始化失败：${error instanceof Error ? error.message : String(error)}`;
});
