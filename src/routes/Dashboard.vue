<script setup lang="ts">
/** 概览：今日三指标 + 近 7 天趋势 + 24 小时分布 + 运行状态。 */
import { NAlert, NCard, NGrid, NGi, NSpin, NStatistic, NTag } from "naive-ui";
import { computed, onMounted, ref, watch } from "vue";
import type { EChartsOption } from "echarts";
import EChart from "../components/EChart.vue";
import { useRangeStore } from "../stores/range";
import { useRuntimeStore } from "../stores/runtime";
import { api, formatBytes, formatNumber, type KeyboardStats, type Overview } from "../lib/ipc";
import { toastError } from "../lib/ui";

const range = useRangeStore();
const runtime = useRuntimeStore();
const loading = ref(false);
const overview = ref<Overview | null>(null);
const keyboard = ref<KeyboardStats | null>(null);

async function load() {
  loading.value = true;
  try {
    const r = range.range;
    const [ov, kb] = await Promise.all([
      api.getOverview(r.start_date, r.end_date),
      api.getKeyboardStats(r.start_date, r.end_date),
    ]);
    overview.value = ov;
    keyboard.value = kb;
  } catch (e) {
    toastError(e, "加载概览失败");
  } finally {
    loading.value = false;
  }
}

const trendOption = computed<EChartsOption>(() => ({
  tooltip: { trigger: "axis" },
  legend: { data: ["按键", "点击"], top: 0, textStyle: { fontSize: 11 } },
  grid: { left: 44, right: 16, top: 30, bottom: 24 },
  xAxis: {
    type: "category",
    data: (overview.value?.by_day ?? []).map((d) => d.date.slice(5)),
    axisLabel: { fontSize: 10 },
  },
  yAxis: { type: "value", axisLabel: { fontSize: 10 } },
  series: [
    {
      name: "按键",
      type: "line",
      smooth: true,
      areaStyle: { opacity: 0.12 },
      itemStyle: { color: "#2563eb" },
      data: (overview.value?.by_day ?? []).map((d) => d.key_count),
    },
    {
      name: "点击",
      type: "line",
      smooth: true,
      areaStyle: { opacity: 0.12 },
      itemStyle: { color: "#f97316" },
      data: (overview.value?.by_day ?? []).map((d) => d.click_count),
    },
  ],
}));

const hourOption = computed<EChartsOption>(() => ({
  tooltip: { trigger: "axis" },
  grid: { left: 44, right: 16, top: 20, bottom: 24 },
  xAxis: {
    type: "category",
    data: Array.from({ length: 24 }, (_, i) => `${i}`),
    axisLabel: { fontSize: 10 },
  },
  yAxis: { type: "value", axisLabel: { fontSize: 10 } },
  series: [
    {
      name: "按键",
      type: "bar",
      itemStyle: { color: "#60a5fa" },
      data: keyboard.value?.by_hour ?? [],
    },
  ],
}));

onMounted(load);
watch(() => range.range, load, { deep: true });
</script>

<template>
  <div class="page">
    <div class="head">
      <h2>概览</h2>
      <span class="range-hint">{{ range.range.start_date }} ~ {{ range.range.end_date }}</span>
    </div>

    <n-alert v-if="runtime.state && !runtime.state.writer_ready" type="warning" class="mb">
      写入器未就绪：数据可能未落库（请检查数据目录权限）。
    </n-alert>
    <n-alert v-if="runtime.state?.paused" type="info" class="mb">
      采集已暂停，当前不会记录新事件。点击右上角"恢复采集"继续。
    </n-alert>

    <n-spin :show="loading">
      <n-grid :cols="3" :x-gap="12" class="mb">
        <n-gi>
          <n-card size="small">
            <n-statistic label="按键次数" :value="formatNumber(overview?.key_total ?? 0)" />
          </n-card>
        </n-gi>
        <n-gi>
          <n-card size="small">
            <n-statistic label="鼠标点击" :value="formatNumber(overview?.click_total ?? 0)" />
          </n-card>
        </n-gi>
        <n-gi>
          <n-card size="small">
            <n-statistic label="活跃小时（估算）" :value="`${overview?.active_hours ?? 0} 小时`" />
          </n-card>
        </n-gi>
      </n-grid>

      <n-card size="small" title="趋势" class="mb">
        <EChart :option="trendOption" height="240px" />
      </n-card>

      <n-card size="small" title="24 小时分布（按键）">
        <EChart :option="hourOption" height="200px" />
      </n-card>
    </n-spin>

    <n-card v-if="runtime.state" size="small" title="运行状态" class="mt">
      <div class="runtime-grid">
        <span>队列容量：{{ formatNumber(runtime.state.queue_capacity) }}</span>
        <span>已落库事件：{{ formatNumber(runtime.state.flushed_events) }}</span>
        <span>丢弃事件：{{ formatNumber(runtime.state.dropped_total) }}</span>
        <span>黑名单拦截：{{ formatNumber(runtime.state.blocked_total) }}</span>
        <span>写入批次：{{ formatNumber(runtime.state.flush_count) }}</span>
        <span>最近批次耗时：{{ runtime.state.last_flush_ms }} ms</span>
        <span>写入错误：{{ formatNumber(runtime.state.write_errors) }}</span>
        <span>数据库大小：{{ formatBytes(runtime.state.db_bytes) }}</span>
        <span>显示器：{{ runtime.state.monitor_count }} 台</span>
        <span>
          钩子：
          <n-tag size="tiny" :type="runtime.state.keyboard_installed ? 'success' : 'error'">键盘</n-tag>
          <n-tag size="tiny" :type="runtime.state.mouse_installed ? 'success' : 'error'" class="ml4">鼠标</n-tag>
        </span>
      </div>
      <div v-if="runtime.state.current_exe" class="current-app">
        当前前台应用：<b>{{ runtime.state.current_exe }}</b>
      </div>
    </n-card>
  </div>
</template>

<style scoped>
.page {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.head {
  display: flex;
  align-items: baseline;
  gap: 12px;
}
h2 {
  margin: 0;
  font-size: 18px;
}
.range-hint {
  font-size: 12px;
  color: #64748b;
}
.mb {
  margin-bottom: 12px;
}
.mt {
  margin-top: 12px;
}
.runtime-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 6px 16px;
  font-size: 12.5px;
  color: #475569;
}
.current-app {
  margin-top: 8px;
  font-size: 12.5px;
  color: #475569;
}
.ml4 {
  margin-left: 4px;
}
</style>
