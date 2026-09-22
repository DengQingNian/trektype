import { describe, expect, it } from "vitest";
import { heatmapColorPosition, heatmapKernelRadius, heatmapPeakAlpha } from "./screen-heatmap";

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
    expect(heatmapColorPosition(0.38)).toBeCloseTo(0.5);
    expect(heatmapColorPosition(10)).toBe(1);
    expect(heatmapColorPosition(1, 0)).toBe(0);
  });
});
