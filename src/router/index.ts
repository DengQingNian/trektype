import { createRouter, createWebHashHistory } from "vue-router";
import { useSettingsStore } from "../stores/settings";

/**
 * 路由：哈希模式（Tauri 生产构建下 file:// 加载，hash 最稳）。
 * 守卫：未完成知情同意时强制进入 /onboarding——**任何统计页面都不可达**，
 * 保证"未同意不采集"的同时也避免用户困惑（页面全空）。
 */
export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/dashboard" },
    {
      path: "/onboarding",
      name: "onboarding",
      component: () => import("../routes/Onboarding.vue"),
      meta: { title: "知情同意" },
    },
    {
      path: "/dashboard",
      name: "dashboard",
      component: () => import("../routes/Dashboard.vue"),
      meta: { title: "概览" },
    },
    {
      path: "/keyboard",
      name: "keyboard",
      component: () => import("../routes/Keyboard.vue"),
      meta: { title: "键盘热力图" },
    },
    {
      path: "/mouse",
      name: "mouse",
      component: () => import("../routes/Mouse.vue"),
      meta: { title: "鼠标热力图" },
    },
    {
      path: "/calendar",
      name: "calendar",
      component: () => import("../routes/Calendar.vue"),
      meta: { title: "日历统计" },
    },
    {
      path: "/data",
      name: "data",
      component: () => import("../routes/Data.vue"),
      meta: { title: "数据管理" },
    },
    {
      path: "/settings",
      name: "settings",
      component: () => import("../routes/Settings.vue"),
      meta: { title: "设置" },
    },
  ],
});

router.beforeEach(async (to) => {
  const settings = useSettingsStore();
  if (!settings.config) {
    await settings.load();
  }
  if (!settings.consented && to.name !== "onboarding") {
    return { name: "onboarding" };
  }
  return true;
});
