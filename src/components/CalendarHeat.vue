<script setup lang="ts">
/** 月历活跃度：完整周网格，键盘/鼠标切换，点击日期查看详情。 */
import { computed } from "vue";
import { NRadioGroup, NRadioButton } from "naive-ui";
import { Cursor2, Keyboard } from "@vicons/carbon";
import { calendarDayStyle, monthGrid } from "../lib/calendar-grid";
import { todayLocal } from "../lib/date";
import { computeBounds, getHeatmapRamp, rgbToCss, type HeatmapPalette } from "../lib/colorscale";
import type { DayCount } from "../lib/ipc";

const props = defineProps<{
  month: string;
  days: DayCount[];
  metric: "key" | "click";
  palette: HeatmapPalette;
  selected?: string | null;
}>();
const emit = defineEmits<{ (e: "select", date: string): void; (e: "update:metric", v: "key" | "click"): void }>();

const weekdays = ["一", "二", "三", "四", "五", "六", "日"];
const today = todayLocal();
const counts = computed(() => new Map(props.days.map((day) => [day.date, props.metric === "key" ? day.key_count : day.click_count])));
const cells = computed(() => monthGrid(props.month).map((cell) => ({ ...cell, count: counts.value.get(cell.date) ?? 0 })));
const bounds = computed(() => computeBounds(cells.value.filter((cell) => cell.inMonth).map((cell) => cell.count)));
const maxValue = computed(() => Math.max(0, ...cells.value.filter((cell) => cell.inMonth).map((cell) => cell.count)));
const paletteColors = computed(() => getHeatmapRamp(props.palette).map(rgbToCss));
const ramp = computed(() => getHeatmapRamp(props.palette));

</script>

<template>
  <div class="cal-wrap">
    <div class="cal-tools">
      <n-radio-group
        :value="props.metric"
        size="small"
        @update:value="(v: string | number) => emit('update:metric', v as 'key' | 'click')"
      >
        <n-radio-button value="key"><Keyboard class="ui-icon button-icon" />键盘</n-radio-button>
        <n-radio-button value="click"><Cursor2 class="ui-icon button-icon" />鼠标</n-radio-button>
      </n-radio-group>
      <span class="hint">点击日期查看当天详情</span>
    </div>
    <div class="calendar-grid" :aria-label="`${props.month} 活跃度日历`">
      <div v-for="day in weekdays" :key="day" class="weekday">{{ day }}</div>
      <button
        v-for="cell in cells"
        :key="cell.date"
        type="button"
        class="day"
        :class="{ outside: !cell.inMonth, selected: props.selected === cell.date, today: today === cell.date, active: cell.count > 0 }"
        :style="cell.inMonth ? calendarDayStyle(cell.count, bounds, ramp) : undefined"
        :disabled="!cell.inMonth"
        :aria-label="`${cell.date}，${props.metric === 'key' ? '按键' : '点击'} ${cell.count.toLocaleString()} 次`"
        :aria-pressed="props.selected === cell.date"
        @click="emit('select', cell.date)"
      >
        <span class="day-number">{{ cell.day }}</span>
        <span v-if="cell.inMonth && cell.count > 0" class="day-count">{{ cell.count.toLocaleString() }}</span>
      </button>
    </div>
    <div class="legend">
      <span>少</span>
      <span class="legend-bar"><i v-for="color in paletteColors" :key="color" :style="{ backgroundColor: color }" /></span>
      <span>多</span>
      <span class="max">当月单日最高 {{ maxValue.toLocaleString() }} 次</span>
    </div>
  </div>
</template>

<style scoped>
.cal-wrap { display: flex; flex-direction: column; gap: 14px; }
.cal-tools { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
.hint, .legend { color: var(--ink-soft); font-size: 12px; }
.calendar-grid { display: grid; grid-template-columns: repeat(7, minmax(0, 1fr)); gap: 6px; width: 100%; max-width: 1080px; margin-inline: auto; }
.weekday { text-align: center; padding: 3px 0 8px; color: var(--ink-soft); font-size: 12px; font-weight: 700; }
.day { min-width: 0; height: 78px; display: flex; flex-direction: column; justify-content: space-between; align-items: flex-start; padding: 9px 10px; border: 1px solid rgba(44, 44, 44, .12); border-radius: 3px; background: var(--paper-light); color: var(--ink); text-align: left; cursor: pointer; transition: transform 130ms var(--ease-sketch), box-shadow 130ms ease; }
.day:hover:not(:disabled) { transform: translateY(-2px); box-shadow: 2px 2px 0 rgba(44, 44, 44, .22); }
.day:focus-visible { outline: 2px solid var(--ink); outline-offset: 2px; }
.day.outside { opacity: .45; background: transparent; border-color: transparent; cursor: default; }
.day.selected { outline: 2px solid var(--ink); outline-offset: 1px; }
.day.today .day-number { text-decoration: underline; text-decoration-thickness: 2px; text-underline-offset: 4px; }
.day-number { font-size: 13px; font-weight: 700; }
.day-count { font-size: 11px; font-variant-numeric: tabular-nums; font-weight: 700; }
.legend { display: flex; align-items: center; gap: 6px; width: 100%; max-width: 1080px; margin-inline: auto; }
.legend-bar { width: 110px; height: 9px; display: flex; overflow: hidden; border-radius: 2px; }
.legend-bar i { flex: 1; }
.max { margin-left: 8px; }
@media (max-width: 700px) { .calendar-grid { gap: 3px; } .day { height: 64px; padding: 6px; } .day-count { font-size: 9px; } }
</style>
