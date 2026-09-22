<script setup lang="ts">
/**
 * 键盘热力图（SVG，自绘）：按布局 JSON 展开键位，填充色由色阶函数给出。
 * 色阶：log1p + P05/P99 分位裁剪（见 lib/colorscale.ts），避免空格/E 键淹没主体。
 */
import { NRadioGroup, NRadioButton } from "naive-ui";
import { computed, ref } from "vue";
import {
  ANSI_104,
  LAYOUTS,
  layoutBounds,
  placeKeys,
  type LayoutDef,
} from "../lib/layout";
import { buildColorMap, computeBounds, getHeatmapRamp, normalize, rgbToCss, type HeatmapPalette } from "../lib/colorscale";

const props = defineProps<{
  /** 键码 → 次数（来自 get_keyboard_stats.keys） */
  counts: Record<string, number>;
  total: number;
  /** 全局热力图配色方案 */
  palette: HeatmapPalette;
}>();

const layoutId = ref<string>(ANSI_104.id);
const layout = computed<LayoutDef>(
  () => LAYOUTS.find((l) => l.id === layoutId.value) ?? ANSI_104,
);
const placed = computed(() => placeKeys(layout.value));
const bounds = computed(() => layoutBounds(layout.value));

/** 单位键宽（px，与 viewBox 同单位） */
const U = 40;
const GAP = 3; // 键间空隙
const PAD = 8;

const colorMap = computed(() =>
  buildColorMap(
    new Map(Object.entries(props.counts)),
    "#f1f5f9",
    0.12,
    getHeatmapRamp(props.palette),
  ),
);
const paletteColors = computed(() => getHeatmapRamp(props.palette).map(rgbToCss));
const scaleBounds = computed(() => computeBounds(Object.values(props.counts)));

function countOf(code: string): number {
  return props.counts[code] ?? 0;
}

/** 深色底用白字（热度过 55% 视为深底）。 */
function textColor(code: string): string {
  return normalize(countOf(code), scaleBounds.value) > 0.55 ? "#ffffff" : "#334155";
}

const viewBox = computed(() => {
  const b = bounds.value;
  return `${-PAD} ${b.minY * U - PAD} ${b.width * U + PAD * 2} ${b.height * U + PAD * 2}`;
});

const maxCount = computed(() => Math.max(0, ...Object.values(props.counts)));
const hottest = computed(() => {
  let best = "";
  let bestN = 0;
  for (const [code, n] of Object.entries(props.counts)) {
    if (n > bestN) {
      best = code;
      bestN = n;
    }
  }
  return best ? { code: best, count: bestN } : null;
});

defineExpose({ maxCount, hottest });
</script>

<template>
  <div class="kb-wrap">
    <div class="kb-toolbar">
      <n-radio-group v-model:value="layoutId" size="small">
        <n-radio-button v-for="l in LAYOUTS" :key="l.id" :value="l.id">{{ l.name }}</n-radio-button>
      </n-radio-group>
      <div class="legend">
        <span class="legend-label">低</span>
        <span class="legend-bar">
          <i v-for="color in paletteColors" :key="color" :style="{ backgroundColor: color }" />
        </span>
        <span class="legend-label">高</span>
      </div>
    </div>

    <svg class="kb" :viewBox="viewBox" role="img" aria-label="键盘热力图">
      <g v-for="key in placed" :key="key.code">
        <rect
          :x="key.x * U"
          :y="key.y * U"
          :width="key.w * U - GAP"
          :height="U - GAP"
          rx="2"
          :fill="colorMap.get(key.code) ?? '#f1f5f9'"
          stroke="#cbd5e1"
          stroke-width="0.6"
        >
          <title>{{ key.label }}：{{ countOf(key.code).toLocaleString() }} 次</title>
        </rect>
        <text
          :x="key.x * U + (key.w * U - GAP) / 2"
          :y="key.y * U + (U - GAP) / 2"
          text-anchor="middle"
          dominant-baseline="central"
          :fill="textColor(key.code)"
          :font-size="key.w >= 2 ? 11 : 12"
          font-family="ui-monospace, Consolas, monospace"
        >
          {{ key.label }}
        </text>
      </g>
    </svg>

    <div class="kb-footer">
      <span>总计 <b>{{ props.total.toLocaleString() }}</b> 次按键</span>
      <span v-if="hottest">
        最热键：<b>{{ hottest.code }}</b>（{{ hottest.count.toLocaleString() }} 次）
      </span>
      <span class="tip">悬停查看单键次数</span>
    </div>
  </div>
</template>

<style scoped>
.kb-wrap {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.kb-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.kb {
  width: 100%;
  height: auto;
  background: var(--paper-light);
  border: 2px dashed var(--ink);
  border-radius: 2px;
  box-shadow: 1px 1px 0 rgba(44, 44, 44, 0.2);
}
.legend {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--ink-soft);
}
.legend-bar {
  width: 120px;
  height: 10px;
  display: inline-flex;
  overflow: hidden;
  border: 1px dashed var(--ink);
  border-radius: 2px;
}
.legend-bar i {
  flex: 1;
  display: block;
}
.kb-footer {
  display: flex;
  gap: 18px;
  font-size: 12.5px;
  color: var(--ink-soft);
  flex-wrap: wrap;
}
.tip {
  color: var(--ink-soft);
}
</style>
