import { describe, expect, it } from "vitest";
import { calendarDayStyle, monthGrid } from "./calendar-grid";
import { computeBounds, getHeatmapRamp } from "./colorscale";

describe("monthGrid", () => {
  it("以周一开头补齐闰年二月，并标记月外日期", () => {
    const grid = monthGrid("2024-02");
    expect(grid).toHaveLength(35);
    expect(grid[0]).toEqual({ date: "2024-01-29", day: 29, inMonth: false });
    expect(grid[3]).toEqual({ date: "2024-02-01", day: 1, inMonth: true });
    expect(grid[31]).toEqual({ date: "2024-02-29", day: 29, inMonth: true });
    expect(grid[34]).toEqual({ date: "2024-03-03", day: 3, inMonth: false });
  });

  it("跨年月份完整占据五周且没有重复日期", () => {
    const grid = monthGrid("2023-12");
    expect(grid).toHaveLength(35);
    expect(grid[0].date).toBe("2023-11-27");
    expect(grid[grid.length - 1]?.date).toBe("2023-12-31");
    expect(new Set(grid.map((cell) => cell.date)).size).toBe(grid.length);
  });
});

describe("calendarDayStyle", () => {
  it("空白日期不着色，浅色热点使用深字，深色热点使用白字", () => {
    const bounds = computeBounds([1, 100]);
    const ramp = getHeatmapRamp("ocean");
    expect(calendarDayStyle(0, bounds, ramp)).toEqual({});
    expect(calendarDayStyle(1, bounds, ramp).color).toBe("#26323b");
    expect(calendarDayStyle(100, bounds, ramp).color).toBe("#fff");
  });
});
