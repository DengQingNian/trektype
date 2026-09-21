import { enumerateDays, parseDate, type TrendGranularity } from "./date";

export interface TrendPoint {
  bucket: string;
  key_count: number;
  click_count: number;
}

/** 生成趋势图完整横轴，保证没有记录的小时/日期/月也显示为 0。 */
export function enumerateTrendBuckets(
  start: string,
  end: string,
  granularity: TrendGranularity,
): string[] {
  if (start > end) return [];
  if (granularity === "hour") {
    return Array.from({ length: 24 }, (_, hour) => `${start}T${String(hour).padStart(2, "0")}`);
  }
  if (granularity === "day") return enumerateDays(start, end);

  const cursor = parseDate(start);
  cursor.setDate(1);
  const last = parseDate(end);
  last.setDate(1);
  const buckets: string[] = [];
  while (cursor <= last) {
    buckets.push(`${cursor.getFullYear()}-${String(cursor.getMonth() + 1).padStart(2, "0")}`);
    cursor.setMonth(cursor.getMonth() + 1, 1);
  }
  return buckets;
}

/** 将后端仅返回的有数据点补齐为完整序列，供 ECharts 直接消费。 */
export function completeTrend(
  points: TrendPoint[],
  start: string,
  end: string,
  granularity: TrendGranularity,
): TrendPoint[] {
  const byBucket = new Map(points.map((point) => [point.bucket, point]));
  return enumerateTrendBuckets(start, end, granularity).map((bucket) => {
    const point = byBucket.get(bucket);
    return point ?? { bucket, key_count: 0, click_count: 0 };
  });
}

/** 趋势横轴短标签：小时显示 HH:00，日显示 MM-DD，月显示 YYYY/MM。 */
export function trendLabel(bucket: string, granularity: TrendGranularity): string {
  if (granularity === "hour") return `${bucket.slice(11, 13)}:00`;
  if (granularity === "day") return bucket.slice(5);
  return bucket.replace("-", "/");
}

/** 计算趋势洞察：总事件、峰值点和非零点数量。 */
export function summarizeTrend(points: TrendPoint[]) {
  let total = 0;
  let activeBuckets = 0;
  let peak: TrendPoint | null = null;
  for (const point of points) {
    const value = point.key_count + point.click_count;
    total += value;
    if (value > 0) activeBuckets += 1;
    if (!peak || value > peak.key_count + peak.click_count) peak = point;
  }
  return { total, activeBuckets, peak };
}
