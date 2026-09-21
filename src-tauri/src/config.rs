//! 应用配置：持久化到 `settings.json`（与数据库文件分离——即使加密库打不开，设置仍可读写）。
//!
//! 遵守 AGENTS.md：每个配置项都带注释，说明用途与默认值理由。

use serde::{Deserialize, Serialize};
use std::path::Path;

/// 热力图网格允许的粒度（px）。聚合按 24px 基准存储，查询层只支持向更粗粒度合并。
pub const ALLOWED_GRID_CELLS: [u32; 3] = [24, 32, 64];
/// 默认 raw 明细保留天数（聚合永久保留）。
pub const DEFAULT_RETENTION_DAYS: u32 = 90;
/// 保留期上限（约 10 年）。
pub const MAX_RETENTION_DAYS: u32 = 3650;
/// 默认暂停/恢复快捷键。
pub const DEFAULT_PAUSE_HOTKEY: &str = "Ctrl+Alt+P";

/// 应用配置（对应 `settings.json`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    /// 首启知情同意时间（RFC3339 字符串）；`None` = 未同意，采集线程不会被创建
    pub consented_at: Option<String>,
    /// 采集总开关；关闭即"暂停"（钩子线程保留，事件在回调最前端丢弃）
    pub capture_enabled: bool,
    /// 键盘采集开关
    pub capture_keyboard: bool,
    /// 鼠标采集开关
    pub capture_mouse: bool,
    /// 统计口径：是否把 OS 自动重复计入按键总数（仅影响查询，已存数据不变）
    pub repeat_counts: bool,
    /// 忽略软件注入事件（AutoHotkey 等自动化用户可关闭，以统计脚本产生的输入）
    pub ignore_injected: bool,
    /// 隐私模式：raw 明细完全不落库（仅写聚合），代价是无法导出/查看明细
    pub privacy_mode: bool,
    /// raw 明细保留天数（到期自动清理；聚合永久保留）
    pub raw_retention_days: u32,
    /// 热力图网格粒度（px，仅允许 ALLOWED_GRID_CELLS）
    pub grid_cell_size: u32,
    /// 键盘黑名单（exe 名，支持 `*` / `?` 通配）
    pub blacklist_keys: Vec<String>,
    /// 鼠标黑名单（exe 名，支持 `*` / `?` 通配）
    pub blacklist_mouse: Vec<String>,
    /// 暂停/恢复全局快捷键（空字符串 = 禁用快捷键）
    pub pause_hotkey: String,
    /// 开机自启（默认关，需用户显式开启）
    pub autostart: bool,
    /// 数据库加密（SQLCipher）；默认关，切换需重启应用
    pub db_encrypted: bool,
    /// "已最小化到托盘，仍在采集中"气泡是否提示过（仅首次提示，避免打扰）
    pub tray_hint_shown: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            consented_at: None,
            // 同意后默认开始采集；隐私相关默认值一律保守
            capture_enabled: true,
            capture_keyboard: true,
            capture_mouse: true,
            repeat_counts: false,
            ignore_injected: true,
            privacy_mode: false,
            raw_retention_days: DEFAULT_RETENTION_DAYS,
            grid_cell_size: 24,
            blacklist_keys: Vec::new(),
            blacklist_mouse: Vec::new(),
            pause_hotkey: DEFAULT_PAUSE_HOTKEY.to_string(),
            autostart: false,
            db_encrypted: false,
            tray_hint_shown: false,
        }
    }
}

impl AppConfig {
    /// 是否已完成知情同意（未同意时不得启动采集）。
    pub fn consented(&self) -> bool {
        self.consented_at.is_some()
    }

    /// 读取配置。文件不存在返回默认值；解析失败时把损坏文件改名为 `.bad` 后返回默认值
    /// （保留现场便于排查，同时保证应用可启动）。
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str::<AppConfig>(&text) {
                Ok(cfg) => cfg.normalized(),
                Err(e) => {
                    eprintln!("[typetrek] 配置文件解析失败，回退默认值：{e}");
                    let _ = std::fs::rename(path, path.with_extension("json.bad"));
                    AppConfig::default()
                }
            },
            Err(_) => AppConfig::default(),
        }
    }

    /// 原子写入：先写临时文件再 rename，避免写入中途断电导致配置损坏。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)
    }

    /// 归一化非法值（用户手改配置、旧版本配置升级时兜底）。
    pub fn normalized(mut self) -> Self {
        if !ALLOWED_GRID_CELLS.contains(&self.grid_cell_size) {
            self.grid_cell_size = 24;
        }
        self.raw_retention_days = self.raw_retention_days.clamp(1, MAX_RETENTION_DAYS);
        self.blacklist_keys.retain(|s| !s.trim().is_empty());
        self.blacklist_mouse.retain(|s| !s.trim().is_empty());
        self.pause_hotkey = self.pause_hotkey.trim().to_string();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "typetrek_cfg_{tag}_{}.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// 默认值必须保守：未同意、不加密、不记录 repeat、隐私模式关、自启关。
    #[test]
    fn defaults_are_conservative() {
        let c = AppConfig::default();
        assert!(!c.consented(), "默认未同意，不得启动采集");
        assert!(!c.repeat_counts, "默认排除自动重复");
        assert!(c.ignore_injected, "默认过滤注入事件");
        assert!(!c.privacy_mode);
        assert!(!c.db_encrypted);
        assert!(!c.autostart);
        assert_eq!(c.raw_retention_days, DEFAULT_RETENTION_DAYS);
        assert_eq!(c.grid_cell_size, 24);
        assert!(c.blacklist_keys.is_empty());
    }

    /// 保存后可完整读回（含各字段）。
    #[test]
    fn save_load_roundtrip() {
        let path = temp_path("roundtrip");
        let c = AppConfig {
            consented_at: Some("2026-09-21T12:00:00+08:00".to_string()),
            blacklist_keys: vec!["keepass*".to_string()],
            repeat_counts: true,
            raw_retention_days: 30,
            ..AppConfig::default()
        };
        c.save(&path).unwrap();

        let loaded = AppConfig::load(&path);
        assert_eq!(loaded, c);
        assert!(loaded.consented());
        let _ = std::fs::remove_file(&path);
    }

    /// 损坏的配置文件：回退默认值并把原文件改名为 .bad，保证应用仍可启动。
    #[test]
    fn corrupted_file_falls_back_and_is_quarantined() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "{ this is not json").unwrap();

        let loaded = AppConfig::load(&path);
        assert_eq!(loaded, AppConfig::default());
        assert!(!path.exists(), "原文件应被移走");
        assert!(
            path.with_extension("json.bad").exists(),
            "损坏文件应保留为 .bad"
        );
        let _ = std::fs::remove_file(path.with_extension("json.bad"));
    }

    /// 文件不存在：直接默认值，不创建任何文件。
    #[test]
    fn missing_file_uses_defaults() {
        let path = temp_path("missing");
        assert_eq!(AppConfig::load(&path), AppConfig::default());
        assert!(!path.exists());
    }

    /// 归一化：非法网格粒度回退 24；保留期钳制到 [1, MAX]；空白黑名单项清除。
    #[test]
    fn normalized_fixes_invalid_values() {
        let raw = r#"{
            "grid_cell_size": 16,
            "raw_retention_days": 0,
            "blacklist_keys": ["keepass*", "  ", ""],
            "pause_hotkey": "  "
        }"#;
        let c: AppConfig = serde_json::from_str(raw).unwrap();
        let c = c.normalized();
        assert_eq!(c.grid_cell_size, 24, "不支持的粒度应回退 24");
        assert_eq!(c.raw_retention_days, 1, "下限 1 天");
        assert_eq!(
            c.blacklist_keys,
            vec!["keepass*".to_string()],
            "空白项应清除"
        );
        assert_eq!(c.pause_hotkey, "", "空白快捷键归一为禁用");

        let too_big = AppConfig {
            raw_retention_days: 99_999,
            grid_cell_size: 64,
            ..AppConfig::default()
        }
        .normalized();
        assert_eq!(too_big.raw_retention_days, MAX_RETENTION_DAYS);
        assert_eq!(too_big.grid_cell_size, 64, "合法粒度保留");
    }

    /// 未知字段（未来版本新增配置）不导致解析失败——向前兼容。
    #[test]
    fn unknown_fields_are_ignored() {
        let raw = r#"{ "capture_enabled": false, "some_future_option": 42 }"#;
        let c: AppConfig = serde_json::from_str(raw).unwrap();
        assert!(!c.capture_enabled);
        assert_eq!(c.grid_cell_size, 24, "缺失字段用默认值");
    }
}
