// @vitest-environment jsdom
import { mount } from "@vue/test-utils";
import { defineComponent } from "vue";
import { describe, expect, it } from "vitest";
import { NInputNumber, NSelect, NSwitch } from "naive-ui";
import ReminderSettings from "./ReminderSettings.vue";
import type { AppConfig } from "../lib/ipc";

const Card = defineComponent({ template: '<section><slot name="header"/><slot/></section>' });
const Form = defineComponent({ template: '<div><slot/></div>' });
const Item = defineComponent({ props: ["label"], template: '<label>{{ label }}<slot/></label>' });
const Switch = defineComponent({ props: ["value"], emits: ["update:value"], template: '<button type="button" @click="$emit(\'update:value\', !value)">{{ value ? "开" : "关" }}</button>' });
const NumberInput = defineComponent({ props: ["value"], emits: ["update:value"], template: '<input type="number" :value="value" @input="$emit(\'update:value\', Number($event.target.value))" />' });
const Select = defineComponent({ props: ["value", "options"], emits: ["update:value"], template: '<select :value="value" @change="$emit(\'update:value\', Number($event.target.value))"><option v-for="option in options" :key="option.value" :value="option.value">{{ option.label }}</option></select>' });

function config(): AppConfig {
  return {
    consented_at: "2026-09-29T00:00:00+08:00", capture_enabled: true,
    capture_keyboard: true, capture_mouse: true, repeat_counts: false,
    ignore_injected: true, privacy_mode: false, raw_retention_days: 90,
    grid_cell_size: 24, heatmap_palette: "classic", blacklist_keys: [], blacklist_mouse: [],
    pause_hotkey: "", autostart: false, db_encrypted: false, tray_hint_shown: false,
    daily_key_reminder_enabled: false, daily_key_limit: 20_000,
    daily_click_reminder_enabled: false, daily_click_limit: 4_000,
    burst_key_reminder_enabled: false, burst_key_limit: 2_500,
    burst_click_reminder_enabled: false, burst_click_limit: 300,
    burst_window_minutes: 10,
  };
}

function render() {
  return mount(ReminderSettings, { props: { config: config() }, global: { stubs: {
    NCard: Card, NForm: Form, NFormItem: Item, NSwitch: Switch, NInputNumber: NumberInput, NSelect: Select,
  } } });
}

describe("休息提醒设置", () => {
  it("展示四项独立开关、各自阈值和共用时间窗口", () => {
    const wrapper = render();
    for (const id of ["daily-key", "daily-click", "burst-key", "burst-click"]) {
      expect(wrapper.find(`[data-test="${id}-enabled"]`).exists()).toBe(true);
      expect(wrapper.find(`[data-test="${id}-limit"]`).exists()).toBe(true);
    }
    expect(wrapper.find('[data-test="burst-window"]').exists()).toBe(true);
    expect(wrapper.text()).toContain("30 分钟");
  });

  it("开关与有效阈值产生新的配置值，空阈值不覆盖旧值", async () => {
    const wrapper = render();
    await wrapper.find('[data-test="daily-key-enabled"]').trigger("click");
    expect(wrapper.emitted("update:config")?.[0]?.[0]).toMatchObject({ daily_key_reminder_enabled: true });
    wrapper.findAllComponents(NInputNumber)[0].vm.$emit("update:value", 25_000);
    expect(wrapper.emitted("update:config")?.[1]?.[0]).toMatchObject({ daily_key_limit: 25_000 });
    wrapper.findAllComponents(NInputNumber)[0].vm.$emit("update:value", 0);
    expect(wrapper.emitted("update:config")?.length).toBe(2);
  });

  it("四组控件分别更新对应设置，窗口只更新共用分钟数", () => {
    const wrapper = render();
    const latest = () => {
      const updates = wrapper.emitted("update:config") ?? [];
      return updates[updates.length - 1]?.[0];
    };
    const toggles = [
      "daily_key_reminder_enabled", "daily_click_reminder_enabled",
      "burst_key_reminder_enabled", "burst_click_reminder_enabled",
    ] as const;
    const limits = ["daily_key_limit", "daily_click_limit", "burst_key_limit", "burst_click_limit"] as const;
    wrapper.findAllComponents(NSwitch).forEach((control, i) => {
      control.vm.$emit("update:value", true);
      expect(latest()).toMatchObject({ [toggles[i]]: true });
    });
    wrapper.findAllComponents(NInputNumber).forEach((control, i) => {
      control.vm.$emit("update:value", 123 + i);
      expect(latest()).toMatchObject({ [limits[i]]: 123 + i });
    });
    wrapper.findComponent(NSelect).vm.$emit("update:value", 30);
    expect(latest()).toMatchObject({ burst_window_minutes: 30 });
  });
});
