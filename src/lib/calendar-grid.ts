import { addDays, endOfMonth, parseDate } from "./date";
import { normalize, rampColor, rgbToCss, type HeatmapRamp, type ScaleBounds } from "./colorscale";

export interface CalendarGridCell {
  date: string;
  day: number;
  inMonth: boolean;
}

/** 把自然月补齐到完整的周，周一为首列。 */
export function monthGrid(month: string): CalendarGridCell[] {
  const first = `${month}-01`;
  const leading = (parseDate(first).getDay() + 6) % 7;
  const last = endOfMonth(first);
  const days = Math.ceil((leading + parseDate(last).getDate()) / 7) * 7;
  const start = addDays(first, -leading);
  return Array.from({ length: days }, (_, index) => {
    const date = addDays(start, index);
    return { date, day: parseDate(date).getDate(), inMonth: date.startsWith(`${month}-`) };
  });
}

/** 活跃日期按色阶着色，文字颜色由实际背景亮度决定。 */
export function calendarDayStyle(count: number, bounds: ScaleBounds, ramp: HeatmapRamp) {
  if (count <= 0) return {};
  const color = rampColor(Math.max(0.1, normalize(count, bounds)), ramp);
  const brightness = (color[0] * 299 + color[1] * 587 + color[2] * 114) / 1000;
  return { backgroundColor: rgbToCss(color), color: brightness > 145 ? "#26323b" : "#fff" };
}
