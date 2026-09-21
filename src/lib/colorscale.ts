/**
 * 色阶：键盘热力图 / 日历 / 屏幕热力图共用。
 *
 * 算法（计划 §8.1）：
 * 1. log1p 压缩长尾（空格、E 键常是字母键均值的数十倍）；
 * 2. P05/P99 分位裁剪，避免极值淹没主体；
 * 3. 归一化到 [0,1] 映射色带；
 * 4. count=0 用底色。
 *
 * 纯函数，单测覆盖 0 值/单值/极值/空数组。
 */

/** 键盘/日历用的单色渐变（低=浅底，高=主色）。 */
const KEY_RAMP: [number, number, number][] = [
  [232, 240, 254], // #e8f0fe 最浅
  [147, 197, 253],
  [59, 130, 246],
  [29, 78, 216],
  [23, 37, 84], // #172554 最深
];

/** 屏幕热力图用的经典热力带（蓝→绿→黄→红）。 */
const HEAT_RAMP: [number, number, number][] = [
  [0, 0, 128],
  [0, 128, 255],
  [0, 255, 128],
  [255, 255, 0],
  [255, 0, 0],
];

/** 取色带中 t∈[0,1] 处的 RGB。 */
export function rampColor(t: number, ramp: [number, number, number][] = KEY_RAMP): [number, number, number] {
  const clamped = Math.min(1, Math.max(0, t));
  if (!Number.isFinite(clamped)) return ramp[0];
  const scaled = clamped * (ramp.length - 1);
  const i = Math.min(ramp.length - 2, Math.floor(scaled));
  const f = scaled - i;
  const a = ramp[i];
  const b = ramp[i + 1];
  return [
    Math.round(a[0] + (b[0] - a[0]) * f),
    Math.round(a[1] + (b[1] - a[1]) * f),
    Math.round(a[2] + (b[2] - a[2]) * f),
  ];
}

export function rgbToCss(rgb: [number, number, number]): string {
  return `rgb(${rgb[0]}, ${rgb[1]}, ${rgb[2]})`;
}

/** 分位数（线性插值）；空数组返回 0。 */
export function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = (sorted.length - 1) * Math.min(1, Math.max(0, p));
  const lo = Math.floor(idx);
  const hi = Math.ceil(idx);
  if (lo === hi) return sorted[lo];
  return sorted[lo] + (sorted[hi] - sorted[lo]) * (idx - lo);
}

/** 归一化参数：lo/hi 为 log1p 空间的分位边界。 */
export interface ScaleBounds {
  lo: number;
  hi: number;
}

/**
 * 由计数数组计算归一化边界（log1p 空间 P05/P99）。
 * 所有值相同（含单值/全零）时返回 lo == hi，此时 `normalize` 对任何正值统一返回 1。
 */
export function computeBounds(counts: number[]): ScaleBounds {
  const positive = counts.filter((c) => c > 0).map((c) => Math.log1p(c)).sort((a, b) => a - b);
  if (positive.length === 0) return { lo: 0, hi: 0 };
  const lo = percentile(positive, 0.05);
  const hi = percentile(positive, 0.99);
  return { lo, hi: hi > lo ? hi : lo };
}

/** 计数 → 归一化热度 t∈[0,1]（count<=0 → 0）。
 *  当所有值相同（span=0，如单值/等值数据）时对任何正值返回 1：
 *  有输入就该显示为热点，否则"只按了几个键的一天"会渲染成一片空白。 */
export function normalize(count: number, bounds: ScaleBounds): number {
  if (count <= 0) return 0;
  const span = bounds.hi - bounds.lo;
  if (span <= 0) return 1;
  const v = Math.log1p(count);
  const t = (v - bounds.lo) / span;
  return Math.min(1, Math.max(0, t));
}

/**
 * 计数映射表 → 颜色（一次计算边界，避免逐键重复排序）。
 * 返回 CSS 颜色；0 值返回底色 `emptyColor`。
 */
export function buildColorMap(
  counts: Map<string, number>,
  emptyColor = "#f1f5f9",
  minT = 0.12,
): Map<string, string> {
  const bounds = computeBounds([...counts.values()]);
  const out = new Map<string, string>();
  for (const [key, count] of counts) {
    if (count <= 0) {
      out.set(key, emptyColor);
      continue;
    }
    // minT：有数据的键至少给一点可见色，避免"刚有几次"与"零"无法区分
    const t = Math.max(minT, normalize(count, bounds));
    out.set(key, rgbToCss(rampColor(t)));
  }
  return out;
}

export const RAMPS = { KEY_RAMP, HEAT_RAMP };
