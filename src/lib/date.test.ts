import { describe, expect, it } from "vitest";
import {
  addDays,
  daysBetween,
  endOfMonth,
  enumerateDays,
  monthDays,
  monthOf,
  presetRange,
  startOfMonth,
  startOfWeek,
  startOfYear,
  endOfYear,
  trendGranularity,
} from "./date";

describe("presetRange", () => {
  it("日 = 当天", () => {
    expect(presetRange("day", "2026-09-21")).toEqual({
      start_date: "2026-09-21",
      end_date: "2026-09-21",
    });
  });

  it("周 = 周一至周日（含周日锚点回退到本周一）", () => {
    // 2026-09-21 是周一
    expect(presetRange("week", "2026-09-21")).toEqual({
      start_date: "2026-09-21",
      end_date: "2026-09-27",
    });
    // 2026-09-27 是周日 → 应回退到 09-21
    expect(presetRange("week", "2026-09-27")).toEqual({
      start_date: "2026-09-21",
      end_date: "2026-09-27",
    });
    // 2026-09-28 是下周一
    expect(startOfWeek("2026-09-28")).toBe("2026-09-28");
  });

  it("月 = 当月 1 日至月末", () => {
    expect(presetRange("month", "2026-09-21")).toEqual({
      start_date: "2026-09-01",
      end_date: "2026-09-30",
    });
    expect(presetRange("month", "2024-02-10")).toEqual({
      start_date: "2024-02-01",
      end_date: "2024-02-29", // 闰年
    });
    expect(presetRange("month", "2025-02-10")).toEqual({
      start_date: "2025-02-01",
      end_date: "2025-02-28",
    });
  });

  it("年 = 自然年 1 月 1 日至 12 月 31 日", () => {
    expect(presetRange("year", "2024-02-10")).toEqual({
      start_date: "2024-01-01",
      end_date: "2024-12-31",
    });
    expect(startOfYear("2026-09-21")).toBe("2026-01-01");
    expect(endOfYear("2026-09-21")).toBe("2026-12-31");
  });
});

describe("日期运算边界", () => {
  it("跨月/跨年加减", () => {
    expect(addDays("2026-01-31", 1)).toBe("2026-02-01");
    expect(addDays("2026-12-31", 1)).toBe("2027-01-01");
    expect(addDays("2026-03-01", -1)).toBe("2026-02-28");
    expect(addDays("2024-03-01", -1)).toBe("2024-02-29"); // 闰年
  });

  it("月末与月初", () => {
    expect(endOfMonth("2026-09-15")).toBe("2026-09-30");
    expect(startOfMonth("2026-09-15")).toBe("2026-09-01");
    expect(monthOf("2026-09-15")).toBe("2026-09");
  });

  it("daysBetween 为闭区间天数", () => {
    expect(daysBetween("2026-09-21", "2026-09-21")).toBe(1);
    expect(daysBetween("2026-09-01", "2026-09-30")).toBe(30);
    expect(daysBetween("2026-01-01", "2026-12-31")).toBe(365);
    expect(daysBetween("2024-01-01", "2024-12-31")).toBe(366); // 闰年
  });

  it("enumerateDays 覆盖闭区间并处理倒序输入", () => {
    expect(enumerateDays("2026-09-29", "2026-10-02")).toEqual([
      "2026-09-29",
      "2026-09-30",
      "2026-10-01",
      "2026-10-02",
    ]);
    expect(enumerateDays("2026-10-02", "2026-09-29")).toEqual([]);
  });

  it("monthDays 返回整月日期（闰年 2 月 29 天）", () => {
    expect(monthDays("2026-09")).toHaveLength(30);
    expect(monthDays("2026-09")[0]).toBe("2026-09-01");
    expect(monthDays("2026-09")[29]).toBe("2026-09-30");
    expect(monthDays("2024-02")).toHaveLength(29);
  });

  it("趋势粒度按预设范围映射，自定义范围按长度降级", () => {
    expect(trendGranularity("day", "2026-09-21", "2026-09-21")).toBe("hour");
    expect(trendGranularity("week", "2026-09-21", "2026-09-27")).toBe("day");
    expect(trendGranularity("month", "2026-09-01", "2026-09-30")).toBe("day");
    expect(trendGranularity("year", "2026-01-01", "2026-12-31")).toBe("month");
    expect(trendGranularity("custom", "2026-09-21", "2026-09-21")).toBe("hour");
    expect(trendGranularity("custom", "2026-09-01", "2026-10-31")).toBe("day");
    expect(trendGranularity("custom", "2026-01-01", "2026-12-31")).toBe("month");
  });
});
