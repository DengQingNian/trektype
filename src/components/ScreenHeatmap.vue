<script setup lang="ts">
/**
 * 屏幕点击热力图（Canvas 自绘，多显示器）。
 *
 * 渲染流程（计划 §8.3）：
 * 1. 每台显示器单独排布，避免不同时期记录的相同坐标显示器叠画；
 * 2. 每台显示器按本屏最高点击网格归一化；
 * 3. 每个网格中心绘制可叠加的径向热核，再按 alpha 着色，形成连续热成像；
 * 4. 叠加显示器边框与名称；悬停仍显示对应聚合网格的点击数。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { NRadioGroup, NRadioButton } from "naive-ui";
import { Screen, ScreenOff } from "@vicons/carbon";
import {
  getHeatmapRamp,
  rampColor,
  rgbToCss,
  type HeatmapPalette,
} from "../lib/colorscale";
import {
  heatmapColorPosition,
  heatmapKernelRadius,
  heatmapPeakAlpha,
  activeHeatmapMonitors,
  layoutHeatmapMonitors,
  normalizeHeatmapCount,
} from "../lib/screen-heatmap";
import type { GridCell, MonitorRow } from "../lib/ipc";

const props = defineProps<{
  monitors: MonitorRow[];
  cells: GridCell[];
  /** 当前网格粒度（px，物理像素），与后端 grid_cell_size 一致 */
  cellSize: number;
  total: number;
  /** 全局热力图配色方案 */
  palette: HeatmapPalette;
}>();

const canvas = ref<HTMLCanvasElement | null>(null);
const viewMode = ref<"all" | number>("all");
const hover = ref<{ x: number; y: number; count: number } | null>(null);

const activeMonitors = computed(() => activeHeatmapMonitors(props.monitors, props.cells));
const layout = computed(() => layoutHeatmapMonitors(activeMonitors.value, viewMode.value));

watch(activeMonitors, (monitors) => {
  if (viewMode.value !== "all" && !monitors.some((monitor) => monitor.id === viewMode.value)) viewMode.value = "all";
});

function render() {
  const cv = canvas.value;
  const { items, width: w, height: h } = layout.value;
  if (!cv || items.length === 0) return;
  const dpr = window.devicePixelRatio || 1;
  cv.width = Math.max(1, Math.round(w * dpr));
  cv.height = Math.max(1, Math.round(h * dpr));
  cv.style.width = `${w}px`;
  cv.style.height = `${h}px`;

  const ctx = cv.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);

  // 屏幕底色
  for (const item of items) {
    ctx.fillStyle = "#0b1220";
    ctx.fillRect(item.x, item.y, item.width, item.height);
  }

  // 先画热度场：每个网格中心是一个平滑径向热核，多个热核会自然叠加。
  // 这样仍使用网格聚合数据，但最终画面没有网格方块的硬边界。
  const cellsByMonitor = new Map<number, GridCell[]>();
  for (const cell of props.cells) {
    const cells = cellsByMonitor.get(cell.monitor_id) ?? [];
    cells.push(cell);
    cellsByMonitor.set(cell.monitor_id, cells);
  }
  const peakByMonitor = new Map<number, number>();
  for (const [monitorId, cells] of cellsByMonitor) {
    const peak = Math.max(0, ...cells.map((cell) => cell.count));
    peakByMonitor.set(monitorId, peak);
  }
  const heatRamp = getHeatmapRamp(props.palette);
  const heatLayer = document.createElement("canvas");
  heatLayer.width = cv.width;
  heatLayer.height = cv.height;
  const heatCtx = heatLayer.getContext("2d");
  if (!heatCtx) return;
  heatCtx.setTransform(dpr, 0, 0, dpr, 0, 0);
  heatCtx.clearRect(0, 0, w, h);

  for (const item of items) {
    const mon = item.monitor;
    const cells = cellsByMonitor.get(mon.id) ?? [];
    if (cells.length === 0) continue;
    const radius = heatmapKernelRadius(props.cellSize, item.scale);
    heatCtx.save();
    // 热核不能把一台显示器的热度涂到显示器间的空白区域。
    heatCtx.beginPath();
    heatCtx.rect(item.x, item.y, item.width, item.height);
    heatCtx.clip();
    for (const cell of cells) {
      if (cell.count <= 0) continue;
      const p = {
        cx: item.x + (cell.cell_x + 0.5) * props.cellSize * item.scale,
        cy: item.y + (cell.cell_y + 0.5) * props.cellSize * item.scale,
      };
      // 低频网格保留微弱可见度，但强度基准只取本屏最高点击次数。
      const t = Math.max(0.04, normalizeHeatmapCount(cell.count, peakByMonitor.get(mon.id) ?? 0));
      const peak = heatmapPeakAlpha(t);
      const gradient = heatCtx.createRadialGradient(p.cx, p.cy, 0, p.cx, p.cy, radius);
      gradient.addColorStop(0, `rgba(255, 255, 255, ${peak})`);
      gradient.addColorStop(0.22, `rgba(255, 255, 255, ${peak * 0.9})`);
      gradient.addColorStop(0.52, `rgba(255, 255, 255, ${peak * 0.5})`);
      gradient.addColorStop(0.78, `rgba(255, 255, 255, ${peak * 0.16})`);
      gradient.addColorStop(1, "rgba(255, 255, 255, 0)");
      heatCtx.fillStyle = gradient;
      heatCtx.fillRect(p.cx - radius, p.cy - radius, radius * 2, radius * 2);
    }
    heatCtx.restore();
  }

  // 热度场是灰度 alpha；这里用 LUT 一次性转换成当前配色，避免每个网格
  // 仍然对应一个硬色块，同时让重叠区域自然趋向高温颜色。
  const heatPixels = heatCtx.getImageData(0, 0, cv.width, cv.height).data;
  const colorLayer = document.createElement("canvas");
  colorLayer.width = cv.width;
  colorLayer.height = cv.height;
  const colorCtx = colorLayer.getContext("2d");
  if (!colorCtx) return;
  const colorPixels = colorCtx.createImageData(cv.width, cv.height);
  const colorLut = Array.from({ length: 256 }, (_, i) => rampColor(i / 255, heatRamp));
  for (let i = 0; i < heatPixels.length; i += 4) {
    const alpha = heatPixels[i + 3] / 255;
    if (alpha <= 0.004) continue;
    // alpha 是多个热核叠加后的真实透明度，范围是 0–1；不能再用单个热核的
    // 峰值作为整张图的色阶上限，否则轻微叠加也会过早落到红色。
    const rgb = colorLut[Math.round(heatmapColorPosition(alpha) * 255)];
    colorPixels.data[i] = rgb[0];
    colorPixels.data[i + 1] = rgb[1];
    colorPixels.data[i + 2] = rgb[2];
    // 留出屏幕底色，低频区域呈柔和蓝光，高频区域逐渐变成高温红。
    colorPixels.data[i + 3] = Math.round(Math.min(0.9, alpha * 0.92) * 255);
  }
  colorCtx.putImageData(colorPixels, 0, 0);
  ctx.drawImage(colorLayer, 0, 0, w, h);

  // 显示器边框与名称
  ctx.strokeStyle = "rgba(148,163,184,0.55)";
  ctx.fillStyle = "rgba(226,232,240,0.85)";
  ctx.font = "11px ui-sans-serif, system-ui";
  ctx.lineWidth = 1;
  for (const item of items) {
    const m = item.monitor;
    ctx.strokeRect(item.x + 0.5, item.y + 0.5, item.width - 1, item.height - 1);
    const label = `${m.device_key.replace(/^\\\\?\.\\/, "")}${m.is_primary ? " · 主屏" : ""} ${m.width}×${m.height}${m.scale !== 1 ? ` @${m.scale}x` : ""}`;
    ctx.fillText(label, item.x + 6, item.y + 14);
  }
}

function onMove(e: MouseEvent) {
  const cv = canvas.value;
  if (!cv) return;
  const rect = cv.getBoundingClientRect();
  const cx = (e.clientX - rect.left) * (cv.width / (window.devicePixelRatio || 1)) / rect.width;
  const cy = (e.clientY - rect.top) * (cv.height / (window.devicePixelRatio || 1)) / rect.height;
  const item = layout.value.items.find(
    (tile) => cx >= tile.x && cx < tile.x + tile.width && cy >= tile.y && cy < tile.y + tile.height,
  );
  if (!item) {
    hover.value = null;
    return;
  }
  const px = item.monitor.x + (cx - item.x) / item.scale;
  const py = item.monitor.y + (cy - item.y) / item.scale;
  const gridX = Math.floor((px - item.monitor.x) / props.cellSize);
  const gridY = Math.floor((py - item.monitor.y) / props.cellSize);
  const cell = props.cells.find(
    (c) => c.monitor_id === item.monitor.id && c.cell_x === gridX && c.cell_y === gridY,
  );
  hover.value = {
    x: Math.round(px),
    y: Math.round(py),
    count: cell?.count ?? 0,
  };
}

onMounted(render);
watch(() => [props.cells, props.monitors, props.palette, props.cellSize, viewMode.value], render, {
  deep: true,
  flush: "post",
});
onBeforeUnmount(() => {
  hover.value = null;
});
</script>

<template>
  <div class="screen-heat">
    <div class="tools">
      <n-radio-group class="monitor-group" v-model:value="viewMode" size="small">
        <n-radio-button value="all"><Screen class="ui-icon button-icon" />全部显示器</n-radio-button>
        <n-radio-button v-for="m in activeMonitors" :key="m.id" :value="m.id">
          {{ m.device_key.replace(/^\\\\?\.\\/, "") }}{{ m.is_primary ? "（主屏）" : "" }}
        </n-radio-button>
      </n-radio-group>
      <span class="legend">
        <span
          v-for="i in 5"
          :key="i"
          class="swatch"
          :style="{ background: rgbToCss(rampColor((i - 1) / 4, getHeatmapRamp(props.palette))) }"
        />
        <span class="legend-label">少 → 多</span>
      </span>
    </div>

    <div v-if="activeMonitors.length === 0" class="empty empty-label"><ScreenOff class="ui-icon empty-icon" />当前时间范围暂无屏幕点击记录</div>
    <canvas v-else ref="canvas" class="canvas" @mousemove="onMove" @mouseleave="hover = null" />

    <div class="footer">
      <span>总点击 <b>{{ props.total.toLocaleString() }}</b> 次</span>
      <span v-if="hover">
        坐标 ({{ hover.x }}, {{ hover.y }})：<b>{{ hover.count }}</b> 次点击
      </span>
      <span class="tip">平滑热成像 · 网格 {{ props.cellSize }}px · 每屏最高点击次数归一化</span>
    </div>
  </div>
</template>

<style scoped>
.screen-heat {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.tools {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.monitor-group {
  gap: 4px;
  display: flex;
  flex-wrap: wrap;
}
.canvas {
  align-self: center;
  background: var(--paper-light);
  border: 2px dashed var(--ink);
  border-radius: 2px;
  box-shadow: 1px 1px 0 rgba(44, 44, 44, 0.2);
  max-width: 100%;
}
.legend {
  display: flex;
  align-items: center;
  gap: 3px;
  font-size: 11px;
  color: var(--ink-soft);
}
.swatch {
  width: 18px;
  height: 10px;
  border-radius: 2px;
}
.legend-label {
  margin-left: 5px;
}
.footer {
  display: flex;
  gap: 18px;
  font-size: 12.5px;
  color: var(--ink-soft);
  flex-wrap: wrap;
}
.tip {
  color: var(--ink-soft);
}
.empty {
  color: #6f6a62;
  font-size: 13px;
  padding: 24px;
  text-align: center;
  background: var(--paper-light);
  border: 2px dashed var(--ink);
  border-radius: 2px;
}
</style>
