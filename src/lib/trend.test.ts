import { describe, expect, it } from "vitest";
import { completeTrend, enumerateTrendBuckets, summarizeTrend, trendLabel, type TrendPoint } from "./trend";

describe("趋势序列", () => {
  it("当天生成 24 个小时桶并补齐空小时", () => {
    const points: TrendPoint[] = [{ bucket: "2026-09-21T10", key_count: 3, click_count: 2 }];
    const completed = completeTrend(points, "2026-09-21", "2026-09-21", "hour");

    expect(completed).toHaveLength(24);
    expect(completed[0]).toEqual({ bucket: "2026-09-21T00", key_count: 0, click_count: 0 });
    expect(completed[10]).toEqual(points[0]);
    expect(trendLabel(completed[10].bucket, "hour")).toBe("10:00");
  });

  it("按天和按月生成连续桶，跨月不丢失", () => {
    expect(enumerateTrendBuckets("2026-01-30", "2026-02-02", "day")).toEqual([
      "2026-01-30",
      "2026-01-31",
      "2026-02-01",
      "2026-02-02",
    ]);
    expect(enumerateTrendBuckets("2025-12-01", "2026-02-28", "month")).toEqual([
      "2025-12",
      "2026-01",
      "2026-02",
    ]);
    expect(trendLabel("2026-02", "month")).toBe("2026/02");
  });

  it("趋势洞察返回总量、峰值和活跃桶数", () => {
    const summary = summarizeTrend([
      { bucket: "a", key_count: 2, click_count: 1 },
      { bucket: "b", key_count: 5, click_count: 2 },
      { bucket: "c", key_count: 0, click_count: 0 },
    ]);
    expect(summary).toEqual({
      total: 10,
      activeBuckets: 2,
      peak: { bucket: "b", key_count: 5, click_count: 2 },
    });
  });
});
