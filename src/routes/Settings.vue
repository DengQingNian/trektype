<script setup lang="ts">
/**
 * 设置页：本地副本 + 防抖自动保存（每次变更 500ms 后落盘）。
 * 所有开关即时生效（后端把配置推送到采集热路径的原子量）。
 */
import {
  NAlert,
  NButton,
  NCard,
  NDivider,
  NForm,
  NFormItem,
  NInput,
  NSelect,
  NSwitch,
} from "naive-ui";
import { Activity, Add, ColorPalette, Filter, FolderOpen, Locked, Security, Settings as SettingsIcon } from "@vicons/carbon";
import { computed, onMounted, ref, watch } from "vue";
import { api, type AppConfig, type AppRow } from "../lib/ipc";
import { getHeatmapRamp, HEATMAP_PALETTES, rgbToCss } from "../lib/colorscale";
import { message, toastError } from "../lib/ui";
import { useSettingsStore } from "../stores/settings";

const settings = useSettingsStore();
const form = ref<AppConfig | null>(null);
const knownApps = ref<AppRow[]>([]);
const saving = ref(false);
let saveTimer: number | null = null;

const retentionOptions = [30, 90, 180, 365].map((d) => ({ label: `${d} 天`, value: d }));
const cellOptions = [24, 32, 64].map((v) => ({ label: `${v} px`, value: v }));
const paletteOptions = HEATMAP_PALETTES.map((palette) => ({
  label: `${palette.label}（${palette.description}）`,
  value: palette.value,
}));
const appOptions = computed(() =>
  knownApps.value.map((a) => ({ label: `${a.exe_name}（最近使用）`, value: a.exe_name })),
);
const palettePreview = computed(() => {
  const palette = form.value?.heatmap_palette ?? "classic";
  return getHeatmapRamp(palette).map(rgbToCss);
});

async function load() {
  if (!settings.config) await settings.load();
  form.value = { ...(settings.config as AppConfig) };
  try {
    knownApps.value = await api.getKnownApps();
  } catch {
    knownApps.value = [];
  }
}

function scheduleSave() {
  if (saveTimer != null) window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(async () => {
    if (!form.value) return;
    saving.value = true;
    try {
      await settings.save(form.value);
      message.success("设置已保存", { duration: 1500 });
    } catch (e) {
      toastError(e, "设置保存失败");
    } finally {
      saving.value = false;
    }
  }, 500);
}

watch(form, scheduleSave, { deep: true });

/** 黑名单快捷添加：把已知应用追加进对应名单（去重）。 */
function addToBlacklist(list: "blacklist_keys" | "blacklist_mouse", exe: string) {
  if (!form.value) return;
  const current = form.value[list] ?? [];
  if (!current.includes(exe)) {
    form.value[list] = [...current, exe];
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
  <div class="page" v-if="form">
    <div class="head">
      <div class="head-copy">
        <span class="eyebrow">SETTINGS / 06</span>
        <h2 class="title-row"><SettingsIcon class="ui-icon title-icon" />设置</h2>
        <p>调整采集边界、隐私策略和热力图的笔触颜色。</p>
      </div>
      <span class="hint">{{ saving ? "保存中…" : "变更自动保存" }}</span>
    </div>

    <n-card size="small">
      <template #header><span class="card-title"><Activity class="ui-icon card-title-icon" />采集</span></template>
      <n-form label-placement="left" label-width="140">
        <n-form-item label="采集总开关">
          <n-switch v-model:value="form.capture_enabled" />
          <span class="desc">关闭即暂停（等同于托盘"暂停采集"）；状态会保留到下次启动</span>
        </n-form-item>
        <n-form-item label="键盘采集">
          <n-switch v-model:value="form.capture_keyboard" />
          <span class="desc">记录键位标识与按下时间（不含输入内容）</span>
        </n-form-item>
        <n-form-item label="鼠标采集">
          <n-switch v-model:value="form.capture_mouse" />
          <span class="desc">只记录左/右/中/侧键的按下与坐标（无移动轨迹、无滚轮）</span>
        </n-form-item>
        <n-form-item label="计入自动重复">
          <n-switch v-model:value="form.repeat_counts" />
          <span class="desc">开启后长按产生的重复按键也计入统计（默认只统计真实按键动作）</span>
        </n-form-item>
        <n-form-item label="过滤脚本注入">
          <n-switch v-model:value="form.ignore_injected" />
          <span class="desc">默认忽略 AutoHotkey 等工具注入的按键；使用自动化工具时可关闭</span>
        </n-form-item>
        <n-form-item label="暂停/恢复快捷键">
          <n-input v-model:value="form.pause_hotkey" placeholder="如 Ctrl+Alt+P（留空禁用）" style="width: 220px" />
        </n-form-item>
        <n-form-item label="开机自启">
          <n-switch v-model:value="form.autostart" />
          <span class="desc">登录后自动启动并最小化到托盘继续统计</span>
        </n-form-item>
      </n-form>
    </n-card>

    <n-card size="small" class="mt12">
      <template #header><span class="card-title"><Locked class="ui-icon card-title-icon" />隐私</span></template>
      <n-alert type="info" class="mb8">
        <template #icon><Security class="ui-icon" /></template>
        窗口标题、输入内容、剪贴板、截图与鼠标轨迹<b>从设计上不采集</b>，因此这里没有对应开关。
      </n-alert>
      <n-form label-placement="left" label-width="140">
        <n-form-item label="隐私模式">
          <n-switch v-model:value="form.privacy_mode" />
          <span class="desc">
            开启后逐条明细完全不写入数据库，只保留计数统计（代价：无法导出明细、无法查看单次事件）。
            已存在的明细不受影响，可在数据管理页删除。
          </span>
        </n-form-item>
        <n-form-item label="明细保留期">
          <n-select v-model:value="form.raw_retention_days" :options="retentionOptions" style="width: 140px" />
          <span class="desc">超期自动清理明细；统计聚合永久保留</span>
        </n-form-item>
        <n-form-item label="热力图网格粒度">
          <n-select v-model:value="form.grid_cell_size" :options="cellOptions" style="width: 140px" />
          <span class="desc">只支持向更粗粒度切换（聚合按 24px 基准存储）</span>
        </n-form-item>
        <n-form-item label="热力图配色">
          <ColorPalette class="field-icon" />
          <n-select v-model:value="form.heatmap_palette" :options="paletteOptions" style="width: 220px" />
          <span class="palette-preview" aria-label="当前热力图配色预览">
            <i v-for="color in palettePreview" :key="color" class="palette-chip" :style="{ backgroundColor: color }" />
          </span>
          <span class="desc">键盘、日历、鼠标热力图统一使用此配色</span>
        </n-form-item>
      </n-form>
    </n-card>

    <n-card size="small" class="mt12">
      <template #header><span class="card-title"><Filter class="ui-icon card-title-icon" />敏感应用黑名单</span></template>
      <div class="desc-block">
        名单内的应用<b>完全不被记录</b>：事件在写入数据库前丢弃，应用名也不会进入统计字典。
        支持通配符（如 <code>*bank*</code>）。
      </div>
      <n-form label-placement="left" label-width="140">
        <n-form-item label="键盘黑名单">
          <n-select
            v-model:value="form.blacklist_keys"
            multiple
            filterable
            tag
            :options="appOptions"
            placeholder="输入 exe 名后回车添加，如 keepass.exe 或 *bank*"
          />
        </n-form-item>
        <n-form-item label="鼠标黑名单">
          <n-select
            v-model:value="form.blacklist_mouse"
            multiple
            filterable
            tag
            :options="appOptions"
            placeholder="同上"
          />
        </n-form-item>
      </n-form>
      <n-divider style="margin: 4px 0 10px" />
      <div class="quick-add">
        <span class="desc">从最近使用过的应用快速添加：</span>
        <n-button
          v-for="app in knownApps.slice(0, 8)"
          :key="app.id"
          size="tiny"
          secondary
          @click="addToBlacklist('blacklist_keys', app.exe_name)"
        >
          <Add class="button-icon" />{{ app.exe_name }}
        </n-button>
        <span v-if="knownApps.length === 0" class="desc">（暂无记录，采集一段时间后会显示）</span>
      </div>
    </n-card>

    <n-card size="small" class="mt12">
      <template #header><span class="card-title"><Security class="ui-icon card-title-icon" />存储与安全</span></template>
      <n-form label-placement="left" label-width="140">
        <n-form-item label="数据库加密">
          <n-switch v-model:value="form.db_encrypted" />
          <span class="desc">
            SQLCipher 整库加密，256-bit 密钥保存在 Windows 凭据管理器。
            加密只对<b>新建数据库</b>生效：切换前请先在数据管理页导出数据、清空数据库文件，然后重启。
            密钥丢失将无法恢复数据（已存在的明细不受当前开关影响）。
          </span>
        </n-form-item>
      </n-form>
      <n-button size="small" @click="openDir"><template #icon><FolderOpen class="button-icon" /></template>打开数据目录</n-button>
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
.hint {
  font-size: 12px;
  color: var(--ink-soft);
}
.mt12 {
  margin-top: 12px;
}
.mb8 {
  margin-bottom: 8px;
}
.desc {
  margin-left: 10px;
  font-size: 12px;
  color: var(--ink-soft);
  line-height: 1.6;
}
.palette-preview {
  display: inline-flex;
  width: 120px;
  height: 14px;
  margin-left: 10px;
  overflow: hidden;
  border: 1px dashed var(--ink);
  border-radius: 2px;
  vertical-align: middle;
}
.palette-chip {
  flex: 1;
  display: block;
}
.desc-block {
  font-size: 12.5px;
  color: #475569;
  line-height: 1.8;
  margin-bottom: 10px;
}
code {
  background: var(--paper-deep);
  padding: 1px 5px;
  border: 1px dashed var(--line);
  border-radius: 2px;
  font-size: 11.5px;
}
.quick-add {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
</style>
