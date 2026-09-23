import { describe, expect, it } from "vitest";
import {
  heatmapColorPosition,
  heatmapKernelRadius,
  heatmapPeakAlpha,
  normalizeHeatmapCount,
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
