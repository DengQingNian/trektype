//! 管线：事件从队列 → 黑名单/归属解析 → 聚合累积 → 单事务批量落库。
//!
//! 写入器线程**独占数据库写连接**：raw 明细、五张聚合表、`apps`/`monitors` 字典的落库
//! 全部集中在此线程，保证 SQLite 写者唯一（WAL 下读写并发、写者唯一，避免锁竞争）。
//! 逐事件处理逻辑集中在可单测的 `flush_batch`，线程循环只负责攒批与调度。

pub mod batcher;

use crate::capture::event::{EventKind, RawEvent};
use crate::capture::monitors::{grid_cell, MonitorInfo};
use crate::capture::CaptureShared;
use crate::db::{self, dao};
use batcher::FlushPolicy;
use crossbeam_channel::{Receiver, RecvTimeoutError};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// flush 间隔与批大小上限（计划参数：500ms / 512 条）。
pub const FLUSH_INTERVAL: Duration = Duration::from_millis(500);
pub const BATCH_SIZE: usize = 512;
/// 写入器轮询 tick（及时响应停止信号与时间触发）。
const TICK: Duration = Duration::from_millis(50);

/// 单批处理结果（写入器统计与测试断言共用）。
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FlushReport {
    /// 通过黑名单过滤并完成聚合的事件数
    pub written: usize,
    /// 被黑名单丢弃的事件数
    pub blocked: usize,
    /// raw 键盘行数（隐私模式下为 0）
    pub keys: usize,
    /// raw 鼠标行数（隐私模式下为 0）
    pub clicks: usize,
    /// 本批错误（显示器落库或批量写入失败）
    pub error: Option<String>,
}

/// exe 名 → app_id 解析器（进程内缓存，避免每事件查库；缺失时按需 upsert）。
pub struct AppResolver {
    exe_to_id: HashMap<String, i64>,
    last_hwnd: isize,
    last_exe: Option<String>,
}

impl Default for AppResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl AppResolver {
    pub fn new() -> Self {
        Self {
            exe_to_id: HashMap::new(),
            last_hwnd: 0,
            last_exe: None,
        }
    }

    /// 从数据库预热 exe 字典（应用数量有限，启动时一次性加载）。
    pub fn warm_up(conn: &Connection) -> rusqlite::Result<Self> {
        let mut r = Self::new();
        let mut stmt = conn.prepare("SELECT id, exe_name FROM apps")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, exe) = row?;
            r.exe_to_id.insert(exe, id);
        }
        Ok(r)
    }

    /// HWND → exe 名（缓存优先，缺失时走 Win32 解析并回填缓存）。
    /// **不写数据库**——黑名单判定必须发生在任何持久化之前，命中的应用不会进入 apps 字典。
    pub fn resolve_exe(&mut self, shared: &CaptureShared, hwnd: isize) -> Option<String> {
        if hwnd == self.last_hwnd {
            return self.last_exe.clone();
        }
        let resolved = shared.foreground.exe_for(hwnd).or_else(|| {
            #[cfg(windows)]
            {
                let r = crate::capture::foreground::resolve_exe_name(hwnd);
                if let Some(ref s) = r {
                    shared.foreground.record(hwnd, s.clone());
                }
                r
            }
            #[cfg(not(windows))]
            {
                None
            }
        });
        self.last_hwnd = hwnd;
        self.last_exe = resolved.clone();
        resolved
    }

    /// exe 名 → app_id（缓存优先，缺失时 upsert 字典）。仅对通过黑名单的事件调用。
    pub fn app_id_for(&mut self, conn: &Connection, exe: &str, ts_ms: i64) -> Option<i64> {
        if let Some(id) = self.exe_to_id.get(exe) {
            return Some(*id);
        }
        let id = dao::upsert_app(conn, exe, ts_ms).ok()?;
        self.exe_to_id.insert(exe.to_string(), id);
        Some(id)
    }
}

/// 事件批处理：显示器快照落库 → 逐事件黑名单过滤与归属解析 → 聚合 → 单事务写入。
///
/// 该函数不依赖任何线程设施，可直接用内存库单测（覆盖黑名单、隐私模式、聚合正确性）。
pub fn flush_batch(
    conn: &mut Connection,
    shared: &CaptureShared,
    resolver: &mut AppResolver,
    events: &[RawEvent],
    session_id: i64,
) -> FlushReport {
    let mut report = FlushReport::default();

    // 1) 显示器快照落库并回填 id（写入器是唯一写者）
    if shared.monitors.is_dirty() {
        let list = shared.monitors.snapshot();
        let now = db::now_ms();
        let mut ids = HashMap::new();
        for m in &list {
            match dao::upsert_monitor(
                conn,
                &m.device_key,
                m.is_primary,
                m.x,
                m.y,
                m.width,
                m.height,
                m.scale,
                now,
            ) {
                Ok(id) => {
                    ids.insert(m.device_key.clone(), id);
                }
                Err(e) => report.error = Some(format!("显示器快照落库失败：{e}")),
            }
        }
        shared.monitors.set_ids(&ids);
        shared.monitors.clear_dirty();
    }

    if events.is_empty() {
        return report;
    }

    let privacy = shared.privacy_mode.load(Ordering::Relaxed);
    let blacklist = shared.blacklist_snapshot();

    let mut keys: Vec<dao::KeyEventRow<'_>> = Vec::new();
    let mut mice: Vec<dao::MouseEventRow<'_>> = Vec::new();
    let mut agg = dao::AggBatch::default();

    for ev in events {
        // 先解析 exe（不落库），黑名单命中则彻底丢弃——raw、聚合、apps 字典都不留痕迹
        let exe = resolver.resolve_exe(shared, ev.hwnd_foreground);
        if let Some(exe) = exe.as_ref() {
            let hit = if matches!(ev.kind, EventKind::Click) {
                blacklist.blocks_mouse(exe)
            } else {
                blacklist.blocks_keys(exe)
            };
            if hit {
                report.blocked += 1;
                continue;
            }
        }
        let app_id = exe
            .as_ref()
            .and_then(|e| resolver.app_id_for(conn, e, ev.ts_ms));

        match ev.kind {
            EventKind::KeyDown => {
                let Some(code) = ev.key_code else { continue };
                agg.add_key(ev.ts_ms, code, ev.is_repeat);
                agg.add_app_key(ev.ts_ms, app_id);
                if !privacy {
                    keys.push(dao::KeyEventRow {
                        ts: ev.ts_ms,
                        session_id,
                        key_code: code,
                        phase: 0,
                        is_repeat: ev.is_repeat,
                        is_injected: ev.is_injected,
                        app_id,
                    });
                }
            }
            EventKind::KeyUp => {
                let Some(code) = ev.key_code else { continue };
                // KeyUp 仅用于未来的按键时长分析，不参与任何聚合
                if !privacy {
                    keys.push(dao::KeyEventRow {
                        ts: ev.ts_ms,
                        session_id,
                        key_code: code,
                        phase: 1,
                        is_repeat: false,
                        is_injected: ev.is_injected,
                        app_id,
                    });
                }
            }
            EventKind::Click => {
                let Some(button) = ev.button else { continue };
                let (x, y) = (ev.x.unwrap_or(0), ev.y.unwrap_or(0));
                let mon: Option<MonitorInfo> = shared.monitors.monitor_at(x, y);
                let monitor_id = mon.as_ref().and_then(|m| m.id);

                agg.add_click(ev.ts_ms, button.as_str());
                agg.add_app_click(ev.ts_ms, app_id);
                if let Some(m) = mon.as_ref() {
                    if let Some(mid) = m.id {
                        let (cx, cy) = grid_cell(x, y, m);
                        agg.add_grid(ev.ts_ms, mid, cx, cy);
                    }
                }
                if !privacy {
                    mice.push(dao::MouseEventRow {
                        ts: ev.ts_ms,
                        session_id,
                        button: button.as_str(),
                        x,
                        y,
                        monitor_id,
                        app_id,
                    });
                }
            }
        }
        report.written += 1;
    }

    if !keys.is_empty() || !mice.is_empty() || !agg.is_empty() {
        match dao::write_batch(conn, &keys, &mice, &agg) {
            Ok(_) => {}
            Err(e) => report.error = Some(format!("批量写入失败（本批丢弃）：{e}")),
        }
    }

    report.keys = keys.len();
    report.clicks = mice.len();
    report
}

/// 启动写入器线程；数据库打开与迁移失败时线程直接退出（`writer_ready` 保持 false）。
/// `encrypted` 与用户设置一致，决定是否用 SQLCipher 打开（见 `db::crypto`）。
pub fn spawn_writer(
    shared: Arc<CaptureShared>,
    rx: Receiver<RawEvent>,
    db_path: PathBuf,
    encrypted: bool,
) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("db-writer".to_string())
        .spawn(move || run_writer(shared, rx, db_path, encrypted))
}

fn run_writer(
    shared: Arc<CaptureShared>,
    rx: Receiver<RawEvent>,
    db_path: PathBuf,
    encrypted: bool,
) {
    let mut conn = match db::crypto::open_maybe_encrypted(&db_path, encrypted)
        .and_then(|c| db::configure(&c).map_err(|e| e.to_string()).map(|_| c))
        .and_then(|c| {
            db::migrations::migrate(&c)
                .map_err(|e| e.to_string())
                .map(|_| c)
        }) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[typetrek] 数据库打开失败，采集数据将不会落库：{e}");
            return;
        }
    };
    let session_id = match dao::create_session(&conn, db::now_ms(), env!("CARGO_PKG_VERSION")) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("[typetrek] 会话创建失败：{e}");
            return;
        }
    };
    let mut resolver = AppResolver::warm_up(&conn).unwrap_or_default();
    let mut policy = FlushPolicy::new(BATCH_SIZE, FLUSH_INTERVAL);
    let mut buffer: Vec<RawEvent> = Vec::with_capacity(BATCH_SIZE);
    shared.writer_ready.store(true, Ordering::SeqCst);

    /// 执行一次 flush 并更新共享统计（宏避免与借用检查冲突）。
    macro_rules! do_flush {
        () => {{
            if !buffer.is_empty() || shared.monitors.is_dirty() {
                let started = Instant::now();
                let rep = flush_batch(&mut conn, &shared, &mut resolver, &buffer, session_id);
                shared.flush_count.fetch_add(1, Ordering::Relaxed);
                shared
                    .last_flush_ms
                    .store(started.elapsed().as_millis() as i64, Ordering::Relaxed);
                shared
                    .flushed_events
                    .fetch_add(rep.written as u64, Ordering::Relaxed);
                shared
                    .blocked_total
                    .fetch_add(rep.blocked as u64, Ordering::Relaxed);
                if let Some(err) = rep.error {
                    shared.write_errors.fetch_add(1, Ordering::Relaxed);
                    eprintln!("[typetrek] {err}");
                }
            }
            buffer.clear();
            policy.on_flush();
        }};
    }

    loop {
        if shared.stop_requested.load(Ordering::SeqCst) {
            // 退出前排空队列并强制 flush，保证已入队事件不丢
            while let Ok(ev) = rx.try_recv() {
                buffer.push(ev);
            }
            do_flush!();
            break;
        }

        match rx.recv_timeout(TICK) {
            Ok(ev) => {
                buffer.push(ev);
                if policy.on_event() {
                    do_flush!();
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if policy.on_tick() {
                    do_flush!();
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                do_flush!();
                break;
            }
        }
    }

    let _ = dao::close_session(&conn, session_id, db::now_ms());
    shared.writer_ready.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::blacklist::Blacklist;
    use crate::capture::event::MouseButton;
    use crate::capture::monitors::MonitorInfo;
    use crate::db::{open_in_memory, ts_to_local_date};
    use crossbeam_channel::bounded;

    /// 构造测试环境：内存库 + 共享状态 + 单显示器 + 会话。
    fn setup() -> (Connection, Arc<CaptureShared>, AppResolver, i64, i64) {
        let conn = open_in_memory().unwrap();
        let (tx, _rx) = bounded(16);
        let shared = Arc::new(CaptureShared::new(tx));
        let sid = dao::create_session(&conn, 1_700_000_000_000, "test").unwrap();
        let mid = dao::upsert_monitor(&conn, "DISPLAY1", true, 0, 0, 1920, 1080, 1.0, 0).unwrap();
        shared.monitors.replace(vec![MonitorInfo {
            id: Some(mid),
            device_key: "DISPLAY1".to_string(),
            is_primary: true,
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            scale: 1.0,
        }]);
        shared.monitors.clear_dirty();
        let resolver = AppResolver::new();
        (conn, shared, resolver, sid, mid)
    }

    fn key_event(ts: i64, code: &'static str, repeat: bool) -> RawEvent {
        RawEvent {
            ts_ms: ts,
            kind: EventKind::KeyDown,
            key_code: Some(code),
            button: None,
            x: None,
            y: None,
            hwnd_foreground: 0,
            is_repeat: repeat,
            is_injected: false,
        }
    }

    fn click_event(ts: i64, button: MouseButton, x: i32, y: i32) -> RawEvent {
        RawEvent {
            ts_ms: ts,
            kind: EventKind::Click,
            key_code: None,
            button: Some(button),
            x: Some(x),
            y: Some(y),
            hwnd_foreground: 0,
            is_repeat: false,
            is_injected: false,
        }
    }

    /// 正常路径：键盘与鼠标事件落 raw + 聚合，网格按 24px 归格。
    #[test]
    fn flush_writes_raw_and_aggregates() {
        let (mut conn, shared, mut resolver, sid, _mid) = setup();
        let t = 1_700_000_000_000i64;
        let events = vec![
            key_event(t, "KeyA", false),
            key_event(t + 1, "KeyA", true),
            click_event(t + 2, MouseButton::Left, 100, 200),
            click_event(t + 3, MouseButton::X1, 30, 40),
        ];
        let rep = flush_batch(&mut conn, &shared, &mut resolver, &events, sid);
        assert_eq!(rep.written, 4);
        assert_eq!(rep.blocked, 0);
        assert!(rep.error.is_none());

        let keys: i64 = conn
            .query_row("SELECT COUNT(*) FROM key_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(keys, 2);
        let mice: i64 = conn
            .query_row("SELECT COUNT(*) FROM mouse_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mice, 2);

        let date = ts_to_local_date(t);
        let (c, r): (i64, i64) = conn
            .query_row(
                "SELECT count, repeat_count FROM agg_key_daily WHERE key_code='KeyA'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!((c, r), (1, 1), "非 repeat=1、repeat=1 分列统计");

        // 点击 (100,200) → 格 (4,8)；(30,40) → 格 (1,1)
        let g1: i64 = conn
            .query_row(
                &format!("SELECT count FROM agg_click_grid_daily WHERE cell_x=4 AND cell_y=8 AND date='{date}'"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        let g2: i64 = conn
            .query_row(
                &format!("SELECT count FROM agg_click_grid_daily WHERE cell_x=1 AND cell_y=1 AND date='{date}'"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((g1, g2), (1, 1));

        let x1: i64 = conn
            .query_row(
                "SELECT count FROM agg_mouse_daily WHERE button='x1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(x1, 1, "侧键应进入按钮聚合");
    }

    /// 黑名单端到端：命中键盘/鼠标名单的事件 0 落库、0 聚合（T21 核心断言）。
    #[test]
    fn blacklist_drops_events_before_any_persistence() {
        let (mut conn, shared, mut resolver, sid, _mid) = setup();
        let t = 1_700_000_000_000i64;

        // 让 hwnd=42 解析为 keepass.exe
        shared.foreground.record(42, "keepass.exe".to_string());
        shared.set_blacklist(Blacklist {
            keys: vec!["keepass*".to_string()],
            mouse: vec!["keepass*".to_string()],
        });

        let mut ev = key_event(t, "KeyA", false);
        ev.hwnd_foreground = 42;
        let mut clk = click_event(t + 1, MouseButton::Left, 50, 50);
        clk.hwnd_foreground = 42;

        let rep = flush_batch(&mut conn, &shared, &mut resolver, &[ev, clk], sid);
        assert_eq!(rep.blocked, 2, "两个事件都应被黑名单拦截");
        assert_eq!(rep.written, 0);

        for (table, name) in [
            ("key_events", "键盘明细"),
            ("mouse_events", "鼠标明细"),
            ("agg_key_daily", "键盘聚合"),
            ("agg_mouse_daily", "鼠标聚合"),
            ("agg_click_grid_daily", "网格聚合"),
            ("agg_app_daily", "应用聚合"),
            ("agg_hour_daily", "小时聚合"),
            ("apps", "应用字典"),
        ] {
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{name}（{table}）不得有任何记录");
        }
    }

    /// 通过黑名单的应用正常入库；被拦截的应用连 apps 字典都不出现（元数据也不泄漏）。
    #[test]
    fn blacklist_leaves_no_trace_even_in_app_dictionary() {
        let (mut conn, shared, mut resolver, sid, _mid) = setup();
        let t = 1_700_000_000_000i64;
        shared.foreground.record(7, "bank.exe".to_string());
        shared.foreground.record(8, "code.exe".to_string());
        shared.set_blacklist(Blacklist {
            keys: vec!["bank.exe".to_string()],
            mouse: vec![],
        });

        let mut blocked = key_event(t, "KeyA", false);
        blocked.hwnd_foreground = 7;
        let mut allowed = key_event(t + 1, "KeyB", false);
        allowed.hwnd_foreground = 8;

        let rep = flush_batch(&mut conn, &shared, &mut resolver, &[blocked, allowed], sid);
        assert_eq!((rep.blocked, rep.written), (1, 1));

        let apps: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT exe_name FROM apps ORDER BY exe_name")
                .unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert_eq!(apps, vec!["code.exe".to_string()], "黑名单应用不得进入字典");
    }

    /// 隐私模式：raw 明细零写入，但聚合照常增长（T23 核心断言）。
    #[test]
    fn privacy_mode_skips_raw_but_keeps_aggregates() {
        let (mut conn, shared, mut resolver, sid, _mid) = setup();
        shared.privacy_mode.store(true, Ordering::Relaxed);

        let t = 1_700_000_000_000i64;
        let events = vec![
            key_event(t, "KeyA", false),
            key_event(t + 1, "KeyB", false),
            click_event(t + 2, MouseButton::Right, 300, 300),
        ];
        let rep = flush_batch(&mut conn, &shared, &mut resolver, &events, sid);
        assert_eq!(rep.written, 3);
        assert_eq!(rep.keys, 0, "隐私模式不得写键盘 raw");
        assert_eq!(rep.clicks, 0, "隐私模式不得写鼠标 raw");

        for table in ["key_events", "mouse_events"] {
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{table} 应为空");
        }
        let agg_keys: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(count),0) FROM agg_key_daily",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let agg_clicks: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(count),0) FROM agg_mouse_daily",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!((agg_keys, agg_clicks), (2, 1), "聚合必须照常累计");
    }

    /// 显示器快照落库：dirty 时 flush 会 upsert 并回填 id，随后点击可归到 monitor_id。
    #[test]
    fn monitor_dirty_state_is_persisted_and_ids_backfilled() {
        let conn = open_in_memory().unwrap();
        let (tx, _rx) = bounded(16);
        let shared = Arc::new(CaptureShared::new(tx));
        let sid = dao::create_session(&conn, 0, "test").unwrap();
        let mut conn = conn;

        // 缓存里有显示器但数据库还没有（模拟启动顺序）
        shared.monitors.replace(vec![MonitorInfo {
            id: None,
            device_key: "MON-NEW".to_string(),
            is_primary: true,
            x: 0,
            y: 0,
            width: 2560,
            height: 1440,
            scale: 1.25,
        }]);
        assert!(shared.monitors.is_dirty());

        let mut resolver = AppResolver::new();
        let t = 1_700_000_000_000i64;
        let rep = flush_batch(
            &mut conn,
            &shared,
            &mut resolver,
            &[click_event(t, MouseButton::Left, 200, 200)],
            sid,
        );
        assert!(rep.error.is_none(), "{:?}", rep.error);
        assert!(!shared.monitors.is_dirty(), "落库后应清除 dirty");

        let (w, scale): (i32, f64) = conn
            .query_row(
                "SELECT width, scale FROM monitors WHERE device_key='MON-NEW'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((w, scale), (2560, 1.25));

        let mid: Option<i64> = conn
            .query_row("SELECT monitor_id FROM mouse_events LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(mid.is_some(), "回填 id 后点击应带 monitor_id");
    }

    /// 空批 + 非 dirty 时完全无副作用（不产生会话外任何写入）。
    #[test]
    fn empty_flush_is_noop() {
        let (mut conn, shared, mut resolver, sid, _mid) = setup();
        let rep = flush_batch(&mut conn, &shared, &mut resolver, &[], sid);
        assert_eq!(rep, FlushReport::default());
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM key_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}
