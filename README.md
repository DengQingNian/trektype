# TypeTrek

本机键盘鼠标行为统计工具（Windows 优先）。**完全离线**：不联网、不上传、不隐藏运行。

> ⚠️ 使用边界：本程序只应用于**你本人拥有或有明确授权使用的设备**，统计你自己的操作习惯。
> 未经他人同意安装或用于监控他人可能违反法律（如《个人信息保护法》）与所在组织政策。

## 它做什么

| 采集 | 不采集（设计上排除） |
|------|---------------------|
| 按键的**键位标识**与按下/释放时间 | 输入的**明文内容**（不做字符合成与还原） |
| 鼠标 5 类按钮（左/右/中/侧键 X1/X2）的按下与坐标 | 鼠标移动轨迹、滚轮 |
| 前台应用 exe 名（用于按应用统计与黑名单） | 窗口标题、剪贴板、屏幕截图 |

统计视图：键盘热力图（104/87 键）、屏幕点击热力图（多显示器）、月历活跃度、Top 键位/区域、概览趋势。

## 隐私与控制

- **首启知情同意**：未点击同意时不会创建任何采集线程（数据库中不会产生记录）。
- **随时暂停**：托盘菜单、主界面按钮或全局快捷键（默认 `Ctrl+Alt+P`）。
- **敏感应用黑名单**：名单内应用的事件在写入数据库**之前**丢弃，应用名也不会进入统计字典；支持 `*` / `?` 通配符。
- **隐私模式**：开启后逐条明细完全不落盘，只保留计数聚合。
- **保留期**：明细默认 90 天自动清理；聚合统计永久保留（可手动删除任意日期区间）。
- **零外联**：程序不含任何网络代码；窗口标题等敏感面直接不实现。
- 数据库位于 `%APPDATA%\com.dengqn.app.typetrek\`（仅当前用户可读），可在设置页一键打开。

## 开发

```bash
pnpm install
pnpm tauri dev          # 开发运行
pnpm test               # 前端单测（vitest）
cd src-tauri && cargo test           # 后端单测
cd src-tauri && cargo run --release --bin bench_events -- 10 10000   # 管线压测
```

> **Windows 构建前置**：数据库加密使用 vendored OpenSSL（从源码编译），需要
> **Strawberry Perl**（Git Bash 自带的 cygwin perl 缺少 `Locale::Maketext::Simple`，
> 会导致 openssl-src 编译失败）与 NASM。若构建时报 openssl 配置失败，请把
> Strawberry Perl 的 bin 目录放到 PATH 最前，例如：
> `PATH="/d/Strawberry/perl/bin:$PATH" cargo build --release`

### 构建安装包

```bash
pnpm tauri build        # 产出 NSIS 安装包（target/release/bundle/nsis/）
```

免安装绿色版：直接复制 `src-tauri/target/release/typetrek.exe`（需系统已有 WebView2 运行时，Win10 1803+ 与 Win11 通常自带）。

### 目录结构

```
src/                     前端（Vue 3 + Pinia + Naive UI + ECharts）
  lib/                   纯函数工具（色阶 / 日期 / 键盘布局 / IPC）+ 单测
  components/            热力图（SVG / Canvas）与通用组件
  routes/                各页面（概览 / 键盘 / 鼠标 / 日历 / 数据 / 设置 / 首启）
src-tauri/src/
  capture/               全局钩子（键盘/鼠标）、键码映射、前台应用、显示器快照、黑名单
  pipeline/              事件管线（黑名单过滤 → 聚合 → 批量写入）
  db/                    连接与迁移、DAO、查询、导出与保留期清理
  commands/, tray.rs, state.rs, config.rs
docs/                    隐私说明与手动验收清单
plans/                   开发计划（进度台账）
```

## 平台支持

| 平台 | 状态 | 说明 |
|------|------|------|
| Windows 10/11 x64 | ✅ 完整支持 | 无需管理员；UAC 提权窗口不记录（系统 UIPI 限制，属预期） |
| macOS | ⏳ 原型（未实现） | 需要"辅助功能"权限；见计划 T28 |
| Linux X11 | ⏳ 原型（未实现） | 需 XRecord 或 input 组权限 |
| Linux Wayland | ❌ 不支持 | 合成器安全模型禁止全局监听 |

## 已知限制

- 管理员权限运行的窗口（提权的任务管理器等）中无法采集按键/点击（Windows UIPI）。
- 锁屏与快速用户切换期间不产生记录。
- 解锁后请在设置页重新授权（当前版本无独立授权按钮，需删除 `settings.json` 后重启）。
- 未签名安装包首次运行会有 SmartScreen 提示。
