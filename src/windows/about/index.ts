import { invoke } from '@tauri-apps/api/core';

import { requireElement } from '../../shared/dom';
import {
  buildAboutSections,
  createPublicVersionText,
  formatCopyright,
  type AboutDetailSection,
  type AboutDialogPayload,
} from './content';
import './index.css';

const title = requireElement('#about-title', HTMLHeadingElement);
const description = requireElement('#about-description', HTMLParagraphElement);
const disclaimer = requireElement('#about-disclaimer', HTMLSpanElement);
const details = requireElement('#about-details', HTMLElement);
const copyright = requireElement('#about-copyright', HTMLSpanElement);
const feedback = requireElement('#about-feedback', HTMLSpanElement);
const copyButton = requireElement('#about-copy', HTMLButtonElement);
const websiteButton = requireElement('#about-website', HTMLButtonElement);
const closeButton = requireElement('#about-close', HTMLButtonElement);

let currentPayload: AboutDialogPayload | undefined;

function renderSection(section: AboutDetailSection): HTMLElement {
  const card = document.createElement('article');
  card.className = 'about-card';
  const heading = document.createElement('h2');
  heading.className = 'about-card-title';
  heading.textContent = section.title;
  const list = document.createElement('dl');
  list.className = 'about-list';
  section.entries.forEach((entry) => {
    const row = document.createElement('div');
    row.className = 'about-row';
    const label = document.createElement('dt');
    label.className = 'about-row-label';
    label.textContent = entry.label;
    const value = document.createElement('dd');
    value.className = 'about-row-value';
    value.textContent = entry.value;
    value.title = entry.value;
    row.append(label, value);
    list.append(row);
  });
  card.append(heading, list);
  return card;
}

function renderPayload(payload: AboutDialogPayload): void {
  currentPayload = payload;
  title.textContent = `${payload.appName} ${payload.version}`;
  description.textContent = payload.description;
  disclaimer.textContent = payload.disclaimer;
  details.replaceChildren(...buildAboutSections(payload).map(renderSection));
  copyright.textContent = formatCopyright(payload.buildTimestampMs, payload.author);
  document.title = `关于 ${payload.appName}`;
  closeButton.focus();
}

async function runButtonAction(
  button: HTMLButtonElement,
  action: () => Promise<void>,
): Promise<void> {
  copyButton.disabled = true;
  websiteButton.disabled = true;
  button.setAttribute('aria-busy', 'true');
  feedback.textContent = '';
  try {
    await action();
  } catch (error: unknown) {
    feedback.textContent = `操作失败：${error instanceof Error ? error.message : String(error)}`;
  } finally {
    copyButton.disabled = false;
    websiteButton.disabled = false;
    button.removeAttribute('aria-busy');
  }
}

copyButton.addEventListener('click', () => {
  const payload = currentPayload;
  if (!payload) return;
  void runButtonAction(copyButton, async () => {
    await invoke('copy_about_info', { text: createPublicVersionText(payload) });
    feedback.textContent = '版本信息已复制';
  });
});

websiteButton.addEventListener('click', () => {
  void runButtonAction(websiteButton, async () => {
    await invoke('open_about_website');
    feedback.textContent = '已在浏览器打开项目主页';
  });
});

closeButton.addEventListener('click', () => {
  void invoke('close_about_dialog');
});

window.addEventListener('keydown', (event) => {
  if (event.key === 'Escape') {
    event.preventDefault();
    void invoke('close_about_dialog');
  }
});

async function initialize(): Promise<void> {
  renderPayload(await invoke<AboutDialogPayload>('about_dialog_payload'));
}

initialize().catch((error: unknown) => {
  title.textContent = '无法显示关于信息';
  description.textContent = error instanceof Error ? error.message : String(error);
  disclaimer.textContent = '';
  details.replaceChildren();
  copyButton.disabled = true;
  websiteButton.disabled = true;
});
