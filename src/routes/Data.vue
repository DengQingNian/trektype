<script setup lang="ts">
/**
 * 数据管理：数据库概览、导出（CSV/JSON × 聚合/明细）、按日期删除、保留期清理。
 * 明细（raw）含按键时序，导出前强制二次确认（隐私红线）。
 */
import {
  NAlert,
  NButton,
  NCard,
  NDatePicker,
  NRadioButton,
  NRadioGroup,
  NSpin,
} from "naive-ui";
import { Clean, DataBase, DocumentDownload, FolderOpen, TrashCan } from "@vicons/carbon";
import { computed, onMounted, ref } from "vue";
import { api, formatBytes, formatNumber, type DbStats } from "../lib/ipc";
import { dialog, message, toastError } from "../lib/ui";
import { parseDate, toDateString, todayLocal, addDays } from "../lib/date";

const stats = ref<DbStats | null>(null);
const loading = ref(false);
const busy = ref(false);

const exportFormat = ref<"csv" | "json">("csv");
const exportScope = ref<"agg" | "raw">("agg");
const exportRange = ref<[number, number]>([
  parseDate(addDays(todayLocal(), -6)).getTime(),
  parseDate(todayLocal()).getTime(),
]);

const deleteRange = ref<[number, number] | null>(null);

const exportDates = computed(() => ({
  start: toDateString(new Date(exportRange.value[0])),
  end: toDateString(new Date(exportRange.value[1])),
}));

async function load() {
  loading.value = true;
  try {
    stats.value = await api.getDbStats();
  } catch (e) {
    toastError(e, "读取数据库信息失败");
  } finally {
    loading.value = false;
  }
}

async function doExport() {
  const { start, end } = exportDates.value;
  if (exportScope.value === "raw") {
    const ok = await new Promise<boolean>((resolve) => {
      dialog.warning({
        title: "导出按键/点击明细",
        content:
          "明细数据包含按键的时间顺序（不含输入内容），导出文件为未加密的 CSV/JSON，可能被其他程序读取。确认导出？",
        positiveText: "确认导出",
        negativeText: "取消",
        onPositiveClick: () => resolve(true),
        onNegativeClick: () => resolve(false),
        onClose: () => resolve(false),
      });
    });
    if (!ok) return;
  }
  busy.value = true;
  try {
    const result = await api.exportData(exportFormat.value, exportScope.value, start, end);
    if (result) {
      message.success(`导出成功：${result.rows.toLocaleString()} 行 / ${formatBytes(result.bytes)}`);
    } else {
      message.info("已取消导出");
    }
  } catch (e) {
    toastError(e, "导出失败");
  } finally {
    busy.value = false;
  }
}

async function doDelete() {
  if (!deleteRange.value) {
    message.warning("请先选择要删除的日期区间");
    return;
  }
  const start = toDateString(new Date(deleteRange.value[0]));
  const end = toDateString(new Date(deleteRange.value[1]));
  const ok = await new Promise<boolean>((resolve) => {
    dialog.error({
      title: "删除数据（不可恢复）",
      content: `将删除 ${start} ~ ${end} 的明细与统计数据，且无法撤销。确认删除？`,
      positiveText: "删除",
      negativeText: "取消",
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
      onClose: () => resolve(false),
    });
  });
  if (!ok) return;
  busy.value = true;
  try {
    const removed = await api.deleteRange(start, end);
    message.success(`已删除 ${removed.toLocaleString()} 条明细及相关统计`);
    await load();
  } catch (e) {
    toastError(e, "删除失败");
  } finally {
    busy.value = false;
  }
}

async function doCleanup() {
  busy.value = true;
  try {
    const removed = await api.cleanupNow();
    message.success(removed > 0 ? `已清理 ${removed.toLocaleString()} 条过期明细` : "没有需要清理的过期明细");
    await load();
  } catch (e) {
    toastError(e, "清理失败");
  } finally {
    busy.value = false;
  }
}

async function openDir() {
  try {
    await api.openDataDir();
  } catch (e) {
    toastError(e, "打开目录失败");
  }
}

onMounted(load);
</script>

<template>
  <div class="page">
    <div class="head">
      <div class="head-copy">
        <span class="eyebrow">DATA / 05</span>
        <h2 class="title-row"><DataBase class="ui-icon title-icon" />数据管理</h2>
        <p>导出、整理和清理你的本地记录，所有动作都由你掌控。</p>
      </div>
      <n-button size="small" @click="openDir"><template #icon><FolderOpen class="button-icon" /></template>打开数据目录</n-button>
    </div>

    <n-spin :show="loading">
      <section class="data-overview mb" aria-label="数据概况">
        <div class="overview-primary">
          <div class="overview-stat"><span class="k">数据库大小</span><strong class="v">{{ formatBytes(stats?.db_bytes ?? 0) }}</strong></div>
          <div class="overview-stat"><span class="k">按键明细</span><strong class="v">{{ formatNumber(stats?.key_rows ?? 0) }}<small> 行</small></strong></div>
          <div class="overview-stat"><span class="k">点击明细</span><strong class="v">{{ formatNumber(stats?.mouse_rows ?? 0) }}<small> 行</small></strong></div>
        </div>
        <div class="overview-secondary">
          <div><span class="k">聚合行数</span><strong>{{ formatNumber(stats?.agg_rows ?? 0) }}</strong></div>
          <div><span class="k">应用</span><strong>{{ formatNumber(stats?.app_count ?? 0) }}</strong></div>
          <div><span class="k">显示器</span><strong>{{ formatNumber(stats?.monitor_count ?? 0) }}</strong></div>
          <div><span class="k">采集会话</span><strong>{{ formatNumber(stats?.session_count ?? 0) }}</strong></div>
          <div><span class="k">最早明细</span><strong>{{ stats?.oldest_raw_date ?? "—" }}</strong></div>
        </div>
      </section>

      <n-card size="small" class="mb">
        <template #header><span class="card-title"><DocumentDownload class="ui-icon card-title-icon" />导出数据</span></template>
        <div class="row">
          <span class="label">格式</span>
          <n-radio-group v-model:value="exportFormat" size="small">
            <n-radio-button value="csv">CSV</n-radio-button>
            <n-radio-button value="json">JSON</n-radio-button>
          </n-radio-group>
          <span class="label">范围</span>
          <n-radio-group v-model:value="exportScope" size="small">
            <n-radio-button value="agg">统计聚合</n-radio-button>
            <n-radio-button value="raw">明细（含时序）</n-radio-button>
          </n-radio-group>
          <n-date-picker v-model:value="exportRange" type="daterange" size="small" :clearable="false" style="width: 240px" />
          <n-button size="small" type="primary" :loading="busy" @click="doExport"><template #icon><DocumentDownload class="button-icon" /></template>导出</n-button>
        </div>
        <n-alert v-if="exportScope === 'raw'" type="warning" class="mt8">
          明细导出包含按键时间顺序（不含输入内容），导出文件为未加密文件，请妥善保管。
        </n-alert>
      </n-card>

      <n-card size="small">
        <template #header><span class="card-title"><TrashCan class="ui-icon card-title-icon" />删除数据</span></template>
        <div class="row">
          <span class="label">日期区间</span>
          <n-date-picker v-model:value="deleteRange" type="daterange" size="small" style="width: 240px" />
          <n-button size="small" type="error" :loading="busy" @click="doDelete"><template #icon><TrashCan class="button-icon" /></template>删除区间数据</n-button>
          <n-button size="small" :loading="busy" @click="doCleanup"><template #icon><Clean class="button-icon" /></template>立即清理过期明细</n-button>
        </div>
        <div class="note">
          删除会同时移除该区间的明细与统计聚合，且不可恢复。保留期清理只删除超过保留期的
          <b>明细</b>，统计聚合永久保留（可在设置页调整保留天数）。
        </div>
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
  gap: 16px;
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
.data-overview { padding: 20px 24px 16px; background: var(--paper-light); border: 1px solid rgba(44, 44, 44, .18); border-left: 4px solid var(--rust); box-shadow: 2px 2px 0 rgba(44, 44, 44, .12); }
.overview-primary { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 22px; padding-bottom: 18px; border-bottom: 1px solid rgba(44, 44, 44, .18); }
.overview-stat { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.overview-secondary { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 20px; padding-top: 15px; }
.overview-secondary > div { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.overview-secondary strong { color: var(--ink); font-size: 15px; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
.k {
  font-size: 11.5px;
  color: var(--ink-soft);
}
.v {
  color: var(--ink);
  font-family: Georgia, "Times New Roman", "Microsoft YaHei", serif;
  font-size: clamp(22px, 2.1vw, 32px);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
.v small { font: 12px "Segoe UI", "Microsoft YaHei", sans-serif; color: var(--ink-soft); }
.row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.label {
  font-size: 12.5px;
  color: #475569;
}
.mb {
  margin-bottom: 12px;
}
.mt8 {
  margin-top: 8px;
}
.note {
  margin-top: 10px;
  font-size: 12px;
  color: var(--ink-soft);
  line-height: 1.7;
}

@media (max-width: 900px) { .overview-secondary { grid-template-columns: repeat(3, minmax(0, 1fr)); } }

@media (max-width: 560px) {
  .overview-primary { grid-template-columns: 1fr; gap: 12px; }
  .overview-secondary { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
