import { describe, expect, it } from "vitest";
import {
  buildColorMap,
  computeBounds,
  normalize,
  percentile,
  rampColor,
  rgbToCss,
  RAMPS,
} from "./colorscale";

describe("rampColor", () => {
  it("t=0 / t=1 取两端颜色", () => {
    expect(rampColor(0)).toEqual(RAMPS.KEY_RAMP[0]);
    expect(rampColor(1)).toEqual(RAMPS.KEY_RAMP[RAMPS.KEY_RAMP.length - 1]);
  });

  it("越界与 NaN 被钳制，不产生非法颜色", () => {
    expect(rampColor(-5)).toEqual(RAMPS.KEY_RAMP[0]);
    expect(rampColor(99)).toEqual(RAMPS.KEY_RAMP[RAMPS.KEY_RAMP.length - 1]);
    expect(rampColor(NaN)).toEqual(RAMPS.KEY_RAMP[0]);
  });

  it("热力带使用独立色板（蓝→红）", () => {
    const lo = rampColor(0, RAMPS.HEAT_RAMP);
    const hi = rampColor(1, RAMPS.HEAT_RAMP);
    expect(lo[2]).toBeGreaterThan(lo[0]); // 蓝多于红
    expect(hi[0]).toBeGreaterThan(hi[2]); // 红多于蓝
  });
});

describe("percentile", () => {
  it("空数组返回 0，不抛错", () => {
    expect(percentile([], 0.5)).toBe(0);
  });

  it("中位数与端点", () => {
    expect(percentile([1, 2, 3, 4, 5], 0.5)).toBe(3);
    expect(percentile([1, 2, 3, 4, 5], 0)).toBe(1);
    expect(percentile([1, 2, 3, 4, 5], 1)).toBe(5);
    // 线性插值：位置 2.5 → 3 与 4 的中点
    expect(percentile([1, 2, 3, 4], 0.8333)).toBeCloseTo(3.5, 1);
  });
});

describe("computeBounds / normalize", () => {
  it("空数组或全零 → 无有效边界，归一化恒为 0", () => {
    expect(computeBounds([])).toEqual({ lo: 0, hi: 0 });
    const b = computeBounds([0, 0, 0]);
    expect(normalize(0, b)).toBe(0);
    expect(normalize(5, b)).toBe(1); // 有值一律最高热（避免除以 0）
  });

  it("单值：该值归一化为 1", () => {
    const b = computeBounds([42]);
    expect(normalize(42, b)).toBe(1);
    expect(normalize(0, b)).toBe(0);
  });

  it("极值不淹没主体：log+P99 后最大值仍为 1、中位数落在中间区间", () => {
    // 模拟真实分布：多数键 50 次上下，空格 5000 次
    const counts = [10, 30, 45, 50, 52, 60, 80, 100, 150, 5000];
    const b = computeBounds(counts);
    expect(normalize(5000, b)).toBe(1);
    const mid = normalize(50, b);
    expect(mid).toBeGreaterThan(0.1);
    expect(mid).toBeLessThan(0.9);
  });

  it("负值与零值一样视为无热度", () => {
    const b = computeBounds([1, 2, 3]);
    expect(normalize(-3, b)).toBe(0);
  });
});

describe("buildColorMap", () => {
  it("零值使用底色，有值至少给最小可见度", () => {
    const map = buildColorMap(new Map([["KeyA", 0], ["KeyB", 1], ["KeyC", 1000]]));
    expect(map.get("KeyA")).toBe("#f1f5f9");
    expect(map.get("KeyB")).not.toBe("#f1f5f9");
    expect(map.get("KeyB")).not.toBe(map.get("KeyC"));
  });

  it("全零数据不产生异常颜色", () => {
    const map = buildColorMap(new Map([["KeyA", 0], ["KeyB", 0]]));
    expect([...map.values()].every((c) => c === "#f1f5f9")).toBe(true);
  });

  it("rgbToCss 格式合法", () => {
    expect(rgbToCss([1, 2, 3])).toBe("rgb(1, 2, 3)");
  });
});
