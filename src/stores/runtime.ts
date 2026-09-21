import { defineStore } from "pinia";
import { ref } from "vue";
import { api, type RuntimeState } from "../lib/ipc";
import { useSettingsStore } from "./settings";

/** 采集运行时状态：定时轮询 + 暂停/恢复操作。 */
export const useRuntimeStore = defineStore("runtime", () => {
  const state = ref<RuntimeState | null>(null);
  let timer: number | null = null;

  async function refresh() {
    state.value = await api.getRuntimeState();
  }

  /** 开始轮询（5s；仅在统计页面挂载时启动，避免空转）。 */
  function startPolling(intervalMs = 5000) {
    stopPolling();
    void refresh();
    timer = window.setInterval(() => void refresh(), intervalMs);
  }

  function stopPolling() {
    if (timer != null) {
      window.clearInterval(timer);
      timer = null;
    }
  }

  /** 暂停/恢复：走后端持久化，并同步设置 store 的副本。 */
  async function togglePause() {
    const paused = !(state.value?.paused ?? false);
    const cfg = await api.setPaused(paused);
    const settings = useSettingsStore();
    settings.config = cfg;
    await refresh();
  }

  return { state, refresh, startPolling, stopPolling, togglePause };
});
