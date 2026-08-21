/** 获取必需 DOM 元素；缺失或类型错误时立即暴露本地窗口打包问题。 */
export function requireElement<TElement extends HTMLElement>(
  selector: string,
  type: new () => TElement,
): TElement {
  const element = document.querySelector(selector);
  if (!(element instanceof type)) {
    throw new Error(`本地窗口元素不存在或类型错误：${selector}`);
  }
  return element;
}
