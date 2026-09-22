<script setup lang="ts">
/** 概览：动态粒度趋势 + 24 小时偏好 + 轻量洞察 + 运行状态。 */
import { NAlert, NCard, NSpin, NStatistic, NTag } from "naive-ui";
import { Activity, Calendar, ChartLine, Cursor2, Keyboard, Timer, Warning } from "@vicons/carbon";
import { computed, onMounted, ref, watch } from "vue";
import type { EChartsOption } from "echarts";
import EChart from "../components/EChart.vue";
import RangePicker from "../components/RangePicker.vue";
import { daysBetween, trendGranularity } from "../lib/date";
import { useRangeStore } from "../stores/range";
import { useRuntimeStore } from "../stores/runtime";
import { api, formatBytes, formatNumber, type KeyboardStats, type Overview, type TrendPoint } from "../lib/ipc";
import { completeTrend, summarizeTrend, trendLabel } from "../lib/trend";
import { toastError } from "../lib/ui";

const range = useRangeStore();
const runtime = useRuntimeStore();
const loading = ref(false);
const overview = ref<Overview | null>(null);
const keyboard = ref<KeyboardStats | null>(null);
const trend = ref<TrendPoint[]>([]);

const granularity = computed(() =>
  trendGranularity(range.kind, range.range.start_date, range.range.end_date),
);
const granularityLabel = computed(() => {
  if (granularity.value === "hour") return "当天按小时";
  if (granularity.value === "month") return range.kind === "year" ? "本年按月" : "按月";
  return "按天";
});

async function load() {
  loading.value = true;
  try {
    const r = range.range;
    const [ov, kb, tr] = await Promise.all([
      api.getOverview(r.start_date, r.end_date),
      api.getKeyboardStats(r.start_date, r.end_date),
      api.getActivityTrend(r.start_date, r.end_date, granularity.value),
    ]);
    overview.value = ov;
    keyboard.value = kb;
    trend.value = tr;
  } catch (e) {
    toastError(e, "加载概览失败");
  } finally {
    loading.value = false;
  }
}

const completedTrend = computed(() =>
  completeTrend(
    trend.value,
    range.range.start_date,
    range.range.end_date,
    granularity.value,
  ),
);

const trendOption = computed<EChartsOption>(() => ({
  tooltip: { trigger: "axis" },
  legend: { data: ["按键", "点击"], top: 0, textStyle: { fontSize: 11 } },
  grid: { left: 44, right: 16, top: 30, bottom: 24 },
  xAxis: {
    type: "category",
    data: completedTrend.value.map((point) => trendLabel(point.bucket, granularity.value)),
    axisLabel: { fontSize: 10 },
  },
  yAxis: { type: "value", axisLabel: { fontSize: 10 } },
  series: [
    {
      name: "按键",
      type: "line",
      smooth: true,
      areaStyle: { opacity: 0.12 },
      itemStyle: { color: "#58758a" },
      data: completedTrend.value.map((point) => point.key_count),
    },
    {
      name: "点击",
      type: "line",
      smooth: true,
      areaStyle: { opacity: 0.12 },
      itemStyle: { color: "#b35f4d" },
      data: completedTrend.value.map((point) => point.click_count),
    },
  ],
}));

const trendSummary = computed(() => summarizeTrend(completedTrend.value));
const peakLabel = computed(() => {
  const peak = trendSummary.value.peak;
  if (!peak || peak.key_count + peak.click_count === 0) return "暂无数据";
  return trendLabel(peak.bucket, granularity.value);
});
const peakCount = computed(() => {
  const peak = trendSummary.value.peak;
  return peak ? peak.key_count + peak.click_count : 0;
});
const peakTitle = computed(() => {
  if (granularity.value === "hour") return "峰值时段";
  if (granularity.value === "day") return "峰值日期";
  return "峰值月份";
});
const averagePerDay = computed(() => {
  const days = daysBetween(range.range.start_date, range.range.end_date);
  return days > 0 ? Math.round(trendSummary.value.total / days) : 0;
});

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
      itemStyle: { color: "#727954" },
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
      <div class="head-copy">
        <span class="eyebrow">WORK RHYTHM / 01</span>
        <h2 class="title-row"><ChartLine class="ui-icon title-icon" />概览</h2>
        <p>把每天的按键与点击，整理成一张可回看的节奏图。</p>
      </div>
      <RangePicker />
    </div>

    <n-alert v-if="runtime.state && !runtime.state.writer_ready" type="warning" class="mb">
      <template #icon><Warning class="ui-icon" /></template>
      写入器未就绪：数据可能未落库（请检查数据目录权限）。
    </n-alert>
    <n-alert v-if="runtime.state?.paused" type="info" class="mb">
      <template #icon><Activity class="ui-icon" /></template>
      采集已暂停，当前不会记录新事件。点击右上角"恢复采集"继续。
    </n-alert>

    <n-spin :show="loading">
      <div class="metric-grid stagger-children mb">
        <div style="--stagger-index: 0">
          <n-card size="small" class="metric-card">
            <div class="stat-card"><Keyboard class="ui-icon metric-icon" /><n-statistic label="按键次数" :value="formatNumber(overview?.key_total ?? 0)" /></div>
          </n-card>
        </div>
        <div style="--stagger-index: 1">
          <n-card size="small" class="metric-card">
            <div class="stat-card"><Cursor2 class="ui-icon metric-icon" /><n-statistic label="鼠标点击" :value="formatNumber(overview?.click_total ?? 0)" /></div>
          </n-card>
        </div>
        <div style="--stagger-index: 2">
          <n-card size="small" class="metric-card">
            <div class="stat-card"><Calendar class="ui-icon metric-icon" /><n-statistic label="活跃天数" :value="`${overview?.day_count ?? 0} 天`" /></div>
          </n-card>
        </div>
        <div style="--stagger-index: 3">
          <n-card size="small" class="metric-card">
            <div class="stat-card"><Timer class="ui-icon metric-icon" /><n-statistic label="活跃小时（估算）" :value="`${overview?.active_hours ?? 0} 小时`" /></div>
          </n-card>
        </div>
      </div>

      <n-card size="small" class="mb">
        <template #header><span class="card-title"><ChartLine class="ui-icon card-title-icon" />趋势 · {{ granularityLabel }}</span></template>
        <EChart :option="trendOption" height="240px" />
      </n-card>

      <div class="insights mb">
        <n-card size="small">
          <span class="insight-label"><Timer class="ui-icon card-title-icon" />{{ peakTitle }}</span>
          <strong>{{ peakLabel }}</strong>
          <small>{{ peakCount ? `${formatNumber(peakCount)} 次事件` : "暂无记录" }}</small>
        </n-card>
        <n-card size="small">
          <span class="insight-label"><Activity class="ui-icon card-title-icon" />日均事件</span>
          <strong>{{ formatNumber(averagePerDay) }}</strong>
          <small>按键 + 点击</small>
        </n-card>
        <n-card size="small">
          <span class="insight-label"><Calendar class="ui-icon card-title-icon" />活跃时间占比</span>
          <strong>{{ `${Math.min(100, Math.round((overview?.active_hours ?? 0) / Math.max(1, daysBetween(range.range.start_date, range.range.end_date) * 24) * 100))}%` }}</strong>
          <small>活跃小时 / 范围小时</small>
        </n-card>
      </div>

      <n-card size="small">
        <template #header><span class="card-title"><Keyboard class="ui-icon card-title-icon" />按键时段偏好（范围累计）</span></template>
        <EChart :option="hourOption" height="200px" />
      </n-card>
    </n-spin>

    <n-card v-if="runtime.state" size="small" class="mt">
      <template #header><span class="card-title"><Activity class="ui-icon card-title-icon" />运行状态</span></template>
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
  align-items: flex-end;
  justify-content: space-between;
  gap: 18px;
  flex-wrap: wrap;
}
.head-copy {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.eyebrow {
  color: var(--rust);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.16em;
}
.head p {
  margin: 0;
  color: var(--ink-soft);
  font-size: 12px;
}
h2 {
  margin: 0;
  color: var(--ink);
  font-family: Georgia, "Times New Roman", "Microsoft YaHei", serif;
  font-size: 28px;
  line-height: 1.1;
}
.metric-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 12px;
}
.metric-grid > * {
  min-width: 0;
}
.mb {
  margin-bottom: 12px;
}
.insights {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
}
.insights :deep(.n-card__content) {
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.insight-label,
.insights small {
  color: var(--ink-soft);
  font-size: 12px;
}
.insights strong {
  color: var(--ink);
  font-family: Georgia, "Times New Roman", "Microsoft YaHei", serif;
  font-size: 18px;
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

@media (max-width: 900px) {
  .metric-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .insights {
    grid-template-columns: 1fr;
  }
}
</style>
