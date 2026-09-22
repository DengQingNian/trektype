<script setup lang="ts">
/**
 * ECharts 轻封装：传入 option 即渲染，自动随容器 resize。
 * 仅在需要图表（折线/饼/日历）的页面使用；热力图均为自绘（SVG/Canvas）。
 */
import * as echarts from "echarts";
import { onBeforeUnmount, onMounted, ref, watch } from "vue";

const props = defineProps<{
  option: echarts.EChartsOption;
  height?: string;
}>();

const el = ref<HTMLDivElement | null>(null);
let chart: echarts.ECharts | null = null;

function render() {
  if (!el.value) return;
  if (!chart) chart = echarts.init(el.value);
  chart.setOption(props.option, true);
}

const onResize = () => chart?.resize();

onMounted(() => {
  render();
  window.addEventListener("resize", onResize);
});

watch(() => props.option, render, { deep: true });

onBeforeUnmount(() => {
  window.removeEventListener("resize", onResize);
  chart?.dispose();
  chart = null;
});
</script>

<template>
  <div ref="el" class="chart-frame" :style="{ width: '100%', height: height ?? '260px' }" />
</template>

<style scoped>
.chart-frame {
  min-width: 0;
}
</style>
