import { describe, expect, it } from "vitest";
import {
  heatmapColorPosition,
  heatmapKernelRadius,
  heatmapPeakAlpha,
  normalizeHeatmapCount,
  layoutHeatmapMonitors,
  activeHeatmapMonitors,
  projectHeatmapToCurrentMonitors,
} from "./screen-heatmap";

describe("screen heatmap rendering parameters", () => {
  it("热核半径会覆盖多个网格，且保持在可控范围内", () => {
    expect(heatmapKernelRadius(24, 1)).toBeGreaterThan(24);
    expect(heatmapKernelRadius(24, 0.5)).toBeGreaterThan(18);
    expect(heatmapKernelRadius(100, 2)).toBe(96);
  });

  it("热度越高中心透明度越高，并限制在稳定范围", () => {
    expect(heatmapPeakAlpha(0)).toBeCloseTo(0.06);
    expect(heatmapPeakAlpha(0.5)).toBeGreaterThan(heatmapPeakAlpha(0.1));
    expect(heatmapPeakAlpha(99)).toBeCloseTo(0.76);
    expect(heatmapPeakAlpha(Number.NaN)).toBeCloseTo(0.06);
  });

  it("叠加 alpha 映射到色带时会安全钳制", () => {
    expect(heatmapColorPosition(0)).toBe(0);
    expect(heatmapColorPosition(0.38)).toBeCloseTo(0.38);
    expect(heatmapColorPosition(10)).toBe(1);
    expect(heatmapColorPosition(1, 0)).toBe(0);
  });

  it("按当前显示器的最高点击次数归一化，最高点为 1", () => {
    expect(normalizeHeatmapCount(0, 100)).toBe(0);
    expect(normalizeHeatmapCount(25, 100)).toBeCloseTo(0.25);
    expect(normalizeHeatmapCount(100, 100)).toBe(1);
    expect(normalizeHeatmapCount(120, 100)).toBe(1);
  });

  it("最高点击次数无效时不产生异常热度", () => {
    expect(normalizeHeatmapCount(10, 0)).toBe(0);
    expect(normalizeHeatmapCount(10, Number.NaN)).toBe(0);
    expect(normalizeHeatmapCount(Number.NaN, 10)).toBe(0);
  });
});

describe("screen heatmap layout", () => {
  const monitors = [
    { id: 1, device_key: "A", is_primary: true, x: 0, y: 0, width: 1920, height: 1080, scale: 1 },
    { id: 2, device_key: "B", is_primary: false, x: 0, y: 0, width: 1920, height: 1080, scale: 1 },
  ];

  it("同坐标的历史显示器在全部视图中分开排布，不会叠画", () => {
    const layout = layoutHeatmapMonitors(monitors, "all");
    expect(layout.items).toHaveLength(2);
    expect(layout.items[0].x + layout.items[0].width).toBeLessThan(layout.items[1].x);
    expect(layout.items[0].y).toBe(layout.items[1].y);
  });

  it("单屏视图只返回所选显示器并使用完整可用宽度", () => {
    const layout = layoutHeatmapMonitors(monitors, 2);
    expect(layout.items.map((item) => item.monitor.id)).toEqual([2]);
    expect(layout.items[0].width).toBe(900);
  });

  it("当前范围只显示有点击的显示器，忽略历史空屏幕", () => {
    const active = activeHeatmapMonitors(monitors, [{ monitor_id: 2, cell_x: 0, cell_y: 0, count: 3 }]);
    expect(active.map((monitor) => monitor.id)).toEqual([2]);
    expect(activeHeatmapMonitors(monitors, [])).toEqual([]);
  });
});

describe("screen heatmap current display projection", () => {
  const oldPrimary = { id: 1, device_key: "DISPLAY1", is_primary: true, x: 0, y: 0, width: 2560, height: 1440, scale: 1.25 };
  const oldVirtual = { id: 2, device_key: "DISPLAY113", is_primary: true, x: 0, y: 0, width: 1920, height: 1080, scale: 1 };
  const current = { id: 3, device_key: "DISPLAY129", is_primary: true, x: 0, y: 0, width: 1920, height: 1080, scale: 1 };

  it("当前只有一块屏幕时将历史主屏记录投影合并，点击次数不丢失", () => {
    const result = projectHeatmapToCurrentMonitors(
      [oldPrimary, oldVirtual, current],
      [
        { monitor_id: 1, cell_x: 40, cell_y: 20, count: 2 },
        { monitor_id: 2, cell_x: 30, cell_y: 15, count: 3 },
        { monitor_id: 3, cell_x: 30, cell_y: 15, count: 4 },
      ],
      [current],
      24,
    );
    expect(result.monitors).toEqual([current]);
    expect(result.cells).toEqual([{ monitor_id: 3, cell_x: 30, cell_y: 15, count: 9 }]);
  });

  it("当前多屏时优先按设备键匹配，再按主屏归属", () => {
    const secondary = { id: 4, device_key: "DISPLAY2", is_primary: false, x: 1920, y: 0, width: 1280, height: 1024, scale: 1 };
    const result = projectHeatmapToCurrentMonitors(
      [oldVirtual, secondary],
      [
        { monitor_id: 2, cell_x: 1, cell_y: 1, count: 2 },
        { monitor_id: 4, cell_x: 1, cell_y: 1, count: 5 },
      ],
      [current, secondary],
      24,
    );
    expect(result.monitors).toEqual([current, secondary]);
    expect(result.cells.map((cell) => [cell.monitor_id, cell.count])).toEqual([[3, 2], [4, 5]]);
  });
});
