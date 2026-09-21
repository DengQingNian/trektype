<script setup lang="ts">
/** Top-N 列表（Top 键 / Top 区域 / 按键分布共用）：名称 + 占比条 + 次数 + 百分比。 */
const props = defineProps<{
  items: { label: string; count: number; ratio: number; sublabel?: string }[];
  max?: number;
  unit?: string;
}>();

const limit = () => props.max ?? 10;
</script>

<template>
  <div class="top-list">
    <div v-for="(it, i) in items.slice(0, limit())" :key="it.label" class="row">
      <span class="rank">{{ i + 1 }}</span>
      <span class="label" :title="it.label">
        {{ it.label }}
        <span v-if="it.sublabel" class="sublabel">{{ it.sublabel }}</span>
      </span>
      <div class="bar">
        <div class="fill" :style="{ width: `${Math.max(2, it.ratio * 100)}%` }" />
      </div>
      <span class="count">{{ it.count.toLocaleString() }}<span class="unit">{{ props.unit ?? "" }}</span></span>
      <span class="ratio">{{ (it.ratio * 100).toFixed(1) }}%</span>
    </div>
    <div v-if="items.length === 0" class="empty">暂无数据</div>
  </div>
</template>

<style scoped>
.top-list {
  display: flex;
  flex-direction: column;
  gap: 7px;
}
.row {
  display: grid;
  grid-template-columns: 18px minmax(64px, 1.1fr) 1.4fr 78px 52px;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
}
.rank {
  color: #94a3b8;
  text-align: right;
}
.label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sublabel {
  color: #94a3b8;
  font-size: 11px;
  margin-left: 4px;
}
.bar {
  background: #e2e8f0;
  border-radius: 4px;
  height: 8px;
  overflow: hidden;
}
.fill {
  height: 100%;
  background: linear-gradient(90deg, #60a5fa, #2563eb);
  border-radius: 4px;
}
.count {
  text-align: right;
  font-variant-numeric: tabular-nums;
}
.unit {
  color: #94a3b8;
  font-size: 11px;
  margin-left: 2px;
}
.ratio {
  text-align: right;
  color: #64748b;
  font-variant-numeric: tabular-nums;
}
.empty {
  color: #94a3b8;
  font-size: 12.5px;
  padding: 6px 0;
}
</style>
