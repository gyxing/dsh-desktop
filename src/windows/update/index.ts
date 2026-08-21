import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

import { requireElement } from '../../shared/dom';
import { parseReleaseNotes, type ReleaseNoteBlock } from './content';
import './index.css';

interface UpdateDialogPayload {
  version: string;
  notes: string;
  confirmation: boolean;
}

const title = requireElement('#update-title', HTMLHeadingElement);
const description = requireElement('#update-description', HTMLParagraphElement);
const notes = requireElement('#update-notes', HTMLElement);
const feedback = requireElement('#update-feedback', HTMLParagraphElement);
const laterButton = requireElement('#update-later', HTMLButtonElement);
const confirmButton = requireElement('#update-confirm', HTMLButtonElement);

function renderBlock(block: ReleaseNoteBlock): HTMLElement {
  if (block.kind === 'heading') {
    const heading = document.createElement('h2');
    heading.className = 'update-notes-heading';
    heading.textContent = block.text;
    return heading;
  }
  if (block.kind === 'list') {
    const list = document.createElement('ul');
    list.className = 'update-notes-list';
    block.items.forEach((item) => {
      const listItem = document.createElement('li');
      listItem.textContent = item;
      list.append(listItem);
    });
    return list;
  }
  const paragraph = document.createElement('p');
  paragraph.className = 'update-notes-paragraph';
  paragraph.textContent = block.text;
  return paragraph;
}

function renderPayload(payload: UpdateDialogPayload): void {
  title.textContent = payload.confirmation
    ? `发现 DSH Desktop ${payload.version}`
    : `DSH Desktop ${payload.version} 更新内容`;
  description.textContent = payload.confirmation
    ? '完整安装包将在后台下载，验签成功后才会开始安装。'
    : '以下内容来自已签名更新清单。';
  notes.replaceChildren(...parseReleaseNotes(payload.notes).map(renderBlock));
  laterButton.hidden = !payload.confirmation;
  confirmButton.textContent = payload.confirmation ? '下载并安装' : '关闭';
  feedback.textContent = '';
  confirmButton.focus();
}

async function respond(accepted: boolean): Promise<void> {
  laterButton.disabled = true;
  confirmButton.disabled = true;
  feedback.textContent = accepted ? '正在准备下载…' : '';
  try {
    await invoke('respond_update_dialog', { accepted });
  } catch (error: unknown) {
    feedback.textContent = `操作失败：${error instanceof Error ? error.message : String(error)}`;
    laterButton.disabled = false;
    confirmButton.disabled = false;
  }
}

laterButton.addEventListener('click', () => {
  void respond(false);
});

confirmButton.addEventListener('click', () => {
  void respond(!laterButton.hidden);
});

window.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') {
    event.preventDefault();
    void respond(false);
  }
});

async function initialize(): Promise<void> {
  await listen<UpdateDialogPayload>('updater-dialog://payload', (event) => {
    renderPayload(event.payload);
  });
  renderPayload(await invoke<UpdateDialogPayload>('update_dialog_payload'));
}

initialize().catch((error: unknown) => {
  title.textContent = '无法显示更新内容';
  description.textContent = error instanceof Error ? error.message : String(error);
  notes.replaceChildren();
  laterButton.hidden = true;
  confirmButton.textContent = '关闭';
});
