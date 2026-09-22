<script setup lang="ts">
/**
 * 屏幕点击热力图（Canvas 自绘，多显示器）。
 *
 * 渲染流程（计划 §8.3）：
 * 1. 按 monitors 快照重建虚拟桌面画布（含负坐标与 DPI 折算）；
 * 2. 按网格点击次数计算 log1p + P05/P99 色阶；
 * 3. 每个网格中心绘制可叠加的径向热核，再按 alpha 着色，形成连续热成像；
 * 4. 叠加显示器边框与名称；悬停仍显示对应聚合网格的点击数。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { NRadioGroup, NRadioButton } from "naive-ui";
import { Screen, ScreenOff } from "@vicons/carbon";
import {
  computeBounds,
  getHeatmapRamp,
  normalize,
  rampColor,
  rgbToCss,
  type HeatmapPalette,
} from "../lib/colorscale";
import {
  heatmapColorPosition,
  heatmapKernelRadius,
  heatmapPeakAlpha,
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

/** 参与渲染的显示器（单屏模式时只取一台）。 */
const shownMonitors = computed(() =>
  viewMode.value === "all" ? props.monitors : props.monitors.filter((m) => m.id === viewMode.value),
);

/** 虚拟桌面包围盒（物理像素，含负坐标）。 */
const bounds = computed(() => {
  const list = shownMonitors.value;
  if (list.length === 0) return { x: 0, y: 0, w: 1, h: 1 };
  const x = Math.min(...list.map((m) => m.x));
  const y = Math.min(...list.map((m) => m.y));
  const right = Math.max(...list.map((m) => m.x + m.width));
  const bottom = Math.max(...list.map((m) => m.y + m.height));
  return { x, y, w: Math.max(1, right - x), h: Math.max(1, bottom - y) };
});

/** 画布尺寸（CSS px）：按包围盒等比缩放到容器宽度，上限 900px 宽。 */
const canvasSize = computed(() => {
  const targetW = 900;
  const scale = Math.min(1, targetW / bounds.value.w);
  return {
    w: Math.round(bounds.value.w * scale),
    h: Math.round(bounds.value.h * scale),
    scale,
  };
});

function render() {
  const cv = canvas.value;
  const list = shownMonitors.value;
  if (!cv || list.length === 0) return;
  const { w, h, scale } = canvasSize.value;
  const dpr = window.devicePixelRatio || 1;
  cv.width = Math.max(1, Math.round(w * dpr));
  cv.height = Math.max(1, Math.round(h * dpr));
  cv.style.width = `${w}px`;
  cv.style.height = `${h}px`;

  const ctx = cv.getContext("2d");
  if (!ctx) return;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);

  const b = bounds.value;
  const toCanvas = (mx: number, my: number) => ({
    cx: (mx - b.x) * scale,
    cy: (my - b.y) * scale,
  });

  // 屏幕底色
  for (const m of list) {
    const p = toCanvas(m.x, m.y);
    ctx.fillStyle = "#0b1220";
    ctx.fillRect(p.cx, p.cy, m.width * scale, m.height * scale);
  }

  // 先画热度场：每个网格中心是一个平滑径向热核，多个热核会自然叠加。
  // 这样仍使用网格聚合数据，但最终画面没有网格方块的硬边界。
  const cellsByMonitor = new Map<number, GridCell[]>();
  for (const cell of props.cells) {
    const cells = cellsByMonitor.get(cell.monitor_id) ?? [];
    cells.push(cell);
    cellsByMonitor.set(cell.monitor_id, cells);
  }
  const visibleCells = props.cells.filter((cell) => list.some((m) => m.id === cell.monitor_id));
  const boundsForColor = computeBounds(visibleCells.map((cell) => cell.count));
  const heatRamp = getHeatmapRamp(props.palette);
  const heatLayer = document.createElement("canvas");
  heatLayer.width = cv.width;
  heatLayer.height = cv.height;
  const heatCtx = heatLayer.getContext("2d");
  if (!heatCtx) return;
  heatCtx.setTransform(dpr, 0, 0, dpr, 0, 0);
  heatCtx.clearRect(0, 0, w, h);

  const radius = heatmapKernelRadius(props.cellSize, scale);
  for (const mon of list) {
    const cells = cellsByMonitor.get(mon.id) ?? [];
    if (cells.length === 0) continue;
    const screen = toCanvas(mon.x, mon.y);
    heatCtx.save();
    // 热核不能把一台显示器的热度涂到显示器间的空白区域。
    heatCtx.beginPath();
    heatCtx.rect(screen.cx, screen.cy, mon.width * scale, mon.height * scale);
    heatCtx.clip();
    for (const cell of cells) {
      if (cell.count <= 0) continue;
      const p = toCanvas(
        mon.x + (cell.cell_x + 0.5) * props.cellSize,
        mon.y + (cell.cell_y + 0.5) * props.cellSize,
      );
      const t = Math.max(0.12, normalize(cell.count, boundsForColor));
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
  const maxAlpha = heatmapPeakAlpha(1);
  for (let i = 0; i < heatPixels.length; i += 4) {
    const alpha = heatPixels[i + 3] / 255;
    if (alpha <= 0.004) continue;
    const rgb = colorLut[Math.round(heatmapColorPosition(alpha, maxAlpha) * 255)];
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
  for (const m of list) {
    const p = toCanvas(m.x, m.y);
    ctx.strokeRect(p.cx + 0.5, p.cy + 0.5, m.width * scale - 1, m.height * scale - 1);
    const label = `${m.device_key.replace(/^\\\\?\.\\/, "")}${m.is_primary ? " · 主屏" : ""} ${m.width}×${m.height}${m.scale !== 1 ? ` @${m.scale}x` : ""}`;
    ctx.fillText(label, p.cx + 6, p.cy + 14);
  }
}

function onMove(e: MouseEvent) {
  const cv = canvas.value;
  if (!cv) return;
  const rect = cv.getBoundingClientRect();
  const { scale } = canvasSize.value;
  const b = bounds.value;
  const px = (e.clientX - rect.left) / scale + b.x;
  const py = (e.clientY - rect.top) / scale + b.y;
  const mon = shownMonitors.value.find(
    (m) => px >= m.x && px < m.x + m.width && py >= m.y && py < m.y + m.height,
  );
  if (!mon) {
    hover.value = null;
    return;
  }
  const cx = Math.floor((px - mon.x) / props.cellSize);
  const cy = Math.floor((py - mon.y) / props.cellSize);
  const cell = props.cells.find(
    (c) => c.monitor_id === mon.id && c.cell_x === cx && c.cell_y === cy,
  );
  hover.value = {
    x: Math.round(px),
    y: Math.round(py),
    count: cell?.count ?? 0,
  };
}

onMounted(render);
watch(() => [props.cells, props.monitors, props.palette, viewMode.value], render, {
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
      <n-radio-group v-model:value="viewMode" size="small">
        <n-radio-button value="all"><Screen class="ui-icon button-icon" />全部显示器</n-radio-button>
        <n-radio-button v-for="m in props.monitors" :key="m.id" :value="m.id">
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

    <div v-if="props.monitors.length === 0" class="empty empty-label"><ScreenOff class="ui-icon empty-icon" />暂无显示器记录（需要先采集到点击事件）</div>
    <canvas v-else ref="canvas" class="canvas" @mousemove="onMove" @mouseleave="hover = null" />

    <div class="footer">
      <span>总点击 <b>{{ props.total.toLocaleString() }}</b> 次</span>
      <span v-if="hover">
        坐标 ({{ hover.x }}, {{ hover.y }})：<b>{{ hover.count }}</b> 次点击
      </span>
      <span class="tip">平滑热成像 · 网格 {{ props.cellSize }}px · 色深按点击次数对数分布</span>
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
.canvas {
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
