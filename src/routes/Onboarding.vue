<script setup lang="ts">
/**
 * 首启知情同意页（全屏）。
 *
 * 合规要求：用户必须显式点击"我已知情并同意"才会启动采集；
 * 选择"暂不同意"则进入界面但采集保持关闭（随时可在设置页重新授权）。
 */
import { NButton, NCard } from "naive-ui";
import { CheckmarkFilled, Information, Locked, Pause, Security } from "@vicons/carbon";
import { ref } from "vue";
import { useRouter } from "vue-router";
import { useSettingsStore } from "../stores/settings";
import { toastError } from "../lib/ui";

const settings = useSettingsStore();
const router = useRouter();
const submitting = ref(false);

async function agree() {
  submitting.value = true;
  try {
    await settings.grantConsent();
    await router.replace({ name: "dashboard" });
  } catch (e) {
    toastError(e, "授权失败");
  } finally {
    submitting.value = false;
  }
}

async function decline() {
  // 仅浏览界面：不写同意时间，采集线程不会创建
  await router.replace({ name: "settings" });
}
</script>

<template>
  <div class="onboarding">
    <n-card class="card">
      <template #header><span class="card-title"><Information class="ui-icon card-title-icon" />使用前请阅读：这是一个什么程序</span></template>
      <div class="intro">
        <span class="eyebrow">WELCOME / PRIVACY FIRST</span>
        TypeTrek 会<b>在本机</b>记录你的键盘与鼠标操作，用于个人效率统计。它不是监控软件：
        只有你本人知情同意后才会开始采集，数据只存在这台电脑上。
      </div>

      <section>
        <h3><CheckmarkFilled class="ui-icon section-icon" />会记录什么</h3>
        <ul>
          <li>按键的<b>键位标识</b>（如 KeyA、Space）与按下时间 —— 用于键盘热力图；</li>
          <li>鼠标<b>点击</b>的按钮（左/右/中/侧键）与屏幕坐标 —— 用于点击热力图；</li>
          <li>事件发生时的前台应用名（如 chrome.exe）—— 用于按应用统计与黑名单排除。</li>
        </ul>
      </section>

      <section>
        <h3><Locked class="ui-icon section-icon" />不会记录什么</h3>
        <ul>
          <li><b>绝不保存你输入的明文内容</b>：不记录字符组合、不做输入还原；</li>
          <li>不采集窗口标题、剪贴板、屏幕截图，不采集鼠标移动轨迹与滚轮；</li>
          <li><b>不上传任何数据</b>：程序完全离线运行，没有网络功能。</li>
        </ul>
      </section>

      <section>
        <h3><Pause class="ui-icon section-icon" />如何暂停与删除</h3>
        <ul>
          <li>托盘菜单、主界面按钮或快捷键 <b>Ctrl+Alt+P</b> 可随时暂停/恢复采集；</li>
          <li>数据管理页可导出、按日期删除；原始明细默认保留 90 天后自动清理（统计数据保留）；</li>
          <li>隐私模式开启后，逐条明细完全不落盘，只保留计数统计。</li>
        </ul>
      </section>

      <section>
        <h3><Security class="ui-icon section-icon" />数据存在哪里</h3>
        <ul>
          <li>本机应用数据目录下的 SQLite 文件（设置页可一键打开目录）；</li>
          <li>仅当前 Windows 用户可读；可选择开启数据库加密。</li>
        </ul>
      </section>

      <n-card class="notice" size="small">
        <b><Security class="ui-icon section-icon" />使用边界</b>：本程序仅供你在自己拥有或有权使用的设备上统计自己的操作习惯。
        未经他人同意在其设备上安装、或用于监控他人，可能违反法律（如《个人信息保护法》）与组织政策。
        关闭窗口程序仍会在托盘继续运行 —— 这是为了让统计连续，托盘图标始终可见，绝不隐蔽运行。
      </n-card>

      <div class="actions">
        <n-button size="large" :loading="submitting" type="primary" @click="agree">
          <template #icon><CheckmarkFilled class="button-icon" /></template>
          我已知情并同意，开始统计
        </n-button>
        <n-button size="large" quaternary @click="decline"><template #icon><Pause class="button-icon" /></template>暂不同意（仅浏览界面）</n-button>
      </div>
    </n-card>
  </div>
</template>

<style scoped>
.onboarding {
  min-height: 100vh;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--paper);
  padding: 28px 16px;
}
.card {
  max-width: 720px;
  width: 100%;
  transform: rotate(-0.25deg);
}
.intro {
  font-size: 13.5px;
  line-height: 1.75;
  color: var(--ink);
  margin-bottom: 8px;
}
.eyebrow {
  display: block;
  margin-bottom: 8px;
  color: var(--rust);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.16em;
}
section {
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px dashed var(--line);
}
h3 {
  font-size: 13.5px;
  margin: 0 0 4px;
  color: var(--ink);
  font-family: Georgia, "Times New Roman", "Microsoft YaHei", serif;
}
ul {
  margin: 0;
  padding-left: 20px;
  font-size: 13px;
  line-height: 1.8;
  color: var(--ink-soft);
}
.notice {
  margin-top: 14px;
  background: #eee3b9 !important;
  font-size: 12.5px;
  line-height: 1.7;
}
.actions {
  display: flex;
  gap: 12px;
  margin-top: 18px;
  justify-content: flex-end;
}

@media (max-width: 640px) {
  .onboarding {
    align-items: flex-start;
    padding: 14px 10px;
  }
  .card {
    transform: none;
  }
  .actions {
    flex-direction: column;
  }
}
</style>
