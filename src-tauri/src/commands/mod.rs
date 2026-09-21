//! Tauri 命令层：前端唯一的后端入口。
//!
//! 约定：所有命令只做参数校验 + 调用下层（`db::query` / `db::export` / `AppState`），
//! 业务逻辑保持在可单测的模块里。

use crate::capture;
use crate::config::AppConfig;
use crate::db::{self, export, query};
use crate::state::AppState;
use crate::tray;
use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

/// 运行状态（Dashboard/设置页展示 + 排障）。
#[derive(Debug, Clone, Serialize)]
pub struct RuntimeState {
    pub paused: bool,
    pub consented: bool,
    pub capture_keyboard: bool,
    pub capture_mouse: bool,
    pub keyboard_installed: bool,
    pub mouse_installed: bool,
    pub writer_ready: bool,
    pub queue_len: usize,
    pub queue_capacity: usize,
    pub dropped_total: u64,
    pub blocked_total: u64,
    pub flushed_events: u64,
    pub flush_count: u64,
    pub last_flush_ms: i64,
    pub write_errors: u64,
    pub current_exe: Option<String>,
    pub monitor_count: usize,
    pub db_bytes: u64,
}

/// 采集与写入的实时状态。
#[tauri::command]
pub fn get_runtime_state(state: State<'_, AppState>) -> RuntimeState {
    let shared = &state.shared;
    RuntimeState {
        paused: shared.paused.load(Ordering::SeqCst),
        consented: state.config().consented(),
        capture_keyboard: shared.capture_keyboard.load(Ordering::SeqCst),
        capture_mouse: shared.capture_mouse.load(Ordering::SeqCst),
        keyboard_installed: shared.keyboard_installed.load(Ordering::SeqCst),
        mouse_installed: shared.mouse_installed.load(Ordering::SeqCst),
        writer_ready: shared.writer_ready.load(Ordering::SeqCst),
        // 通道长度在 sender 侧不可见，改由命令侧通过 flushed/dropped 差值间接反映；
        // 这里给出容量与丢弃计数即可满足运行状态页需求
        queue_len: 0,
        queue_capacity: capture::QUEUE_CAPACITY,
        dropped_total: shared.dropped.load(Ordering::Relaxed),
        blocked_total: shared.blocked_total.load(Ordering::Relaxed),
        flushed_events: shared.flushed_events.load(Ordering::Relaxed),
        flush_count: shared.flush_count.load(Ordering::Relaxed),
        last_flush_ms: shared.last_flush_ms.load(Ordering::Relaxed),
        write_errors: shared.write_errors.load(Ordering::Relaxed),
        current_exe: shared.foreground.latest(),
        monitor_count: shared.monitors.snapshot().len(),
        db_bytes: std::fs::metadata(&state.db_path)
            .map(|m| m.len())
            .unwrap_or(0),
    }
}

/// 读取全部设置。
#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> AppConfig {
    state.config()
}

/// 保存设置（整体覆盖）。含开机自启与快捷键的副作用处理。
#[tauri::command]
pub fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AppConfig,
) -> Result<AppConfig, String> {
    let before = state.config();
    let after = state.apply_config(config)?;

    // 开机自启副作用
    if before.autostart != after.autostart {
        use tauri_plugin_autostart::ManagerExt;
        let mgr = app.autolaunch();
        let result = if after.autostart {
            mgr.enable()
        } else {
            mgr.disable()
        };
        if let Err(e) = result {
            eprintln!("[typetrek] 开机自启设置失败：{e}");
        }
    }

    // 快捷键变更副作用：重新注册
    if before.pause_hotkey != after.pause_hotkey {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let gs = app.global_shortcut();
        let _ = gs.unregister_all();
        if !after.pause_hotkey.is_empty() {
            if let Err(e) = gs.register(after.pause_hotkey.as_str()) {
                eprintln!("[typetrek] 快捷键注册失败（{e}）：{}", after.pause_hotkey);
            }
        }
    }

    tray::refresh(&app);
    Ok(after)
}

/// 暂停/恢复采集（等价于翻转 capture_enabled 并持久化）。
#[tauri::command]
pub fn set_paused(
    app: AppHandle,
    state: State<'_, AppState>,
    paused: bool,
) -> Result<AppConfig, String> {
    let mut cfg = state.config();
    cfg.capture_enabled = !paused;
    let after = state.apply_config(cfg)?;
    tray::refresh(&app);
    Ok(after)
}

/// 首启知情同意：记录同意时间并启动采集线程。
#[tauri::command]
pub fn grant_consent(app: AppHandle, state: State<'_, AppState>) -> Result<AppConfig, String> {
    let mut cfg = state.config();
    if cfg.consented_at.is_none() {
        cfg.consented_at = Some(chrono::Local::now().to_rfc3339());
    }
    let after = state.apply_config(cfg)?;
    capture::start_capture(&state.shared);
    tray::refresh(&app);
    Ok(after)
}

/// 概览：范围内总量与按天趋势。
#[tauri::command]
pub fn get_overview(
    state: State<'_, AppState>,
    start_date: String,
    end_date: String,
) -> Result<query::Overview, String> {
    let with_repeat = state.config().repeat_counts;
    query::overview(&state.db(), &start_date, &end_date, with_repeat).map_err(|e| e.to_string())
}

/// 键盘统计：按键明细 + 小时分布。
#[tauri::command]
pub fn get_keyboard_stats(
    state: State<'_, AppState>,
    start_date: String,
    end_date: String,
) -> Result<query::KeyboardStats, String> {
    let with_repeat = state.config().repeat_counts;
    query::keyboard_stats(&state.db(), &start_date, &end_date, with_repeat)
        .map_err(|e| e.to_string())
}

/// 鼠标统计：按钮分布 + 显示器 + 网格点击。
#[tauri::command]
pub fn get_mouse_stats(
    state: State<'_, AppState>,
    start_date: String,
    end_date: String,
) -> Result<query::MouseStats, String> {
    let cell = state.config().grid_cell_size;
    query::mouse_stats(&state.db(), &start_date, &end_date, cell).map_err(|e| e.to_string())
}

/// 日历统计（`month` = "YYYY-MM"）。
#[tauri::command]
pub fn get_calendar_stats(
    state: State<'_, AppState>,
    month: String,
) -> Result<Vec<query::DayCount>, String> {
    let with_repeat = state.config().repeat_counts;
    query::calendar(&state.db(), &month, with_repeat).map_err(|e| e.to_string())
}

/// 单日详情（日历下钻）。
#[tauri::command]
pub fn get_day_detail(
    state: State<'_, AppState>,
    date: String,
) -> Result<query::DayDetail, String> {
    let with_repeat = state.config().repeat_counts;
    query::day_detail(&state.db(), &date, with_repeat).map_err(|e| e.to_string())
}

/// 显示器列表（热力图布局重建）。
#[tauri::command]
pub fn get_monitors(state: State<'_, AppState>) -> Result<Vec<query::MonitorRow>, String> {
    query::monitors(&state.db()).map_err(|e| e.to_string())
}

/// 已知应用字典（黑名单快捷添加）。
#[tauri::command]
pub fn get_known_apps(state: State<'_, AppState>) -> Result<Vec<query::AppRow>, String> {
    query::known_apps(&state.db(), 200).map_err(|e| e.to_string())
}

/// 数据库概览（数据管理页）。
#[tauri::command]
pub fn get_db_stats(state: State<'_, AppState>) -> Result<export::DbStats, String> {
    export::db_stats(&state.db(), &state.db_path)
}

/// 导出数据。弹出系统保存对话框；用户取消时返回 None。
/// `format` = csv|json；`scope` = agg|raw（raw 含按键时序，前端须先二次确认）。
#[tauri::command]
pub fn export_data(
    app: AppHandle,
    state: State<'_, AppState>,
    format: String,
    scope: String,
    start_date: String,
    end_date: String,
) -> Result<Option<export::ExportSummary>, String> {
    let ext = match format.as_str() {
        "csv" => "csv",
        "json" => "json",
        other => return Err(format!("不支持的导出格式：{other}")),
    };
    if scope != "agg" && scope != "raw" {
        return Err(format!("不支持的导出范围：{scope}"));
    }
    let default_name = format!("typetrek-{scope}-{start_date}_{end_date}.{ext}");

    let picked = app
        .dialog()
        .file()
        .set_file_name(&default_name)
        .add_filter(ext.to_uppercase(), &[ext])
        .blocking_save_file();
    let Some(picked) = picked else {
        return Ok(None); // 用户取消
    };
    let path = picked
        .into_path()
        .map_err(|e| format!("保存路径无效：{e}"))?;

    let conn = state.db();
    let summary = match (format.as_str(), scope.as_str()) {
        ("csv", "agg") => export::export_agg_csv(&conn, &start_date, &end_date, &path),
        ("csv", "raw") => export::export_raw_csv(&conn, &start_date, &end_date, &path),
        (_, "agg") => export::export_json(&conn, "agg", &start_date, &end_date, &path),
        (_, "raw") => export::export_json(&conn, "raw", &start_date, &end_date, &path),
        _ => unreachable!(),
    }?;
    Ok(Some(summary))
}

/// 按日期闭区间删除（raw 明细 + 全部聚合，单事务）。返回删除的明细行数。
#[tauri::command]
pub fn delete_range(
    state: State<'_, AppState>,
    start_date: String,
    end_date: String,
) -> Result<u64, String> {
    let (start_ts, end_ts) =
        db::local_date_range_to_ts(&start_date, &end_date).ok_or("日期区间无效")?;
    let mut conn = state.db();
    db::dao::delete_range(&mut conn, &start_date, &end_date, start_ts, end_ts)
        .map_err(|e| e.to_string())
}

/// 立即按保留期清理过期 raw 明细，返回删除行数。
#[tauri::command]
pub fn cleanup_now(state: State<'_, AppState>) -> Result<u64, String> {
    let days = state.config().raw_retention_days;
    let mut conn = state.db();
    export::cleanup_expired(&mut conn, days)
}

/// 打开数据目录（资源管理器）。
#[tauri::command]
pub fn open_data_dir(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = state
        .db_path
        .parent()
        .ok_or("数据目录不可用")?
        .to_path_buf();
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

/// 浏览器/前端可直接读取的应用版本。
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 主窗口显示（前端"打开面板"入口；与托盘共用逻辑）。
#[tauri::command]
pub fn show_main_window(app: AppHandle) {
    tray::show_main_window(&app);
}
