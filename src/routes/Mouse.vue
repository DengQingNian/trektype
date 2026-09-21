<script setup lang="ts">
/** 鼠标热力图页：屏幕点击热力图（多显示器）+ 五类按钮分布 + Top 区域。 */
import { NCard, NSpin, NStatistic } from "naive-ui";
import { computed, onMounted, ref, watch } from "vue";
import type { EChartsOption } from "echarts";
import EChart from "../components/EChart.vue";
import RangePicker from "../components/RangePicker.vue";
import ScreenHeatmap from "../components/ScreenHeatmap.vue";
import TopList from "../components/TopList.vue";
import { useRangeStore } from "../stores/range";
import { useSettingsStore } from "../stores/settings";
import { api, BUTTON_LABELS, formatNumber, type MouseStats } from "../lib/ipc";
import { toastError } from "../lib/ui";

const range = useRangeStore();
const settings = useSettingsStore();
const loading = ref(false);
const stats = ref<MouseStats | null>(null);

const cellSize = computed(() => settings.config?.grid_cell_size ?? 24);
const palette = computed(() => settings.config?.heatmap_palette ?? "classic");

const buttonOption = computed<EChartsOption>(() => ({
  tooltip: { trigger: "item", formatter: "{b}: {c} ({d}%)" },
  legend: { bottom: 0, textStyle: { fontSize: 11 } },
  series: [
    {
      type: "pie",
      radius: ["42%", "68%"],
      center: ["50%", "44%"],
      itemStyle: { borderColor: "#fff", borderWidth: 2 },
      label: { fontSize: 11, formatter: "{b}\n{d}%" },
      data: (stats.value?.by_button ?? []).map((b) => ({
        name: BUTTON_LABELS[b.button] ?? b.button,
        value: b.count,
      })),
    },
  ],
}));

const REGION_NAMES = ["左上", "上", "右上", "左", "中", "右", "左下", "下", "右下"];

/** 每台显示器按 3×3 分区统计点击（区域边界按显示器物理尺寸均分）。 */
const topRegions = computed(() => {
  const s = stats.value;
  if (!s) return [];
  const total = s.total || 1;
  const cellsize = cellSize.value;
  const region = new Map<string, number>();
  for (const c of s.cells) {
    const mon = s.monitors.find((m) => m.id === c.monitor_id);
    if (!mon) continue;
    const gx = Math.min(2, Math.floor(((c.cell_x + 0.5) * cellsize) / (mon.width / 3)));
    const gy = Math.min(2, Math.floor(((c.cell_y + 0.5) * cellsize) / (mon.height / 3)));
    const key = `${mon.id}:${gy * 3 + gx}`;
    region.set(key, (region.get(key) ?? 0) + c.count);
  }
  return [...region.entries()]
    .map(([key, count]) => {
      const [midStr, idxStr] = key.split(":");
      const mon = s.monitors.find((m) => m.id === Number(midStr));
      const name = REGION_NAMES[Number(idxStr)] ?? "未知区域";
      return {
        label: `${mon?.device_key.replace(/^\\\\?\.\\/, "") ?? "屏"} · ${name}`,
        count,
        ratio: count / total,
      };
    })
    .sort((a, b) => b.count - a.count);
});

async function load() {
  loading.value = true;
  try {
    stats.value = await api.getMouseStats(range.range.start_date, range.range.end_date);
  } catch (e) {
    toastError(e, "加载鼠标统计失败");
  } finally {
    loading.value = false;
  }
}

onMounted(async () => {
  if (!settings.config) await settings.load();
  await load();
});
watch(() => range.range, load, { deep: true });
watch(cellSize, load);
</script>

<template>
  <div class="page">
    <div class="head">
      <h2>鼠标热力图</h2>
      <RangePicker />
    </div>

    <n-spin :show="loading">
      <div class="stats-row">
        <n-card size="small"><n-statistic label="点击总数" :value="formatNumber(stats?.total ?? 0)" /></n-card>
        <n-card size="small"><n-statistic label="显示器" :value="`${stats?.monitors.length ?? 0} 台`" /></n-card>
        <n-card size="small">
          <n-statistic
            label="左键占比"
            :value="`${stats && stats.total > 0 ? (((stats.by_button.find((b) => b.button === 'left')?.count ?? 0) / stats.total) * 100).toFixed(1) : '0.0'}%`"
          />
        </n-card>
      </div>

      <n-card size="small" class="mb">
        <ScreenHeatmap
          :monitors="stats?.monitors ?? []"
          :cells="stats?.cells ?? []"
          :cell-size="cellSize"
          :total="stats?.total ?? 0"
          :palette="palette"
        />
      </n-card>

      <div class="two-col">
        <n-card size="small" title="按钮分布">
          <EChart :option="buttonOption" height="240px" />
        </n-card>
        <n-card size="small" title="Top 区域（3×3 分区）">
          <TopList :items="topRegions" :max="5" unit=" 次" />
        </n-card>
      </div>
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
.two-col {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.mb {
  margin-bottom: 12px;
}
</style>
