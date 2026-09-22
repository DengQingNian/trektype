# Windows 编译与打包

项目使用 Tauri 2 打包 Windows 应用，打包配置位于 `src-tauri/tauri.conf.json`。当前配置会先执行前端生产构建，再编译 Rust release 版本，最后生成 NSIS 安装程序。

## 常用命令

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

## NSIS 缓存准备

Tauri 的 NSIS bundler 需要 NSIS 3.11 及 `nsis_tauri_utils.dll`。默认会从 GitHub 下载 `nsis-3.11.zip`；如果下载超时、反复重试或在受限环境中出现“拒绝访问”，按参考构建任务准备本机缓存后再执行打包：

1. 将完整的 NSIS 3.11 目录放到 `%LOCALAPPDATA%\tauri\NSIS`，目录内应包含 `Bin\makensis.exe`、`Include\MUI2.nsh`、`Stubs`、`Plugins` 等文件。
2. 将 Tauri 的 `nsis_tauri_utils.dll` 放到 `Plugins\x86-unicode\additional\nsis_tauri_utils.dll`。
3. 用以下命令确认工具链可执行且版本正确：

```powershell
& "$env:LOCALAPPDATA\tauri\NSIS\Bin\makensis.exe" /VERSION
```

命令应输出 `v3.11`。确认后重新执行 `pnpm tauri build`；必要时使用有权限访问 `%LOCALAPPDATA%\tauri\NSIS` 的终端运行。

### GitHub 下载失败时使用代理

某些网络环境无法稳定访问 GitHub，Tauri 自动下载 NSIS 可能因此超时或失败。此时使用以下代理地址下载原始压缩包，再按上面的目录结构解压和补齐缓存文件：

<https://gh-proxy.org/https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip>

PowerShell 示例：

```powershell
$nsisZip = Join-Path $env:TEMP "nsis-3.11.zip"
Invoke-WebRequest `
  -Uri "https://gh-proxy.org/https://github.com/tauri-apps/binary-releases/releases/download/nsis-3.11/nsis-3.11.zip" `
  -OutFile $nsisZip
```

代理只用于下载，不能替代 NSIS 缓存目录配置；下载完成后仍需将内容放入 `%LOCALAPPDATA%\tauri\NSIS`，并确认 `makensis.exe /VERSION` 输出 `v3.11`。如果代理返回的不是 ZIP 文件，应先删除该文件并检查网络代理响应，再重新下载。

## 构建验证

构建结束后可以用以下命令确认产物存在并查看大小：

```powershell
Get-Item `
  .\src-tauri\target\release\typetrek.exe, `
  .\src-tauri\target\release\bundle\nsis\TypeTrek_0.1.0_x64-setup.exe |
  Select-Object FullName, Length, LastWriteTime
```

本次按上述流程验证于 2026-09-22，`pnpm tauri build` 返回成功，已生成 release 可执行文件和 NSIS 安装包。构建日志中的 `LNK4099`（OpenSSL PDB 缺失）以及 Vite 大 chunk 提示属于警告；只有命令退出码为 0 且两个产物均存在时，才视为打包成功。
