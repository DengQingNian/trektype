/**
 * 屏幕热成像渲染参数。
 *
 * 这些函数保持纯函数，Canvas 组件只负责把它们应用到像素层，便于验证
 * 热核半径、透明度和色带映射在极端数据下仍然稳定。
 */

function clamp(value: number, min = 0, max = 1): number {
  if (!Number.isFinite(value)) return min;
  return Math.min(max, Math.max(min, value));
}

/** 根据网格大小和画布缩放比例计算热核半径（CSS px）。 */
export function heatmapKernelRadius(cellSize: number, scale: number): number {
  const safeCellSize = Number.isFinite(cellSize) && cellSize > 0 ? cellSize : 24;
  const safeScale = Number.isFinite(scale) && scale > 0 ? scale : 1;
  // 让相邻网格的热核有明显重叠，视觉上不会再出现方块边界。
  return Math.max(18, Math.min(96, safeCellSize * safeScale * 3.4));
}

/** 把归一化热度转换为热核中心透明度。低频点击仍保留可辨识的蓝色。 */
export function heatmapPeakAlpha(normalizedHeat: number): number {
  return Math.min(0.76, 0.06 + clamp(normalizedHeat) * 0.7);
}

/** 把热核叠加后的 alpha 映射回色带位置。 */
export function heatmapColorPosition(alpha: number, maxAlpha = heatmapPeakAlpha(1)): number {
  if (!Number.isFinite(maxAlpha) || maxAlpha <= 0) return 0;
  return clamp(alpha / maxAlpha);
}
