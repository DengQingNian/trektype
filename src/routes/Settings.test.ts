// @vitest-environment jsdom
import { mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Settings from "./Settings.vue";

const { getAppVersion, getKnownApps, openUrl, toastError } = vi.hoisted(() => ({
  getAppVersion: vi.fn(),
  getKnownApps: vi.fn(),
  openUrl: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock("../stores/settings", () => ({
  useSettingsStore: () => ({
    config: { capture_enabled: true, blacklist_keys: [], blacklist_mouse: [] },
    load: vi.fn(),
    save: vi.fn(),
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
    getAppVersion.mockResolvedValue("0.1.3");
    openUrl.mockResolvedValue(undefined);
  });

  it("显示在最后，展示应用版本和项目说明", async () => {
    const wrapper = mountSettings();
    await vi.waitFor(() => expect(wrapper.text()).toContain("0.1.3"));
    const cards = wrapper.findAll(".n-card");
    const aboutCard = cards[cards.length - 1];
    expect(aboutCard.text()).toContain("关于");
    expect(aboutCard.text()).toContain("TypeTrek");
    expect(aboutCard.text()).toContain("本机");
    expect(aboutCard.text()).toContain("GitHub");
  });

  it("点击 GitHub 按钮后通过系统浏览器打开指定仓库", async () => {
    const wrapper = mountSettings();
    await vi.waitFor(() => expect(wrapper.text()).toContain("0.1.3"));

    await wrapper.find('[data-testid="github-button"]').trigger("click");

    expect(openUrl).toHaveBeenCalledExactlyOnceWith("https://github.com/DengQingNian/trektype");
  });

  it("浏览器打开失败时提示错误", async () => {
    openUrl.mockRejectedValueOnce(new Error("failed"));
    const wrapper = mountSettings();
    await vi.waitFor(() => expect(wrapper.text()).toContain("0.1.3"));

    await wrapper.find('[data-testid="github-button"]').trigger("click");
    await vi.waitFor(() => expect(toastError).toHaveBeenCalledWith(expect.any(Error), "打开 GitHub 失败"));
  });
});
