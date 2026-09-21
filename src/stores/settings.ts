import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { api, type AppConfig } from "../lib/ipc";

/**
 * 设置：唯一来源是后端 settings.json（经 get_settings/set_settings 命令）。
 * 前端只缓存副本，任何修改都必须走 save() 落盘，保证暂停状态跨重启一致。
 */
export const useSettingsStore = defineStore("settings", () => {
  const config = ref<AppConfig | null>(null);
  const loading = ref(false);
  const saving = ref(false);

  const consented = computed(() => config.value?.consented_at != null);
  /** 采集中 = 已同意 且 capture_enabled */
  const capturing = computed(() => !!config.value && config.value.consented_at != null && config.value.capture_enabled);

  async function load() {
    loading.value = true;
    try {
      config.value = await api.getSettings();
    } finally {
      loading.value = false;
    }
  }

  async function save(next: AppConfig) {
    saving.value = true;
    try {
      config.value = await api.setSettings(next);
    } finally {
      saving.value = false;
    }
  }

  /** 局部更新（读取当前值 → 合并 → 保存）。 */
  async function patch(partial: Partial<AppConfig>) {
    if (!config.value) await load();
    const next = { ...(config.value as AppConfig), ...partial };
    await save(next);
  }

  async function grantConsent() {
    config.value = await api.grantConsent();
  }

  return { config, loading, saving, consented, capturing, load, save, patch, grantConsent };
});
