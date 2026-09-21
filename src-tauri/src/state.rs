//! 应用运行时状态：配置、采集共享状态与命令层数据库连接。

use crate::capture::blacklist::Blacklist;
use crate::capture::CaptureShared;
use crate::config::AppConfig;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, RwLock};

/// Tauri `manage` 的全局状态。
pub struct AppState {
    /// 采集侧共享状态（钩子线程、写入器、监视线程共用）
    pub shared: Arc<CaptureShared>,
    /// 当前配置（内存副本；写入必须走 `apply_config` 以保证落盘 + 同步采集侧）
    config: RwLock<AppConfig>,
    /// `settings.json` 路径
    pub settings_path: PathBuf,
    /// 数据库文件路径（数据管理页展示、加密切换用）
    pub db_path: PathBuf,
    /// 命令层数据库连接（WAL 下与写入器并发；Mutex 串行化查询）
    pub db: Mutex<Connection>,
}

impl AppState {
    /// 构造并完成一次配置同步（保证采集侧原子量与配置一致）。
    pub fn new(
        shared: Arc<CaptureShared>,
        config: AppConfig,
        settings_path: PathBuf,
        db_path: PathBuf,
        db: Connection,
    ) -> Self {
        let s = Self {
            shared,
            config: RwLock::new(config.clone()),
            settings_path,
            db_path,
            db: Mutex::new(db),
        };
        s.sync_shared(&config);
        s
    }

    /// 当前配置快照。
    pub fn config(&self) -> AppConfig {
        self.config
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 唯一的配置写入口：归一化 → 落盘 → 同步采集侧 → 更新内存副本。
    pub fn apply_config(&self, new_config: AppConfig) -> Result<AppConfig, String> {
        let cfg = new_config.normalized();
        cfg.save(&self.settings_path)
            .map_err(|e| format!("配置保存失败：{e}"))?;
        self.sync_shared(&cfg);
        *self.config.write().unwrap_or_else(|e| e.into_inner()) = cfg.clone();
        Ok(cfg)
    }

    /// 把配置推送到采集侧原子量（无锁热路径）。
    pub fn sync_shared(&self, cfg: &AppConfig) {
        // 采集总开关 → paused：暂停即关闭采集（语义统一，状态跨重启保留）
        self.shared
            .paused
            .store(!cfg.capture_enabled, Ordering::SeqCst);
        self.shared
            .capture_keyboard
            .store(cfg.capture_keyboard, Ordering::SeqCst);
        self.shared
            .capture_mouse
            .store(cfg.capture_mouse, Ordering::SeqCst);
        self.shared
            .ignore_injected
            .store(cfg.ignore_injected, Ordering::SeqCst);
        self.shared
            .privacy_mode
            .store(cfg.privacy_mode, Ordering::SeqCst);
        self.shared.set_blacklist(Blacklist {
            keys: cfg.blacklist_keys.clone(),
            mouse: cfg.blacklist_mouse.clone(),
        });
    }

    /// 便捷访问数据库（Mutex 中毒时仍可用，不让一次 panic 拖垮整个应用）。
    pub fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }
}
