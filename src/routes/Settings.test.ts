// @vitest-environment jsdom
import { mount } from "@vue/test-utils";
import { nextTick } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Settings from "./Settings.vue";
import ReminderSettings from "../components/ReminderSettings.vue";

const { getAppVersion, getKnownApps, openUrl, toastError, save } = vi.hoisted(() => ({
  getAppVersion: vi.fn(),
  getKnownApps: vi.fn(),
  openUrl: vi.fn(),
  toastError: vi.fn(),
  save: vi.fn(),
}));

vi.mock("../stores/settings", () => ({
  useSettingsStore: () => ({
    config: {
      capture_enabled: true, blacklist_keys: [], blacklist_mouse: [],
      daily_key_reminder_enabled: false, daily_key_limit: 20_000,
      daily_click_reminder_enabled: false, daily_click_limit: 4_000,
      burst_key_reminder_enabled: false, burst_key_limit: 2_500,
      burst_click_reminder_enabled: false, burst_click_limit: 300,
      burst_window_minutes: 10,
    },
    load: vi.fn(),
    save,
  }),
}));
vi.mock("../lib/ipc", () => ({ api: { getKnownApps, getAppVersion } }));
vi.mock("../lib/ui", () => ({ message: { success: vi.fn() }, toastError }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl }));

describe("设置页关于区域", () => {
  const mountSettings = () => mount(Settings);

  beforeEach(() => {
    vi.clearAllMocks();
    getKnownApps.mockResolvedValue([]);
    getAppVersion.mockResolvedValue("0.2.0");
    openUrl.mockResolvedValue(undefined);
  });

  it("显示在最后，展示应用版本和项目说明", async () => {
    const wrapper = mountSettings();
    await vi.waitFor(() => expect(wrapper.text()).toContain("0.2.0"));
    const cards = wrapper.findAll(".n-card");
    const aboutCard = cards[cards.length - 1];
    expect(aboutCard.text()).toContain("关于");
    expect(aboutCard.text()).toContain("TypeTrek");
    expect(aboutCard.text()).toContain("本机");
    expect(aboutCard.text()).toContain("GitHub");
  });

  it("点击 GitHub 按钮后通过系统浏览器打开指定仓库", async () => {
    const wrapper = mountSettings();
    await vi.waitFor(() => expect(wrapper.text()).toContain("0.2.0"));

    await wrapper.find('[data-testid="github-button"]').trigger("click");

    expect(openUrl).toHaveBeenCalledExactlyOnceWith("https://github.com/DengQingNian/trektype");
  });

  it("浏览器打开失败时提示错误", async () => {
    openUrl.mockRejectedValueOnce(new Error("failed"));
    const wrapper = mountSettings();
    await vi.waitFor(() => expect(wrapper.text()).toContain("0.2.0"));

    await wrapper.find('[data-testid="github-button"]').trigger("click");
    await vi.waitFor(() => expect(toastError).toHaveBeenCalledWith(expect.any(Error), "打开 GitHub 失败"));
  });

  it("提醒设置变更通过原有 500ms 防抖流程自动保存", async () => {
    const wrapper = mountSettings();
    await vi.waitFor(() => expect(wrapper.text()).toContain("0.2.0"));
    const card = wrapper.findComponent(ReminderSettings);
    expect(card.exists()).toBe(true);
    expect(save).not.toHaveBeenCalled();
    vi.useFakeTimers();
    try {
      card.vm.$emit("update:config", { ...card.props("config"), daily_key_reminder_enabled: true });
      await nextTick();
      expect(save).not.toHaveBeenCalled();
      await vi.advanceTimersByTimeAsync(500);
      expect(save).toHaveBeenCalledTimes(1);
      expect(save).toHaveBeenCalledWith(expect.objectContaining({ daily_key_reminder_enabled: true, daily_key_limit: 20_000 }));
    } finally {
      vi.useRealTimers();
      wrapper.unmount();
    }
  });
});
