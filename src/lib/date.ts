/**
 * 本地日期工具：全部以 "YYYY-MM-DD" 字符串为交换格式（与后端聚合表一致），
 * 内部用本地 Date 运算，避免 UTC/时区偏移导致的跨日错误。
 *
 * 纯函数，单测覆盖日/周/月/自定义范围与跨月、跨年、闰年边界。
 */

export interface DateRange {
  start_date: string;
  end_date: string;
}

export type RangeKind = "day" | "week" | "month" | "year" | "custom";
export type TrendGranularity = "hour" | "day" | "month";

function pad(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}

export function toDateString(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function parseDate(s: string): Date {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, (m ?? 1) - 1, d ?? 1);
}

export function todayLocal(): string {
  return toDateString(new Date());
}

export function addDays(date: string, days: number): string {
  const d = parseDate(date);
  d.setDate(d.getDate() + days);
  return toDateString(d);
}

/** "YYYY-MM" */
export function monthOf(date: string): string {
  return date.slice(0, 7);
}

/** 周一为一周起点（与日历/计划一致）。 */
export function startOfWeek(date: string): string {
  const d = parseDate(date);
  const dow = (d.getDay() + 6) % 7; // 周一=0
  d.setDate(d.getDate() - dow);
  return toDateString(d);
}

export function endOfMonth(date: string): string {
  const d = parseDate(date);
  d.setMonth(d.getMonth() + 1, 0);
  return toDateString(d);
}

export function startOfMonth(date: string): string {
  const d = parseDate(date);
  d.setDate(1);
  return toDateString(d);
}

export function startOfYear(date: string): string {
  return `${parseDate(date).getFullYear()}-01-01`;
}

export function endOfYear(date: string): string {
  return `${parseDate(date).getFullYear()}-12-31`;
}

/** 预设范围：日=当天；周=本周一至周日；月=当月；年=自然年。 */
export function presetRange(kind: Exclude<RangeKind, "custom">, anchor = todayLocal()): DateRange {
  switch (kind) {
    case "day":
      return { start_date: anchor, end_date: anchor };
    case "week": {
      const start = startOfWeek(anchor);
      return { start_date: start, end_date: addDays(start, 6) };
    }
    case "month":
      return { start_date: startOfMonth(anchor), end_date: endOfMonth(anchor) };
    case "year":
      return { start_date: startOfYear(anchor), end_date: endOfYear(anchor) };
  }
}

/** 根据范围决定趋势图粒度；自定义范围按长度自动选择，避免轴标签拥挤。 */
export function trendGranularity(
  kind: RangeKind,
  start: string,
  end: string,
): TrendGranularity {
  if (kind === "day") return "hour";
  if (kind === "week" || kind === "month") return "day";
  if (kind === "year") return "month";

  const days = daysBetween(start, end);
  if (days <= 1) return "hour";
  if (days <= 62) return "day";
  return "month";
}

/** 范围内天数（闭区间）。 */
export function daysBetween(start: string, end: string): number {
  const ms = parseDate(end).getTime() - parseDate(start).getTime();
  return Math.floor(ms / 86_400_000) + 1;
}

/** 枚举闭区间内所有日期（用于日历补零、趋势图补点）。 */
export function enumerateDays(start: string, end: string): string[] {
  const out: string[] = [];
  let cur = start;
  // 防御：start > end 时返回空数组
  if (parseDate(start) > parseDate(end)) return out;
  while (cur <= end) {
    out.push(cur);
    cur = addDays(cur, 1);
  }
  return out;
}

/** 该月所有日期（按自然顺序），用于月历补零。 */
export function monthDays(month: string): string[] {
  const start = `${month}-01`;
  return enumerateDays(start, endOfMonth(start));
}
