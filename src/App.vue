<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted } from "vue";
import { RouterLink, RouterView, useRoute } from "vue-router";
import { NButton, NTag } from "naive-ui";
import { Activity, Calendar, Cursor2, Dashboard, DataTable, Keyboard, Locked, Pause, Play, Settings } from "@vicons/carbon";
import { useRuntimeStore } from "./stores/runtime";
import { useSettingsStore } from "./stores/settings";

const route = useRoute();
const runtime = useRuntimeStore();
const settings = useSettingsStore();
const navItems = [
  { name: "dashboard", label: "概览", icon: Dashboard },
  { name: "keyboard", label: "键盘热力图", icon: Keyboard },
  { name: "mouse", label: "鼠标热力图", icon: Cursor2 },
  { name: "calendar", label: "日历统计", icon: Calendar },
  { name: "data", label: "数据管理", icon: DataTable },
  { name: "settings", label: "设置", icon: Settings },
];
const isOnboarding = computed(() => route.name === "onboarding");
const statusText = computed(() => {
  if (!settings.consented) return "未授权采集";
  if (runtime.state?.paused) return "已暂停";
  if (runtime.state && (!runtime.state.keyboard_installed || !runtime.state.mouse_installed)) return "采集不可用（钩子未安装）";
  return "采集中";
});
const statusType = computed(() => {
  if (!settings.consented) return "warning" as const;
  if (runtime.state?.paused) return "default" as const;
  if (runtime.state && (!runtime.state.keyboard_installed || !runtime.state.mouse_installed)) return "error" as const;
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
        <div><div class="name">TypeTrek</div><div class="tagline">本地键鼠行为统计</div></div>
      </div>
      <div class="sidebar-rule" aria-hidden="true"><span>LOCAL / PRIVATE</span></div>
      <nav aria-label="主导航">
        <RouterLink v-for="item in navItems" :key="item.name" :to="{ name: item.name }" class="nav-item" :class="{ active: route.name === item.name }">
          <span class="button-label"><component :is="item.icon" class="ui-icon nav-icon" aria-hidden="true" />{{ item.label }}</span>
          <span class="nav-mark" aria-hidden="true">↗</span>
        </RouterLink>
      </nav>
      <div class="privacy-note"><Locked class="ui-icon privacy-icon" aria-hidden="true" /><span>数据仅存本机 · 不上传</span><span>窗口关闭后仍在托盘运行</span></div>
    </aside>
    <main class="content">
      <header class="topbar">
        <div class="status">
          <span class="topbar-kicker">TODAY'S TRACE</span>
          <n-tag :type="statusType" size="small"><template #icon><Activity class="ui-icon" /></template>{{ statusText }}</n-tag>
          <span v-if="runtime.state" class="meta">已落库 {{ runtime.state.flushed_events.toLocaleString() }} 事件<template v-if="runtime.state.current_exe"> · 前台：{{ runtime.state.current_exe }}</template></span>
        </div>
        <n-button v-if="settings.consented" size="small" :type="runtime.state?.paused ? 'primary' : 'default'" @click="togglePause">
          <template #icon><Play v-if="runtime.state?.paused" class="button-icon" /><Pause v-else class="button-icon" /></template>
          {{ runtime.state?.paused ? "恢复采集" : "暂停采集" }}
        </n-button>
      </header>
      <div class="view-stack">
        <RouterView v-slot="{ Component }">
          <Transition name="page-slide" mode="out-in">
            <component :is="Component" :key="route.fullPath" />
          </Transition>
        </RouterView>
      </div>
    </main>
  </div>
</template>

<style scoped>
.app-shell { display: flex; height: 100vh; background: transparent; color: var(--ink); }
.sidebar { width: 232px; flex: 0 0 232px; margin: 14px 0 14px 14px; padding: 20px 14px 16px; background: var(--paper-deep); color: var(--ink); border: 2px dashed var(--ink); border-radius: 3px; box-shadow: 3px 3px 0 rgba(44, 44, 44, .28); display: flex; flex-direction: column; gap: 16px; transform: rotate(-.25deg); }
.brand { display: flex; gap: 10px; align-items: center; padding: 0 6px; }
.logo { width: 36px; height: 36px; display: inline-flex; align-items: center; justify-content: center; color: var(--paper); background: var(--ink); border: 2px solid var(--ink); box-shadow: 2px 2px 0 rgba(44, 44, 44, .3); transform: rotate(3deg); font-size: 27px; }
.logo :deep(svg) { width: 100%; height: 100%; }
.name { font-weight: 700; font-size: 17px; letter-spacing: .02em; }
.tagline { font-size: 11px; color: var(--ink-soft); }
.sidebar-rule { display: flex; align-items: center; gap: 8px; color: var(--ink-soft); font-size: 9px; letter-spacing: .14em; }
.sidebar-rule::before, .sidebar-rule::after { content: ""; height: 1px; flex: 1; border-top: 1px dashed var(--line); }
nav { display: flex; flex-direction: column; gap: 5px; flex: 1; }
.nav-item { display: flex; align-items: center; justify-content: space-between; color: var(--ink); text-decoration: none; padding: 10px 11px; border: 1px solid transparent; border-radius: 2px; font-size: 13.5px; transition: transform 160ms var(--ease-sketch), background-color 160ms ease, border-color 160ms ease; }
.nav-item:hover { background: rgba(245, 240, 232, .7); border-color: var(--line); transform: translateX(2px); }
.nav-item.active { background: var(--ink); color: var(--paper); border-color: var(--ink); box-shadow: 2px 2px 0 rgba(44, 44, 44, .25); }
.nav-mark { opacity: 0; transition: opacity 160ms ease, transform 160ms ease; }
.nav-item:hover .nav-mark, .nav-item.active .nav-mark { opacity: 1; transform: translateX(2px); }
.privacy-note { display: grid; grid-template-columns: 8px 1fr; gap: 2px 7px; font-size: 10.5px; color: var(--ink-soft); line-height: 1.6; padding: 0 6px; }
.privacy-note span:nth-child(2), .privacy-note span:nth-child(3) { grid-column: 2; }
.privacy-icon { width: 13px; height: 13px; margin-top: 3px; color: var(--sage); }
.content { flex: 1; overflow-y: auto; min-width: 0; padding: 20px clamp(18px, 3vw, 44px) 48px; }
.topbar { display: flex; justify-content: space-between; align-items: center; margin-bottom: 24px; padding-bottom: 12px; border-bottom: 2px dashed var(--line); }
.status { display: flex; align-items: center; gap: 9px; min-width: 0; }
.topbar-kicker { color: var(--ink-soft); font-size: 9px; font-weight: 700; letter-spacing: .14em; }
.meta { font-size: 12px; color: var(--ink-soft); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.view-stack { position: relative; }
@media (max-width: 760px) {
  .app-shell { display: block; height: auto; min-height: 100vh; }
  .sidebar { width: auto; margin: 10px 10px 0; padding: 14px; transform: none; gap: 12px; }
  .sidebar-rule, .privacy-note { display: none; }
  nav { display: grid; grid-template-columns: repeat(3, 1fr); gap: 5px; }
  .nav-item { justify-content: center; padding: 8px 5px; font-size: 12px; }
  .nav-mark { display: none; }
  .content { overflow: visible; padding: 18px 12px 32px; }
  .topbar { align-items: flex-start; gap: 12px; margin-bottom: 18px; }
  .topbar-kicker, .meta { display: none; }
}
</style>
