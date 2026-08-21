export type RuntimePhase = 'starting' | 'probing' | 'loading' | 'ready' | 'failed' | 'exited';

export type RuntimeErrorCode =
  | 'RUNTIME_MISSING'
  | 'SPAWN_FAILED'
  | 'PROCESS_TREE_FAILED'
  | 'READINESS_INVALID'
  | 'STARTUP_TIMEOUT'
  | 'HTTP_UNREACHABLE'
  | 'PAGE_LOAD_FAILED'
  | 'PROCESS_EXITED'
  | 'RUNTIME_COMMUNICATION';

export interface RuntimeStatus {
  phase: RuntimePhase;
  message: string;
  code?: RuntimeErrorCode;
  url?: string;
}

export interface RuntimeElements {
  stage: HTMLElement;
  kicker: HTMLSpanElement;
  title: HTMLHeadingElement;
  description: HTMLParagraphElement;
  message: HTMLParagraphElement;
  indicator: HTMLDivElement;
  errorCode: HTMLParagraphElement;
  progressSteps: HTMLLIElement[];
  retryButton: HTMLButtonElement;
  diagnosticsButton: HTMLButtonElement;
  actionFeedback: HTMLParagraphElement;
}

interface RuntimeView {
  kicker: string;
  title: string;
  description: string;
  message: string;
  step: number;
  isError: boolean;
}

const PENDING_VIEWS: Record<'starting' | 'probing' | 'loading', Omit<RuntimeView, 'isError'>> = {
  starting: {
    kicker: '本地运行时',
    title: '正在准备 DeepSeek Harness',
    description: '正在启动本地运行时，通常只需几秒。',
    message: '正在启动本地运行时…',
    step: 0,
  },
  probing: {
    kicker: '本地运行时',
    title: '正在准备 DeepSeek Harness',
    description: '正在启动本地运行时，通常只需几秒。',
    message: '正在检查本地服务…',
    step: 1,
  },
  loading: {
    kicker: '本地运行时',
    title: '正在准备 DeepSeek Harness',
    description: '正在启动本地运行时，通常只需几秒。',
    message: '正在打开工作界面…',
    step: 2,
  },
};

// 失败发生后只保留错误码，按实际启动环节恢复进度位置。
const ERROR_STEPS: Partial<Record<RuntimeErrorCode, number>> = {
  RUNTIME_MISSING: 0,
  SPAWN_FAILED: 0,
  PROCESS_TREE_FAILED: 0,
  READINESS_INVALID: 0,
  RUNTIME_COMMUNICATION: 0,
  STARTUP_TIMEOUT: 1,
  HTTP_UNREACHABLE: 1,
  PAGE_LOAD_FAILED: 2,
  PROCESS_EXITED: 2,
};

function getRuntimeView(status: RuntimeStatus): RuntimeView {
  if (status.phase === 'starting' || status.phase === 'probing' || status.phase === 'loading') {
    return { ...PENDING_VIEWS[status.phase], isError: false };
  }

  if (status.phase === 'failed') {
    return {
      kicker: '需要处理',
      title: '启动遇到问题',
      description: '本地服务未能完成启动。你可以重新启动，或复制诊断信息进行排查。',
      message: status.message,
      step: status.code ? (ERROR_STEPS[status.code] ?? 1) : 1,
      isError: true,
    };
  }

  if (status.phase === 'exited') {
    return {
      kicker: '运行已停止',
      title: 'DeepSeek Harness 已停止',
      description: '本地运行时已经退出。重新启动后，可以继续使用原有会话与配置。',
      message: status.message,
      step: 2,
      isError: true,
    };
  }

  return {
    kicker: '准备完成',
    title: 'DeepSeek Harness 已就绪',
    description: '本地服务和工作界面均已准备完成。',
    message: status.message,
    step: 2,
    isError: false,
  };
}

/** 获取启动页必需元素，缺失时立即暴露打包错误。 */
export function getRuntimeElements(): RuntimeElements {
  const stage = document.querySelector('#startup-stage');
  const kicker = document.querySelector('#runtime-kicker');
  const title = document.querySelector('#startup-title');
  const description = document.querySelector('#startup-description');
  const message = document.querySelector('#runtime-message');
  const indicator = document.querySelector('#runtime-indicator');
  const errorCode = document.querySelector('#runtime-error-code');
  const progressSteps = Array.from(document.querySelectorAll('[data-runtime-step]'));
  const retryButton = document.querySelector('#retry-button');
  const diagnosticsButton = document.querySelector('#diagnostics-button');
  const actionFeedback = document.querySelector('#action-feedback');

  if (!(stage instanceof HTMLElement)) {
    throw new Error('启动场景元素不存在');
  }

  if (!(kicker instanceof HTMLSpanElement)) {
    throw new Error('启动阶段元素不存在');
  }

  if (!(title instanceof HTMLHeadingElement)) {
    throw new Error('启动标题元素不存在');
  }

  if (!(description instanceof HTMLParagraphElement)) {
    throw new Error('启动说明元素不存在');
  }

  if (!(message instanceof HTMLParagraphElement)) {
    throw new Error('启动状态元素不存在');
  }

  if (!(retryButton instanceof HTMLButtonElement)) {
    throw new Error('重新启动按钮不存在');
  }

  if (!(indicator instanceof HTMLDivElement)) {
    throw new Error('启动状态指示器不存在');
  }

  if (!(errorCode instanceof HTMLParagraphElement)) {
    throw new Error('错误代码元素不存在');
  }

  if (
    progressSteps.length !== 3 ||
    progressSteps.some((element) => !(element instanceof HTMLLIElement))
  ) {
    throw new Error('启动进度元素不完整');
  }

  if (!(diagnosticsButton instanceof HTMLButtonElement)) {
    throw new Error('复制诊断按钮不存在');
  }

  if (!(actionFeedback instanceof HTMLParagraphElement)) {
    throw new Error('操作反馈元素不存在');
  }

  return {
    stage,
    kicker,
    title,
    description,
    message,
    indicator,
    errorCode,
    progressSteps: progressSteps as HTMLLIElement[],
    retryButton,
    diagnosticsButton,
    actionFeedback,
  };
}

/** 根据 Sidecar 状态同步用户可见反馈和重试入口。 */
export function renderRuntimeStatus(elements: RuntimeElements, status: RuntimeStatus): void {
  const view = getRuntimeView(status);
  const isPending =
    status.phase === 'starting' || status.phase === 'probing' || status.phase === 'loading';
  const isRecoverable = status.phase === 'failed' || status.phase === 'exited';

  elements.stage.dataset.phase = status.phase;
  elements.stage.classList.toggle('startup-stage-error', view.isError);
  elements.kicker.textContent = view.kicker;
  elements.title.textContent = view.title;
  elements.description.textContent = view.description;
  elements.message.textContent = view.message;
  elements.progressSteps.forEach((element, index) => {
    element.classList.toggle('is-complete', index < view.step);
    element.classList.toggle('is-active', index === view.step);
    element.classList.toggle('is-error', view.isError && index === view.step);
    if (index === view.step) {
      element.setAttribute('aria-current', 'step');
    } else {
      element.removeAttribute('aria-current');
    }
  });

  elements.message.setAttribute('role', isRecoverable ? 'alert' : 'status');
  elements.message.setAttribute('aria-live', isRecoverable ? 'assertive' : 'polite');
  elements.indicator.hidden = !isPending;
  elements.errorCode.hidden = !isRecoverable || !status.code;
  elements.errorCode.textContent = status.code ? `错误代码：${status.code}` : '';
  elements.retryButton.hidden = !isRecoverable;
  elements.retryButton.disabled = isPending;
  elements.diagnosticsButton.hidden = !isRecoverable;
  elements.diagnosticsButton.disabled = false;
  elements.actionFeedback.textContent = '';
}

/** 将未知异常转换为不包含运行环境细节的用户提示。 */
export function getErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
