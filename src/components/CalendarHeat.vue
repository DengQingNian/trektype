<script setup lang="ts">
/**
 * 日历热力图（ECharts calendar 系列）。
 * 键盘/鼠标两个序列切换；颜色深浅 = 活跃度（对数 + 分位裁剪后的色阶）。
 * 点击某天 → 触发 select 事件，由父页面展示单日详情。
 */
import { computed } from "vue";
import { NRadioGroup, NRadioButton } from "naive-ui";
import type { EChartsOption } from "echarts";
import EChart from "./EChart.vue";
import { monthDays } from "../lib/date";
import { computeBounds, normalize, rampColor, rgbToCss } from "../lib/colorscale";
import type { DayCount } from "../lib/ipc";

const props = defineProps<{
  month: string;
  days: DayCount[];
  metric: "key" | "click";
}>();
const emit = defineEmits<{ (e: "select", date: string): void; (e: "update:metric", v: "key" | "click"): void }>();

/** 补齐整月（无数据日显示为 0，颜色为空白）。 */
const series = computed(() => {
  const map = new Map(props.days.map((d) => [d.date, d]));
  return monthDays(props.month).map((date) => {
    const d = map.get(date);
    const value = d ? (props.metric === "key" ? d.key_count : d.click_count) : 0;
    return [date, value] as [string, number];
  });
});

const maxValue = computed(() => Math.max(0, ...series.value.map(([, v]) => v)));

const option = computed<EChartsOption>(() => {
  const values = series.value.map(([, v]) => v);
  const bounds = computeBounds(values);
  const cmap = (count: number) => {
    if (count <= 0) return "#f1f5f9";
    return rgbToCss(rampColor(Math.max(0.10, normalize(count, bounds))));
  };
  const textColor = (count: number) => (normalize(count, bounds) > 0.55 ? "#ffffff" : "#334155");

  // 说明：ECharts 的 TS 类型把 label.color 限定为 string，但运行时支持回调函数
  // （用于"深底白字/浅底深字"），因此此处用断言保留函数式配色。
  return {
    tooltip: {
      formatter: (p: unknown) => {
        const item = p as { data: [string, number] };
        const [date, count] = item.data;
        const label = props.metric === "key" ? "按键" : "点击";
        return `${date}<br/>${label}：<b>${count.toLocaleString()}</b> 次`;
      },
    },
    visualMap: { show: false, min: 0, max: Math.max(1, maxValue.value) },
    calendar: {
      top: 40,
      left: 40,
      right: 20,
      bottom: 10,
      range: props.month,
      cellSize: ["auto", 20],
      splitLine: { show: false },
      itemStyle: { borderWidth: 2, borderColor: "#fff" },
      dayLabel: { nameMap: ["日", "一", "二", "三", "四", "五", "六"], color: "#64748b", fontSize: 11 },
      monthLabel: { show: false },
      yearLabel: { show: false },
    },
    series: [
      {
        type: "heatmap",
        coordinateSystem: "calendar",
        data: series.value,
        itemStyle: {
          color: (p: unknown) => {
            const params = p as { data: [string, number] };
            return cmap(params.data[1]);
          },
        },
        label: {
          show: true,
          formatter: (p: unknown) => {
            const params = p as { data: [string, number] };
            const [, count] = params.data;
            return count > 0 ? count.toLocaleString() : "";
          },
          fontSize: 9,
          color: (p: unknown) => {
            const params = p as { data: [string, number] };
            return textColor(params.data[1]);
          },
        },
        emphasis: { itemStyle: { shadowBlur: 6, shadowColor: "rgba(0,0,0,0.3)" } },
      },
    ],
  } as unknown as EChartsOption;
});

function onChartClick(params: unknown) {
  const p = params as { componentType?: string; value?: unknown };
  if (p.componentType === "series" && Array.isArray(p.value)) {
    emit("select", String(p.value[0]));
  }
}
</script>

<template>
  <div class="cal-wrap">
    <div class="cal-tools">
      <n-radio-group
        :value="props.metric"
        size="small"
        @update:value="(v: string | number) => emit('update:metric', v as 'key' | 'click')"
      >
        <n-radio-button value="key">键盘</n-radio-button>
        <n-radio-button value="click">鼠标</n-radio-button>
      </n-radio-group>
      <span class="hint">色深 = 当日活跃度（对数色阶）；点击日期查看详情</span>
    </div>
    <EChart :option="option" height="300px" @click="onChartClick" />
    <div class="legend">
      <span class="legend-label">少</span>
      <span class="legend-bar" />
      <span class="legend-label">多</span>
      <span class="max">当月单日最高：{{ maxValue.toLocaleString() }}</span>
    </div>
  </div>
</template>

<style scoped>
.cal-wrap {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.cal-tools {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.hint {
  font-size: 12px;
  color: #64748b;
}
.legend {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: #64748b;
}
.legend-bar {
  width: 110px;
  height: 10px;
  border-radius: 5px;
  background: linear-gradient(90deg, #e8f0fe, #93c5fd, #3b82f6, #1d4ed8, #172554);
}
.max {
  margin-left: 12px;
}
</style>
