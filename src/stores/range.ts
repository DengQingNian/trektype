import { defineStore } from "pinia";
import { ref } from "vue";
import { presetRange, todayLocal, type DateRange, type RangeKind } from "../lib/date";

/** 全局时间范围：各统计页共享（切换范围后所有页面联动）。 */
export const useRangeStore = defineStore("range", () => {
  const kind = ref<RangeKind>("day");
  const range = ref<DateRange>(presetRange("day"));

  function setPreset(k: Exclude<RangeKind, "custom">) {
    kind.value = k;
    range.value = presetRange(k, todayLocal());
  }

  function setCustom(next: DateRange) {
    kind.value = "custom";
    range.value = next;
  }

  return { kind, range, setPreset, setCustom };
});
