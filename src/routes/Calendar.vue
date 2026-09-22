<script setup lang="ts">
/** 日历统计页：月历热力图（键盘/鼠标切换）+ 点击某日查看详情。 */
import { NButton, NCard, NDatePicker, NSpin, NStatistic } from "naive-ui";
import { ArrowLeft, ArrowRight, Calendar, CalendarHeatMap, Cursor2, Information, Keyboard } from "@vicons/carbon";
import { computed, onMounted, ref, watch } from "vue";
import CalendarHeat from "../components/CalendarHeat.vue";
import TopList from "../components/TopList.vue";
import { api, BUTTON_LABELS, formatNumber, type DayCount, type DayDetail } from "../lib/ipc";
import { monthOf, parseDate, toDateString, todayLocal } from "../lib/date";
import { defaultLabel } from "../lib/layout";
import { toastError } from "../lib/ui";
import { useSettingsStore } from "../stores/settings";

const monthTs = ref<number>(parseDate(todayLocal()).getTime());
const settings = useSettingsStore();
const metric = ref<"key" | "click">("key");
const loading = ref(false);
const days = ref<DayCount[]>([]);
const selected = ref<string | null>(null);
const detail = ref<DayDetail | null>(null);
const palette = computed(() => settings.config?.heatmap_palette ?? "classic");

const month = computed(() => monthOf(toDateString(new Date(monthTs.value))));

async function loadMonth() {
  loading.value = true;
  try {
    days.value = await api.getCalendarStats(month.value);
    // 切月后清空选中详情
    selected.value = null;
    detail.value = null;
  } catch (e) {
    toastError(e, "加载月份统计失败");
  } finally {
    loading.value = false;
  }
}

async function selectDate(date: string) {
  selected.value = date;
  try {
    detail.value = await api.getDayDetail(date);
  } catch (e) {
    toastError(e, "加载单日详情失败");
  }
}

function shiftMonth(delta: number) {
  const d = new Date(monthTs.value);
  d.setMonth(d.getMonth() + delta, 1);
  monthTs.value = d.getTime();
}

const monthSummary = computed(() => {
  const keyTotal = days.value.reduce((s, d) => s + d.key_count, 0);
  const clickTotal = days.value.reduce((s, d) => s + d.click_count, 0);
  const activeDays = days.value.filter((d) => d.key_count > 0 || d.click_count > 0).length;
  return { keyTotal, clickTotal, activeDays };
});

const detailKeys = computed(() =>
  (detail.value?.key_top ?? []).map((k) => ({
    label: defaultLabel(k.code),
    sublabel: k.code,
    count: k.count,
    ratio: detail.value && detail.value.key_total > 0 ? k.count / detail.value.key_total : 0,
  })),
);

const detailButtons = computed(() =>
  (detail.value?.by_button ?? []).map((b) => ({
    label: BUTTON_LABELS[b.button] ?? b.button,
    count: b.count,
    ratio: detail.value && detail.value.click_total > 0 ? b.count / detail.value.click_total : 0,
  })),
);

onMounted(async () => {
  if (!settings.config) await settings.load();
  await loadMonth();
});
watch(month, loadMonth);
</script>

<template>
  <div class="page">
    <div class="head">
      <div class="head-copy">
        <span class="eyebrow">CALENDAR / 04</span>
      <h2 class="title-row"><Calendar class="ui-icon title-icon" />日历统计</h2>
        <p>把一个月摊开，找到节奏最浓的那几天。</p>
      </div>
      <div class="tools">
        <n-button size="small" @click="shiftMonth(-1)"><template #icon><ArrowLeft class="button-icon" /></template>上个月</n-button>
        <n-date-picker v-model:value="monthTs" type="month" size="small" :clearable="false" style="width: 140px" />
        <n-button size="small" @click="shiftMonth(1)"><template #icon><ArrowRight class="button-icon" /></template>下个月</n-button>
      </div>
    </div>

    <n-spin :show="loading">
      <div class="stats-row stagger-children">
        <n-card size="small" style="--stagger-index: 0"><div class="stat-card"><Keyboard class="ui-icon metric-icon" /><n-statistic label="本月按键" :value="formatNumber(monthSummary.keyTotal)" /></div></n-card>
        <n-card size="small" style="--stagger-index: 1"><div class="stat-card"><Cursor2 class="ui-icon metric-icon" /><n-statistic label="本月点击" :value="formatNumber(monthSummary.clickTotal)" /></div></n-card>
        <n-card size="small" style="--stagger-index: 2"><div class="stat-card"><CalendarHeatMap class="ui-icon metric-icon" /><n-statistic label="有记录天数" :value="`${monthSummary.activeDays} 天`" /></div></n-card>
      </div>

      <n-card size="small" class="mb">
        <template #header><span class="card-title"><CalendarHeatMap class="ui-icon card-title-icon" />{{ month }} 活跃度</span></template>
        <CalendarHeat
          :month="month"
          :days="days"
          :metric="metric"
          :palette="palette"
          @update:metric="(v) => (metric = v)"
          @select="selectDate"
        />
      </n-card>

      <n-card v-if="selected" size="small">
        <template #header><span class="card-title"><Calendar class="ui-icon card-title-icon" />{{ selected }} 详情</span></template>
        <div class="detail-stats">
          <n-statistic label="按键" :value="formatNumber(detail?.key_total ?? 0)" />
          <n-statistic label="点击" :value="formatNumber(detail?.click_total ?? 0)" />
        </div>
        <div class="detail-cols">
          <div>
            <h4>Top 键位</h4>
            <TopList :items="detailKeys" :max="8" unit=" 次" />
          </div>
          <div>
            <h4>按钮分布</h4>
            <TopList :items="detailButtons" :max="5" unit=" 次" />
          </div>
        </div>
      </n-card>
      <n-card v-else size="small">
        <span class="hint empty-label"><Information class="ui-icon empty-icon" />点击日历中的某一天查看当日详情。</span>
      </n-card>
    </n-spin>
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
  justify-content: space-between;
  align-items: flex-end;
  gap: 12px;
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
h4 {
  margin: 0 0 8px;
  font-size: 12.5px;
  color: #475569;
}
.tools {
  display: flex;
  gap: 8px;
  align-items: center;
}
.stats-row {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
  margin-bottom: 12px;
}
.detail-stats {
  display: flex;
  gap: 40px;
  margin-bottom: 12px;
}
.detail-cols {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
}
.hint {
  color: var(--ink-soft);
  font-size: 12.5px;
}
.mb {
  margin-bottom: 12px;
}

@media (max-width: 680px) {
  .stats-row,
  .detail-cols {
    grid-template-columns: 1fr;
  }
}
</style>
