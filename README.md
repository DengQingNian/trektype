<div align="center">
  <img src="public/typetrek-icon.svg" width="96" height="96" alt="TypeTrek 图标" />

  <h1>TypeTrek</h1>

  <p>面向 Windows 的本地键盘与鼠标行为统计工具</p>

  <p>
    <img src="https://img.shields.io/badge/platform-Windows%2010%2F11-0078D6?style=flat-square" alt="Windows 10/11" />
    <img src="https://img.shields.io/badge/Tauri-2-24C8DB?style=flat-square" alt="Tauri 2" />
    <img src="https://img.shields.io/badge/status-early%20development-orange?style=flat-square" alt="Early development" />
  </p>
</div>

TypeTrek 用于帮助你回顾自己的键盘与鼠标使用习惯：它在本机记录键位、点击和前台应用进程名，并将数据转换为键盘热力图、屏幕点击热力图、日历活跃度和趋势统计。

**完全离线运行：不联网、不上传、不隐藏运行。** 首次启动必须由用户明确同意，采集过程中也可以随时暂停。

> ⚠️ **使用边界**
>
> 仅在你本人拥有或已获得明确授权的设备上使用。未经他人同意安装或用于监控他人，可能违反适用法律与所在组织政策。

## 目录

- [功能概览](#功能概览)
- [隐私与数据控制](#隐私与数据控制)
- [安装与运行](#安装与运行)
- [GitHub 自动构建与发布](#github-自动构建与发布)
- [从源码构建](#从源码构建)
- [项目结构](#项目结构)
- [平台支持](#平台支持)
- [已知限制](#已知限制)
- [参与贡献](#参与贡献)

## 功能概览

| 模块 | 能做什么 |
| --- | --- |
| 键盘统计 | 支持 104 键与 87 键（TKL）布局，查看键位热力图、Top 键位、区域占比和趋势 |
| 鼠标统计 | 统计左键、右键、中键、侧键 X1/X2，生成多显示器屏幕点击热力图和 Top 区域 |
| 日历与概览 | 按天查看活跃度、键盘/鼠标总量和时间趋势 |
| 数据管理 | 导出统计聚合或原始明细（CSV/JSON），按日期删除，清理过期明细 |
| 采集控制 | 托盘菜单、主界面按钮和全局快捷键暂停/恢复；键盘与鼠标可独立开关 |
| 敏感应用保护 | 按 exe 名配置黑名单，支持 `*` 和 `?` 通配符，命中后在写入数据库前丢弃事件 |
| 本地存储 | SQLite 本地数据库；可选 SQLCipher 整库加密，密钥保存于 Windows 凭据管理器 |

## 隐私与数据控制

### 采集什么

- 键盘：键位标识（如 `KeyA`、`Space`）以及按下/释放时间；
- 鼠标：五类按钮的按下事件、屏幕坐标和显示器信息；
- 前台应用：进程可执行文件名（如 `chrome.exe`），用于按应用统计与黑名单判断。

### 设计上不采集什么

- 不记录输入的明文内容，不做字符映射或按键序列还原；
- 不采集窗口标题、剪贴板、屏幕截图、音频或摄像头；
- 不采集鼠标移动轨迹和滚轮事件；
- 不向网络发送数据，断网时也可以完整运行。

### 可用的控制手段

- **首启知情同意**：未同意时不创建采集线程，也不会产生采集记录；
- **随时暂停**：默认快捷键为 `Ctrl+Alt+P`，也可以在设置页修改或禁用；
- **隐私模式**：不写入逐条明细，只保留计数聚合；
- **保留期**：原始明细默认保留 90 天，可设置为 30/90/180/365 天；统计聚合不会随保留期清理；
- **按日期删除**：从数据管理页删除指定日期范围的明细和统计；
- **敏感应用黑名单**：事件在写入数据库前丢弃，命中的应用名也不会进入统计字典。

数据目录默认为：

```text
%APPDATA%\com.dengqn.app.typetrek\
```

更完整的采集字段、存储方式、残余风险和彻底删除步骤，请阅读[隐私说明](docs/privacy.md)。

## 安装与运行

当前项目处于早期开发阶段，仓库暂未提供签名的 GitHub Release 安装包。你可以直接从源码运行，或按下方说明构建 Windows 安装包。

### 环境要求

- Windows 10/11 x64；
- [Node.js](https://nodejs.org/) 与 [pnpm](https://pnpm.io/)；
- [Rust](https://www.rust-lang.org/tools/install) stable toolchain；
- Visual Studio Build Tools 的 MSVC 编译工具与 Windows SDK；
- WebView2 Runtime（Windows 10 1803+ 和 Windows 11 通常已预装）。

### 开发运行

```bash
pnpm install
pnpm tauri dev
```

首次启动后，TypeTrek 会显示知情同意页。选择“我已知情并同意，开始统计”后才会启动键盘和鼠标采集；选择“暂不同意”仍可浏览界面，但不会创建采集线程。

### 构建安装包

```bash
pnpm install
pnpm tauri build
```

构建成功后，NSIS 安装包位于：

```text
src-tauri/target/release/bundle/nsis/
```

## GitHub 自动构建与发布

仓库内的 `.github/workflows/build-windows.yml` 会在 PR、`master`/`main` 分支推送和手动运行时执行前端测试、Rust 检查并构建 Windows x64 安装包。运行完成后，可以在对应的 GitHub Actions 运行页面下载 `TypeTrek-Windows-*` 构建产物，其中包含 NSIS 安装包、绿色版 ZIP 和 `SHA256SUMS.txt`。

发布正式版本时，先确保 `package.json` 与 `src-tauri/tauri.conf.json` 的版本一致，再创建并推送同名的 `v` 标签。例如当前版本：

```bash
git tag v0.1.0
git push origin v0.1.0
```

推送标签后，工作流会自动创建 GitHub Release，上传 NSIS 安装包、绿色版 ZIP 和 SHA-256 校验文件。标签必须严格匹配 Tauri 配置中的版本号（例如 `v0.1.0`）；如果只想验证构建而不发布版本，请使用分支推送、PR 或 Actions 页面中的 **Run workflow**。

Windows 构建使用 vendored OpenSSL 从源码编译 SQLCipher 依赖，因此还需要安装 [Strawberry Perl](https://strawberryperl.com/) 和 [NASM](https://www.nasm.us/)。如果 `openssl-src` 报配置或编译错误，请确保 Strawberry Perl 的 `bin` 目录位于 `PATH` 前部。

## 从源码构建

### 前端测试与构建

```bash
pnpm test                 # Vitest 单元测试
pnpm run build            # TypeScript 检查 + Vite 构建
pnpm run test:watch       # 监听模式运行前端测试
```

### Rust 后端测试与压测

```bash
cd src-tauri
cargo test
cargo run --release --bin bench_events -- 10 10000
```

压测命令的两个参数分别是持续时间（秒）和每批事件数量，可按需要调整。

## 项目结构

```text
src/
├─ components/       热力图与通用 UI 组件
├─ lib/              色阶、日期、键盘布局、趋势等纯函数与单测
├─ routes/           概览、键盘、鼠标、日历、数据、设置、首启页面
├─ stores/           Pinia 状态
└─ router/           Vue Router 路由

src-tauri/src/
├─ capture/          键盘/鼠标钩子、前台应用、显示器、黑名单
├─ pipeline/         黑名单过滤、聚合、批量写入
├─ db/               数据库、迁移、查询、导出、保留期清理
├─ commands/         前后端 IPC 命令
└─ tray.rs           系统托盘

docs/
├─ privacy.md        隐私说明
└─ manual-qa.md      真机手动验收清单
```

主要技术栈：Vue 3、TypeScript、Vite、Pinia、Naive UI、ECharts、Tauri 2、Rust、SQLite/SQLCipher。

## 平台支持

| 平台 | 状态 | 说明 |
| --- | --- | --- |
| Windows 10/11 x64 | ✅ 当前支持 | 无需管理员权限；UAC 提权窗口受 Windows UIPI 限制，属于预期行为 |
| macOS | ⏳ 尚未实现 | 需要辅助功能权限，暂不提供可用版本 |
| Linux X11 | ⏳ 尚未实现 | 需要额外的全局输入监听能力与权限 |
| Linux Wayland | ❌ 不支持 | Wayland 安全模型通常禁止此类全局监听 |

## 已知限制

- 以管理员身份运行的窗口中无法采集按键和点击，这是 Windows UIPI 的系统限制；
- 锁屏和快速用户切换期间不会产生记录；
- 当前版本没有独立的“重新授权”按钮；如需重新显示首启授权页，需要删除 `settings.json` 后重启；
- 未签名安装包首次运行可能触发 Windows SmartScreen 提示；
- 开启 SQLCipher 加密只对新建数据库生效。切换前请先导出数据、清空数据库文件，再重启应用；密钥丢失后无法恢复数据库。

真实系统钩子、多显示器、托盘和 UAC 相关行为请参考[手动验收清单](docs/manual-qa.md)。

## 参与贡献

欢迎通过 Issue 反馈问题或提交 Pull Request。提交涉及采集、隐私、数据库或跨平台行为的改动时，请同时说明：

- 变更范围与潜在隐私影响；
- 对应的自动化测试或手动验收步骤；
- Windows 版本、是否多显示器以及复现条件（如适用）。

新增后端方法应补充对应单元测试；涉及真实系统行为的改动，请同步更新手动验收清单。

## 开源说明

本仓库当前未包含许可证文件。若计划在 GitHub 上公开分发，请在发布前补充合适的 `LICENSE`，并根据许可证要求完善第三方依赖声明。
