<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from "vue";
import { RouterLink, RouterView, useRoute } from "vue-router";
import { NButton, NTag } from "naive-ui";
import { Keyboard } from "@vicons/carbon";
import { useRuntimeStore } from "./stores/runtime";
import { useSettingsStore } from "./stores/settings";

const route = useRoute();
const runtime = useRuntimeStore();
const settings = useSettingsStore();

const navItems = [
  { name: "dashboard", label: "概览" },
  { name: "keyboard", label: "键盘热力图" },
  { name: "mouse", label: "鼠标热力图" },
  { name: "calendar", label: "日历统计" },
  { name: "data", label: "数据管理" },
  { name: "settings", label: "设置" },
];

/** 首启同意页为全屏独立布局，不显示导航与状态条。 */
const isOnboarding = computed(() => route.name === "onboarding");

/** 采集状态：以运行时状态为准（含钩子是否成功安装）。 */
const statusText = computed(() => {
  if (!settings.consented) return "未授权采集";
  if (runtime.state?.paused) return "已暂停";
  if (runtime.state && (!runtime.state.keyboard_installed || !runtime.state.mouse_installed)) {
    return "采集不可用（钩子未安装）";
  }
  return "采集中";
});
const statusType = computed(() => {
  if (!settings.consented) return "warning" as const;
  if (runtime.state?.paused) return "default" as const;
  if (runtime.state && (!runtime.state.keyboard_installed || !runtime.state.mouse_installed)) {
    return "error" as const;
  }
  return "success" as const;
});

onMounted(async () => {
  await settings.load();
  if (!isOnboarding.value) runtime.startPolling();
});

onBeforeUnmount(() => runtime.stopPolling());

async function togglePause() {
  await runtime.togglePause();
}
</script>

<template>
  <RouterView v-if="isOnboarding" />

  <div v-else class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <span class="logo" aria-hidden="true"><Keyboard /></span>
        <div>
          <div class="name">TypeTrek</div>
          <div class="tagline">本地键鼠行为统计</div>
        </div>
      </div>
      <nav>
        <RouterLink
          v-for="item in navItems"
          :key="item.name"
          :to="{ name: item.name }"
          class="nav-item"
          :class="{ active: route.name === item.name }"
        >
          {{ item.label }}
        </RouterLink>
      </nav>
      <div class="privacy-note">
        数据仅存本机 · 不上传<br />
        窗口关闭后仍在托盘运行
      </div>
    </aside>

    <main class="content">
      <header class="topbar">
        <div class="status">
          <n-tag :type="statusType" size="small" round>{{ statusText }}</n-tag>
          <span v-if="runtime.state" class="meta">
            已落库 {{ runtime.state.flushed_events.toLocaleString() }} 事件
            <template v-if="runtime.state.current_exe"> · 前台：{{ runtime.state.current_exe }}</template>
          </span>
        </div>
        <n-button
          v-if="settings.consented"
          size="small"
          :type="runtime.state?.paused ? 'primary' : 'default'"
          @click="togglePause"
        >
          {{ runtime.state?.paused ? "恢复采集" : "暂停采集" }}
        </n-button>
      </header>
      <RouterView />
    </main>
  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  height: 100vh;
  background: #f8fafc;
  color: #0f172a;
}
.sidebar {
  width: 208px;
  flex: 0 0 208px;
  background: #0f172a;
  color: #e2e8f0;
  display: flex;
  flex-direction: column;
  padding: 18px 12px;
  gap: 18px;
}
.brand {
  display: flex;
  gap: 10px;
  align-items: center;
  padding: 0 6px;
}
.logo {
  width: 28px;
  height: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #67e8f9;
  font-size: 27px;
}
.logo :deep(svg) {
  width: 100%;
  height: 100%;
}
.name {
  font-weight: 700;
  font-size: 17px;
}
.tagline {
  font-size: 11px;
  color: #94a3b8;
}
nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
}
.nav-item {
  color: #cbd5e1;
  text-decoration: none;
  padding: 9px 12px;
  border-radius: 8px;
  font-size: 13.5px;
}
.nav-item:hover {
  background: #1e293b;
}
.nav-item.active {
  background: #2563eb;
  color: #fff;
}
.privacy-note {
  font-size: 10.5px;
  color: #64748b;
  line-height: 1.6;
  padding: 0 6px;
}
.content {
  flex: 1;
  overflow-y: auto;
  padding: 18px 22px 40px;
}
.topbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}
.status {
  display: flex;
  align-items: center;
  gap: 10px;
}
.meta {
  font-size: 12px;
  color: #64748b;
}
</style>
