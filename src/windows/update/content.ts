export type ReleaseNoteBlock =
  | { kind: 'heading'; text: string }
  | { kind: 'paragraph'; text: string }
  | { kind: 'list'; items: string[] };

/** 将远程更新说明收窄为纯文本块，渲染层只创建文本节点，禁止注入HTML。 */
export function parseReleaseNotes(notes: string): ReleaseNoteBlock[] {
  const normalized = notes.trim();
  if (!normalized) {
    return [{ kind: 'paragraph', text: '此版本未提供更新说明。' }];
  }

  const blocks: ReleaseNoteBlock[] = [];
  let listItems: string[] = [];
  let paragraphLines: string[] = [];
  const flushList = (): void => {
    if (listItems.length > 0) {
      blocks.push({ kind: 'list', items: listItems });
      listItems = [];
    }
  };
  const flushParagraph = (): void => {
    if (paragraphLines.length > 0) {
      blocks.push({ kind: 'paragraph', text: paragraphLines.join('\n') });
      paragraphLines = [];
    }
  };

  normalized.split(/\r?\n/u).forEach((rawLine) => {
    const line = rawLine.trim();
    if (!line || /^-{3,}$/u.test(line)) {
      flushParagraph();
      flushList();
      return;
    }
    const heading = line.match(/^#{1,4}\s+(.+)$/u);
    if (heading) {
      flushParagraph();
      flushList();
      blocks.push({ kind: 'heading', text: heading[1] });
      return;
    }
    const listItem = line.match(/^[-*]\s+(.+)$/u);
    if (listItem) {
      flushParagraph();
      listItems.push(listItem[1]);
      return;
    }
    flushList();
    paragraphLines.push(line);
  });
  flushParagraph();
  flushList();
  return blocks;
}
