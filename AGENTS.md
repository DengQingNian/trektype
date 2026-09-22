# 约定

- 新增方法需要增加对应的单元测试
- Agents生成的plan、spec等说明文档全部用**中文**
- 使用测试驱动开发，单测用例要清晰说明
- 配置文件新增的配置需要有对应的注释说明是什么和用处
- 适当使用skill:sketch-style作为界面风格，skill:animations-collect作为动效

## Windows 编译与打包

项目使用 Tauri 2 打包 Windows 应用，打包配置位于 `src-tauri/tauri.conf.json`。当前配置会先执行前端生产构建，再编译 Rust release 版本，最后生成 NSIS 安装程序。

### 常用命令

在项目根目录 `F:\proj\typetrek` 执行：

```powershell
# 前端类型检查与生产构建
pnpm run build

# 单元测试
pnpm test

# 编译 Rust release 并生成 Windows NSIS 安装包
pnpm tauri build
```

`pnpm tauri build` 成功时应至少生成以下两个文件：

- `src-tauri/target/release/typetrek.exe`：未安装版 release 可执行文件
- `src-tauri/target/release/bundle/nsis/TypeTrek_0.1.0_x64-setup.exe`：Windows x64 安装程序

### NSIS 缓存准备

Tauri 的 NSIS bundler 需要 NSIS 3.11 及 `nsis_tauri_utils.dll`。默认会从 GitHub 下载 `nsis-3.11.zip`；如果下载超时、反复重试或在受限环境中出现“拒绝访问”，按参考构建任务准备本机缓存后再执行打包：

1. 将完整的 NSIS 3.11 目录放到 `%LOCALAPPDATA%\tauri\NSIS`，目录内应包含 `Bin\makensis.exe`、`Include\MUI2.nsh`、`Stubs`、`Plugins` 等文件。
2. 将 Tauri 的 `nsis_tauri_utils.dll` 放到 `Plugins\x86-unicode\additional\nsis_tauri_utils.dll`。
3. 用以下命令确认工具链可执行且版本正确：

```powershell
& "$env:LOCALAPPDATA\tauri\NSIS\Bin\makensis.exe" /VERSION
```

命令应输出 `v3.11`。确认后重新执行 `pnpm tauri build`；必要时使用有权限访问 `%LOCALAPPDATA%\tauri\NSIS` 的终端运行。

### 构建验证

构建结束后可以用以下命令确认产物存在并查看大小：

```powershell
Get-Item `
  .\src-tauri\target\release\typetrek.exe, `
  .\src-tauri\target\release\bundle\nsis\TypeTrek_0.1.0_x64-setup.exe |
  Select-Object FullName, Length, LastWriteTime
```

本次按上述流程验证于 2026-09-22，`pnpm tauri build` 返回成功，已生成 release 可执行文件和 NSIS 安装包。构建日志中的 `LNK4099`（OpenSSL PDB 缺失）以及 Vite 大 chunk 提示属于警告；只有命令退出码为 0 且两个产物均存在时，才视为打包成功。
