<script setup lang="ts">
/** 时间范围选择：日/周/月/年/自定义，写入全局 range store（所有统计页联动）。 */
import { NDatePicker, NRadioButton, NRadioGroup } from "naive-ui";
import { Calendar } from "@vicons/carbon";
import { computed } from "vue";
import { useRangeStore } from "../stores/range";
import { parseDate, toDateString, type RangeKind } from "../lib/date";

const range = useRangeStore();

const customValue = computed({
  get: (): [number, number] | null => [
    parseDate(range.range.start_date).getTime(),
    parseDate(range.range.end_date).getTime(),
  ],
  set: (v: [number, number] | null) => {
    if (Array.isArray(v) && v[0] != null && v[1] != null) {
      range.setCustom({
        start_date: toDateString(new Date(v[0])),
        end_date: toDateString(new Date(v[1])),
      });
    }
  },
});

function onPreset(v: string | number) {
  if (v === "day" || v === "week" || v === "month" || v === "year") {
    range.setPreset(v as Exclude<RangeKind, "custom">);
  }
}
</script>

<template>
  <div class="range-picker">
    <span class="range-label">VIEW</span>
    <n-radio-group :value="range.kind" size="small" @update:value="onPreset">
      <n-radio-button value="day">今日</n-radio-button>
      <n-radio-button value="week">本周</n-radio-button>
      <n-radio-button value="month">本月</n-radio-button>
      <n-radio-button value="year">本年</n-radio-button>
    </n-radio-group>
    <Calendar class="field-icon" aria-hidden="true" />
    <n-date-picker
      v-model:value="customValue"
      type="daterange"
      size="small"
      :clearable="false"
      style="width: 240px"
    />
    <span class="hint">{{ range.range.start_date }} ~ {{ range.range.end_date }}</span>
  </div>
</template>

<style scoped>
.range-picker {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  padding: 6px 8px;
  border: 1px dashed var(--line);
  background: rgba(232, 226, 216, 0.45);
  box-shadow: 1px 1px 0 rgba(44, 44, 44, 0.16);
}
.range-label {
  color: var(--rust);
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.12em;
}
.hint {
  font-size: 12px;
  color: var(--ink-soft);
}
</style>
