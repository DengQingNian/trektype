<script setup lang="ts">
/** 键盘热力图页：范围选择 + 104/87 布局热力图 + Top 键 + 占比。 */
import { NCard, NSpin, NStatistic } from "naive-ui";
import { computed, onMounted, ref, watch } from "vue";
import KeyboardHeatmap from "../components/KeyboardHeatmap.vue";
import RangePicker from "../components/RangePicker.vue";
import TopList from "../components/TopList.vue";
import { useRangeStore } from "../stores/range";
import { useSettingsStore } from "../stores/settings";
import { api, formatNumber, type KeyboardStats } from "../lib/ipc";
import { defaultLabel } from "../lib/layout";
import { toastError } from "../lib/ui";

const range = useRangeStore();
const settings = useSettingsStore();
const loading = ref(false);
const stats = ref<KeyboardStats | null>(null);
const palette = computed(() => settings.config?.heatmap_palette ?? "classic");

const counts = computed<Record<string, number>>(() => {
  const out: Record<string, number> = {};
  for (const k of stats.value?.keys ?? []) out[k.code] = k.count;
  return out;
});

const topKeys = computed(() =>
  (stats.value?.keys ?? []).map((k) => ({
    label: defaultLabel(k.code),
    sublabel: k.code,
    count: k.count,
    ratio: stats.value && stats.value.total > 0 ? k.count / stats.value.total : 0,
  })),
);

const repeatTotal = computed(() =>
  (stats.value?.keys ?? []).reduce((sum, k) => sum + k.repeat_count, 0),
);

async function load() {
  loading.value = true;
  try {
    stats.value = await api.getKeyboardStats(range.range.start_date, range.range.end_date);
  } catch (e) {
    toastError(e, "加载键盘统计失败");
  } finally {
    loading.value = false;
  }
}

onMounted(async () => {
  if (!settings.config) await settings.load();
  await load();
});
watch(() => range.range, load, { deep: true });
</script>

<template>
  <div class="page">
    <div class="head">
      <h2>键盘热力图</h2>
      <RangePicker />
    </div>

    <n-spin :show="loading">
      <div class="stats-row">
        <n-card size="small"><n-statistic label="按键总数" :value="formatNumber(stats?.total ?? 0)" /></n-card>
        <n-card size="small"><n-statistic label="不同键位" :value="formatNumber(stats?.keys.length ?? 0)" /></n-card>
        <n-card size="small">
          <n-statistic label="自动重复（未计入）" :value="formatNumber(repeatTotal)" />
        </n-card>
      </div>

      <n-card size="small" class="mb">
        <KeyboardHeatmap :counts="counts" :total="stats?.total ?? 0" :palette="palette" />
      </n-card>

      <n-card size="small" title="Top 键位">
        <TopList :items="topKeys" :max="10" unit=" 次" />
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
  flex-direction: column;
  gap: 8px;
}
h2 {
  margin: 0;
  font-size: 18px;
}
.stats-row {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 12px;
  margin-bottom: 12px;
}
.mb {
  margin-bottom: 12px;
}
</style>
