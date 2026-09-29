<script setup lang="ts">
import { NCard, NForm, NFormItem, NInputNumber, NSelect, NSwitch } from "naive-ui";
import { Timer } from "@vicons/carbon";
import type { AppConfig } from "../lib/ipc";

const props = defineProps<{ config: AppConfig }>();
const emit = defineEmits<{ "update:config": [config: AppConfig] }>();
const windowOptions = [5, 10, 15, 30].map((value) => ({ label: `${value} 分钟`, value }));

/** 复制完整配置，交由设置页原有的防抖自动保存流程处理。 */
function update<K extends keyof AppConfig>(key: K, value: AppConfig[K]) {
  emit("update:config", { ...props.config, [key]: value });
}

/** 输入框清空或输入无效数字时保留已保存的阈值。 */
function updateLimit(key: "daily_key_limit" | "daily_click_limit" | "burst_key_limit" | "burst_click_limit", value: number | null) {
  if (value == null || !Number.isInteger(value) || value < 1 || value > 1_000_000) return;
  update(key, value);
}
</script>

<template>
  <n-card size="small" class="reminder-card">
    <template #header><span class="card-title"><Timer class="ui-icon card-title-icon" />休息提醒</span></template>
    <p class="reminder-intro">按键与点击分别计算；达到阈值后发送系统通知，采集会继续。每日每项最多提醒一次，短期提醒间隔至少 30 分钟。</p>
    <n-form label-placement="left" label-width="140">
      <n-form-item label="每日按键">
        <n-switch data-test="daily-key-enabled" :value="config.daily_key_reminder_enabled" @update:value="update('daily_key_reminder_enabled', $event)" />
        <n-input-number data-test="daily-key-limit" class="limit-input" :value="config.daily_key_limit" :min="1" :max="1000000" :precision="0" :show-button="false" @update:value="updateLimit('daily_key_limit', $event)" />
        <span class="unit">次 / 日</span>
      </n-form-item>
      <n-form-item label="每日点击">
        <n-switch data-test="daily-click-enabled" :value="config.daily_click_reminder_enabled" @update:value="update('daily_click_reminder_enabled', $event)" />
        <n-input-number data-test="daily-click-limit" class="limit-input" :value="config.daily_click_limit" :min="1" :max="1000000" :precision="0" :show-button="false" @update:value="updateLimit('daily_click_limit', $event)" />
        <span class="unit">次 / 日</span>
      </n-form-item>
      <n-form-item label="短期按键">
        <n-switch data-test="burst-key-enabled" :value="config.burst_key_reminder_enabled" @update:value="update('burst_key_reminder_enabled', $event)" />
        <n-input-number data-test="burst-key-limit" class="limit-input" :value="config.burst_key_limit" :min="1" :max="1000000" :precision="0" :show-button="false" @update:value="updateLimit('burst_key_limit', $event)" />
        <span class="unit">次 / 窗口</span>
      </n-form-item>
      <n-form-item label="短期点击">
        <n-switch data-test="burst-click-enabled" :value="config.burst_click_reminder_enabled" @update:value="update('burst_click_reminder_enabled', $event)" />
        <n-input-number data-test="burst-click-limit" class="limit-input" :value="config.burst_click_limit" :min="1" :max="1000000" :precision="0" :show-button="false" @update:value="updateLimit('burst_click_limit', $event)" />
        <span class="unit">次 / 窗口</span>
      </n-form-item>
      <n-form-item label="短期窗口">
        <n-select data-test="burst-window" class="window-select" :value="config.burst_window_minutes" :options="windowOptions" @update:value="update('burst_window_minutes', $event)" />
        <span class="unit">键盘与鼠标共用，按任意连续时段计算</span>
      </n-form-item>
    </n-form>
    <p class="reminder-note">按键口径跟随“计入自动重复”；敏感应用黑名单中的输入不参与提醒。关闭应用后，短期窗口重新计算。</p>
  </n-card>
</template>

<style scoped>
.reminder-intro, .reminder-note { margin: 0 0 12px; color: var(--ink-soft); font-size: 12px; line-height: 1.7; }
.reminder-note { margin: 4px 0 0; }
.limit-input { width: 140px; margin-left: 12px; }
.window-select { width: 140px; }
.unit { margin-left: 9px; color: var(--ink-soft); font-size: 12px; }
</style>
