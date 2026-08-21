export interface AboutDialogPayload {
  appName: string;
  version: string;
  description: string;
  disclaimer: string;
  buildTimestampMs: number;
  buildId: string;
  platform: string;
  dshVersion: string;
  nodeVersion: string;
  pnpmVersion: string;
  website: string;
  author: string;
}

export interface AboutDetailEntry {
  label: string;
  value: string;
}

export interface AboutDetailSection {
  title: string;
  entries: AboutDetailEntry[];
}

function padNumber(value: number): string {
  return String(value).padStart(2, '0');
}

function formatUtcOffset(offsetMinutes: number): string {
  const sign = offsetMinutes >= 0 ? '+' : '-';
  const absoluteMinutes = Math.abs(offsetMinutes);
  const hours = Math.floor(absoluteMinutes / 60);
  const minutes = absoluteMinutes % 60;
  return `UTC${sign}${hours}${minutes === 0 ? '' : `:${padNumber(minutes)}`}`;
}

/** 按用户所在时区展示构建时刻，并明确标注偏移量。 */
export function formatBuildTime(
  timestampMs: number,
  timezoneOffsetMinutes = -new Date(timestampMs).getTimezoneOffset(),
): string {
  if (
    timestampMs <= 0 ||
    !Number.isFinite(timestampMs) ||
    !Number.isFinite(timezoneOffsetMinutes)
  ) {
    return '未知';
  }
  const localTime = new Date(timestampMs + timezoneOffsetMinutes * 60_000);
  if (Number.isNaN(localTime.getTime())) return '未知';
  const date = [
    localTime.getUTCFullYear(),
    padNumber(localTime.getUTCMonth() + 1),
    padNumber(localTime.getUTCDate()),
  ].join('-');
  const time = `${padNumber(localTime.getUTCHours())}:${padNumber(localTime.getUTCMinutes())}`;
  return `${date} ${time} ${formatUtcOffset(timezoneOffsetMinutes)}`;
}

/** 生成版权文本；构建时间不可用时不虚构年份。 */
export function formatCopyright(timestampMs: number, author: string): string {
  const year = timestampMs > 0 ? new Date(timestampMs).getUTCFullYear() : Number.NaN;
  return Number.isFinite(year) ? `© ${year} ${author}` : `© ${author}`;
}

/** 生成关于窗口的两个紧凑信息分组。 */
export function buildAboutSections(
  payload: AboutDialogPayload,
  timezoneOffsetMinutes?: number,
): AboutDetailSection[] {
  return [
    {
      title: '版本信息',
      entries: [
        { label: '应用版本', value: payload.version },
        {
          label: '构建时间',
          value: formatBuildTime(payload.buildTimestampMs, timezoneOffsetMinutes),
        },
        { label: '构建标识', value: payload.buildId },
        { label: '运行平台', value: payload.platform },
      ],
    },
    {
      title: '内置运行时',
      entries: [
        { label: 'DeepSeek Harness', value: payload.dshVersion },
        { label: 'Node.js', value: payload.nodeVersion },
        { label: 'pnpm', value: payload.pnpmVersion },
      ],
    },
  ];
}

/** 生成不包含设备路径等敏感字段的公开版本文本。 */
export function createPublicVersionText(
  payload: AboutDialogPayload,
  timezoneOffsetMinutes?: number,
): string {
  return [
    `${payload.appName} ${payload.version}`,
    `构建时间：${formatBuildTime(payload.buildTimestampMs, timezoneOffsetMinutes)}`,
    `构建标识：${payload.buildId}`,
    `运行平台：${payload.platform}`,
    `DeepSeek Harness：${payload.dshVersion}`,
    `Node.js：${payload.nodeVersion}`,
    `pnpm：${payload.pnpmVersion}`,
    `项目主页：${payload.website}`,
  ].join('\n');
}
