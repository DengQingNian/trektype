/**
 * 屏幕热成像渲染参数。
 *
 * 这些函数保持纯函数，Canvas 组件只负责把它们应用到像素层，便于验证
 * 热核半径、透明度和色带映射在极端数据下仍然稳定。
 */

import type { GridCell, MonitorRow } from "./ipc";

/** 只保留当前查询范围内有点击数据的显示器。 */
export function activeHeatmapMonitors(monitors: MonitorRow[], cells: GridCell[]): MonitorRow[] {
  const ids = new Set(cells.filter((cell) => cell.count > 0).map((cell) => cell.monitor_id));
  return monitors.filter((monitor) => ids.has(monitor.id));
}

export interface HeatmapMonitorTile {
  monitor: MonitorRow;
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
}

/** 全部视图分屏排布，避免历史记录中坐标相同的显示器相互叠画。 */
export function layoutHeatmapMonitors(monitors: MonitorRow[], mode: "all" | number, maxWidth = 900) {
  const visible = mode === "all" ? monitors : monitors.filter((monitor) => monitor.id === mode);
  const columns = mode === "all" && visible.length > 1 ? 2 : 1;
  const gap = 20;
  const tileWidth = (maxWidth - gap * (columns - 1)) / columns;
  const items: HeatmapMonitorTile[] = [];
  let y = 0;
  for (let index = 0; index < visible.length; index += columns) {
    const row = visible.slice(index, index + columns).map((monitor, column) => {
      const scale = Math.min(1, tileWidth / Math.max(1, monitor.width));
      return {
        monitor,
        x: column * (tileWidth + gap),
        y,
        width: monitor.width * scale,
        height: monitor.height * scale,
        scale,
      };
    });
    items.push(...row);
    y += Math.max(...row.map((item) => item.height)) + gap;
  }
  return { width: maxWidth, height: Math.max(1, y - (items.length ? gap : 0)), items };
}

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

/**
 * 将一个网格的点击次数按当前显示器的最高点击网格归一化。
 *
 * 屏幕之间不共用峰值，避免某台屏幕的高频点击把其他屏幕整体压暗；
 * 同时不使用分位裁剪，让最高点击位置稳定对应到 1。
 */
export function normalizeHeatmapCount(count: number, peakCount: number): number {
  if (!Number.isFinite(count) || count <= 0 || !Number.isFinite(peakCount) || peakCount <= 0) return 0;
  return clamp(count / peakCount);
}

/** 把热核叠加后的 alpha 映射回色带位置。 */
export function heatmapColorPosition(alpha: number, maxAlpha = 1): number {
  if (!Number.isFinite(maxAlpha) || maxAlpha <= 0) return 0;
  return clamp(alpha / maxAlpha);
}
