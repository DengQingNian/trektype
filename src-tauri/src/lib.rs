//! TypeTrek 后端入口：数据层、采集、管线、托盘与 Tauri 命令的装配。
//!
//! 启动顺序（重要）：
//! 1. 路径与配置 → 2. 数据库（迁移 + 保留期清理）→ 3. 共享状态与写入器
//! 4. 托盘 → 5. **仅在已知情同意时**启动采集线程 → 6. 快捷键。
//!
//! 合规保证：未同意（`consented_at` 为空）时不会创建任何钩子线程。

pub mod capture;
pub mod commands;
pub mod config;
pub mod db;
pub mod pipeline;
pub mod state;
pub mod tray;

use capture::{CaptureShared, QUEUE_CAPACITY};
use config::AppConfig;
use state::AppState;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例必须最先注册：第二次启动时聚焦已有窗口而不是开新进程
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            tray::show_main_window(app);
        }))
        // 开机自启（默认关闭，设置页可开；实际启用状态由 config.autostart 驱动）
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        // 全局快捷键：默认 Ctrl+Alt+P 切换暂停/恢复
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let state = app.state::<AppState>();
                        let mut cfg = state.config();
                        cfg.capture_enabled = !cfg.capture_enabled;
                        if let Err(e) = state.apply_config(cfg) {
                            eprintln!("[typetrek] 快捷键切换暂停失败：{e}");
                        } else {
                            tray::refresh(app);
                        }
                    }
                })
                .build(),
        )
        // 导出文件保存对话框（Rust 侧调用，前端无需该插件权限）
        .plugin(tauri_plugin_dialog::init())
        // 窗口隐藏到托盘时的首次气泡提示
        .plugin(tauri_plugin_notification::init())
        // 打开数据目录
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // ---------- 路径与配置 ----------
            let data_dir = handle.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let settings_path = data_dir.join("settings.json");
            let db_path = data_dir.join("typetrek.db");
            let cfg = AppConfig::load(&settings_path);

            // ---------- 数据层与共享状态 ----------
            // 按配置打开数据库（可选 SQLCipher 加密；模式不匹配时启动失败并给出可操作提示）
            let mut conn = db::crypto::open_maybe_encrypted(&db_path, cfg.db_encrypted)?;
            db::configure(&conn)?;
            db::migrations::migrate(&conn)?;
            // 启动时按保留期清理一次过期 raw（聚合永久保留）
            match db::export::cleanup_expired(&mut conn, cfg.raw_retention_days) {
                Ok(0) => {}
                Ok(n) => eprintln!("[typetrek] 启动清理：删除 {n} 条过期明细"),
                Err(e) => eprintln!("[typetrek] 启动清理失败：{e}"),
            }

            let (tx, rx) = crossbeam_channel::bounded(QUEUE_CAPACITY);
            let shared = Arc::new(CaptureShared::new(tx));
            app.manage(AppState::new(
                shared.clone(),
                cfg.clone(),
                settings_path,
                db_path.clone(),
                conn,
            ));

            // ---------- 写入器（常驻；未同意时队列始终为空） ----------
            if let Err(e) = pipeline::spawn_writer(shared.clone(), rx, db_path, cfg.db_encrypted) {
                eprintln!("[typetrek] 写入器线程启动失败：{e}");
            }

            // ---------- 托盘 ----------
            if let Err(e) = tray::build(&handle) {
                eprintln!("[typetrek] 托盘初始化失败：{e}");
            }

            // ---------- 采集线程：仅有知情同意时启动 ----------
            if cfg.consented() {
                capture::start_capture(&shared);
            } else {
                eprintln!("[typetrek] 尚未完成知情同意，采集未启动（等待用户在首启页面授权）");
            }

            // ---------- 全局快捷键 ----------
            if !cfg.pause_hotkey.is_empty() {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                if let Err(e) = handle.global_shortcut().register(cfg.pause_hotkey.as_str()) {
                    eprintln!("[typetrek] 快捷键注册失败（{}）：{e}", cfg.pause_hotkey);
                }
            }

            // ---------- 每日保留期清理（轻量后台线程） ----------
            {
                let h = handle.clone();
                std::thread::Builder::new()
                    .name("retention-cleaner".to_string())
                    .spawn(move || loop {
                        // 每 15s 检查停止信号；累计约 24h 后执行一次清理
                        for _ in 0..(24 * 60 * 4) {
                            std::thread::sleep(Duration::from_secs(15));
                            if h.state::<AppState>()
                                .shared
                                .stop_requested
                                .load(Ordering::SeqCst)
                            {
                                return;
                            }
                        }
                        let state = h.state::<AppState>();
                        let days = state.config().raw_retention_days;
                        let mut conn = state.db();
                        if let Err(e) = db::export::cleanup_expired(&mut conn, days) {
                            eprintln!("[typetrek] 定期清理失败：{e}");
                        }
                    })?;
            }

            Ok(())
        })
        // 主窗口关闭 = 隐藏到托盘（首次气泡提示"仍在采集中"），真正退出走托盘菜单
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();

                let app = window.app_handle().clone();
                let state = app.state::<AppState>();
                let cfg = state.config();
                if !cfg.tray_hint_shown {
                    let mut next = cfg.clone();
                    next.tray_hint_shown = true;
                    let _ = state.apply_config(next);

                    use tauri_plugin_notification::NotificationExt;
                    let _ = app
                        .notification()
                        .builder()
                        .title("TypeTrek 仍在采集中")
                        .body("窗口已最小化到托盘，采集继续运行。可在托盘菜单暂停或完全退出。")
                        .show();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_runtime_state,
            commands::get_settings,
            commands::set_settings,
            commands::set_paused,
            commands::grant_consent,
            commands::get_overview,
            commands::get_keyboard_stats,
            commands::get_mouse_stats,
            commands::get_calendar_stats,
            commands::get_day_detail,
            commands::get_monitors,
            commands::get_known_apps,
            commands::get_db_stats,
            commands::export_data,
            commands::delete_range,
            commands::cleanup_now,
            commands::open_data_dir,
            commands::get_app_version,
            commands::show_main_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
