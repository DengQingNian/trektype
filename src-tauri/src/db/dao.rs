//! 数据访问层：批量写入（raw 明细 + 聚合同事务）、聚合增量累加器、按日期范围删除。
//!
//! 写入契约：`write_batch` 在**单个事务**内完成 raw 插入与五张聚合表 upsert，
//! 任一步失败整批回滚——保证不会出现"raw 写了聚合没写"的半批状态。

use crate::db::{ts_to_local_date, ts_to_local_hour};
use rusqlite::{params, Connection};
use std::collections::HashMap;

/// 键盘明细行（与 key_events 表一一对应）。
pub struct KeyEventRow<'a> {
    pub ts: i64,
    pub session_id: i64,
    pub key_code: &'a str,
    /// 0=Down 1=Up（统计只用 Down）
    pub phase: i64,
    /// OS 自动重复标记（统计口径默认排除，见设置 repeat_counts）
    pub is_repeat: bool,
    /// 软件注入事件（按 ignore_injected 设置决定是否入队）
    pub is_injected: bool,
    pub app_id: Option<i64>,
}

/// 鼠标明细行（只记 5 种按钮的按下：left/right/middle/x1/x2）。
pub struct MouseEventRow<'a> {
    pub ts: i64,
    pub session_id: i64,
    pub button: &'a str,
    pub x: i32,
    pub y: i32,
    pub monitor_id: Option<i64>,
    pub app_id: Option<i64>,
}

/// 一批聚合增量。写入器 flush 前把本批事件先归并到 HashMap：
/// 同一 (日期, 维度) 在批内累加为一条，再一次性 upsert。
/// 因此**单个事件只贡献 1 次计数**；若同一份原始数据被重复送入同一个 AggBatch，计数会重复累加，
/// 但管线保证每个 RawEvent 只进入一次（批内内存聚合 + 事务原子提交），不存在重放路径。
#[derive(Default, Debug)]
pub struct AggBatch {
    /// (date, key_code) -> (非 repeat 次数, repeat 次数)
    pub key_daily: HashMap<(String, String), (i64, i64)>,
    /// (date, button) -> 次数
    pub mouse_daily: HashMap<(String, String), i64>,
    /// (date, monitor_id, cell_x, cell_y) -> 次数
    pub click_grid: HashMap<(String, i64, i32, i32), i64>,
    /// (date, app_id) -> (key_count, click_count)
    pub app_daily: HashMap<(String, i64), (i64, i64)>,
    /// (date, hour) -> (非 repeat 按键次数, repeat 按键次数, 点击次数)
    pub hour_daily: HashMap<(String, i32), (i64, i64, i64)>,
}

impl AggBatch {
    /// 是否无任何待写计数。
    pub fn is_empty(&self) -> bool {
        self.key_daily.is_empty()
            && self.mouse_daily.is_empty()
            && self.click_grid.is_empty()
            && self.app_daily.is_empty()
            && self.hour_daily.is_empty()
    }

    /// 键盘事件计数（date/hour 由 ts 本地时区推导）。
    /// `is_repeat=true` 计入 repeat_count 列；小时趋势保留该口径，应用活跃度仍只统计非 repeat。
    pub fn add_key(&mut self, ts_ms: i64, key_code: &str, is_repeat: bool) {
        let date = ts_to_local_date(ts_ms);
        let entry = self
            .key_daily
            .entry((date, key_code.to_string()))
            .or_default();
        if is_repeat {
            entry.1 += 1;
        } else {
            entry.0 += 1;
        }
        self.add_hour_key(ts_ms, is_repeat);
    }

    /// 鼠标点击计数。
    pub fn add_click(&mut self, ts_ms: i64, button: &str) {
        let date = ts_to_local_date(ts_ms);
        *self
            .mouse_daily
            .entry((date, button.to_string()))
            .or_insert(0) += 1;
        self.add_hour_click(ts_ms);
    }

    /// 点击网格计数（cell 为 24px 基准网格索引）。
    pub fn add_grid(&mut self, ts_ms: i64, monitor_id: i64, cell_x: i32, cell_y: i32) {
        let date = ts_to_local_date(ts_ms);
        *self
            .click_grid
            .entry((date, monitor_id, cell_x, cell_y))
            .or_insert(0) += 1;
    }

    /// 应用维度：键盘 +1（app_id 为空时跳过，例如前台解析失败）。
    pub fn add_app_key(&mut self, ts_ms: i64, app_id: Option<i64>) {
        if let Some(id) = app_id {
            let date = ts_to_local_date(ts_ms);
            self.app_daily.entry((date, id)).or_default().0 += 1;
        }
    }

    /// 应用维度：点击 +1。
    pub fn add_app_click(&mut self, ts_ms: i64, app_id: Option<i64>) {
        if let Some(id) = app_id {
            let date = ts_to_local_date(ts_ms);
            self.app_daily.entry((date, id)).or_default().1 += 1;
        }
    }

    fn add_hour_key(&mut self, ts_ms: i64, is_repeat: bool) {
        let key = (ts_to_local_date(ts_ms), ts_to_local_hour(ts_ms));
        let entry = self.hour_daily.entry(key).or_default();
        if is_repeat {
            entry.1 += 1;
        } else {
            entry.0 += 1;
        }
    }

    fn add_hour_click(&mut self, ts_ms: i64) {
        let key = (ts_to_local_date(ts_ms), ts_to_local_hour(ts_ms));
        self.hour_daily.entry(key).or_default().2 += 1;
    }
}

/// 单事务写入：raw 明细批量插入 + 五张聚合表增量 upsert。
/// 空切片/空聚合表均安全跳过。返回本批写入的 raw 行数（键盘 + 鼠标）。
pub fn write_batch(
    conn: &mut Connection,
    keys: &[KeyEventRow<'_>],
    mice: &[MouseEventRow<'_>],
    agg: &AggBatch,
) -> rusqlite::Result<usize> {
    let tx = conn.transaction()?;

    if !keys.is_empty() {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO key_events(ts, session_id, key_code, phase, is_repeat, is_injected, app_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for r in keys {
            stmt.execute(params![
                r.ts,
                r.session_id,
                r.key_code,
                r.phase,
                r.is_repeat,
                r.is_injected,
                r.app_id
            ])?;
        }
    }

    if !mice.is_empty() {
        let mut stmt = tx.prepare_cached(
            "INSERT INTO mouse_events(ts, session_id, button, x, y, monitor_id, app_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for r in mice {
            stmt.execute(params![
                r.ts,
                r.session_id,
                r.button,
                r.x,
                r.y,
                r.monitor_id,
                r.app_id
            ])?;
        }
    }

    upsert_aggregates(&tx, agg)?;
    tx.commit()?;
    Ok(keys.len() + mice.len())
}

/// 五张聚合表的增量 upsert（`count = count + excluded.count`）。
/// 可独立调用（事务内），供写入器与测试复用。
pub fn upsert_aggregates(conn: &Connection, agg: &AggBatch) -> rusqlite::Result<()> {
    if !agg.key_daily.is_empty() {
        let mut stmt = conn.prepare_cached(
            "INSERT INTO agg_key_daily(date, key_code, count, repeat_count) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(date, key_code) DO UPDATE SET
               count = count + excluded.count,
               repeat_count = repeat_count + excluded.repeat_count",
        )?;
        for ((date, code), (n, r)) in &agg.key_daily {
            stmt.execute(params![date, code, n, r])?;
        }
    }

    if !agg.mouse_daily.is_empty() {
        let mut stmt = conn.prepare_cached(
            "INSERT INTO agg_mouse_daily(date, button, count) VALUES (?1, ?2, ?3)
             ON CONFLICT(date, button) DO UPDATE SET count = count + excluded.count",
        )?;
        for ((date, button), n) in &agg.mouse_daily {
            stmt.execute(params![date, button, n])?;
        }
    }

    if !agg.click_grid.is_empty() {
        let mut stmt = conn.prepare_cached(
            "INSERT INTO agg_click_grid_daily(date, monitor_id, cell_x, cell_y, count)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(date, monitor_id, cell_x, cell_y) DO UPDATE SET count = count + excluded.count",
        )?;
        for ((date, mid, cx, cy), n) in &agg.click_grid {
            stmt.execute(params![date, mid, cx, cy, n])?;
        }
    }

    if !agg.app_daily.is_empty() {
        let mut stmt = conn.prepare_cached(
            "INSERT INTO agg_app_daily(date, app_id, key_count, click_count) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(date, app_id) DO UPDATE SET
               key_count = key_count + excluded.key_count,
               click_count = click_count + excluded.click_count",
        )?;
        for ((date, app_id), (k, c)) in &agg.app_daily {
            stmt.execute(params![date, app_id, k, c])?;
        }
    }

    if !agg.hour_daily.is_empty() {
        let mut stmt = conn.prepare_cached(
            "INSERT INTO agg_hour_daily(date, hour, key_count, repeat_count, click_count) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(date, hour) DO UPDATE SET
               key_count = key_count + excluded.key_count,
               repeat_count = repeat_count + excluded.repeat_count,
               click_count = click_count + excluded.click_count",
        )?;
        for ((date, hour), (k, r, c)) in &agg.hour_daily {
            stmt.execute(params![date, hour, k, r, c])?;
        }
    }

    Ok(())
}

/// 按本地日期**闭区间** `[start_date, end_date]` 删除 raw 明细与全部聚合数据（单事务）。
/// `start_ts`/`end_ts` 由调用方经 `db::local_date_range_to_ts` 计算，与日期边界保持一致。
/// 返回删除的 raw 明细行数（键盘 + 鼠标），供 UI 反馈。
pub fn delete_range(
    conn: &mut Connection,
    start_date: &str,
    end_date: &str,
    start_ts: i64,
    end_ts: i64,
) -> rusqlite::Result<u64> {
    let tx = conn.transaction()?;
    let mut removed: u64 = 0;

    removed += tx.execute(
        "DELETE FROM key_events WHERE ts >= ?1 AND ts < ?2",
        params![start_ts, end_ts],
    )? as u64;
    removed += tx.execute(
        "DELETE FROM mouse_events WHERE ts >= ?1 AND ts < ?2",
        params![start_ts, end_ts],
    )? as u64;

    for table in [
        "agg_key_daily",
        "agg_mouse_daily",
        "agg_click_grid_daily",
        "agg_app_daily",
        "agg_hour_daily",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE date >= ?1 AND date <= ?2"),
            params![start_date, end_date],
        )?;
    }

    tx.commit()?;
    Ok(removed)
}

/// 创建会话并返回 id（每次进程启动一条）。
pub fn create_session(
    conn: &Connection,
    started_at: i64,
    app_version: &str,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO sessions(started_at, app_version) VALUES (?1, ?2)",
        params![started_at, app_version],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 回填会话结束时间（优雅退出时调用）。
pub fn close_session(conn: &Connection, session_id: i64, ended_at: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
        params![ended_at, session_id],
    )?;
    Ok(())
}

/// 取得（必要时创建）应用字典 id。exe_name 需为已小写规范化的进程名。
pub fn upsert_app(conn: &Connection, exe_name: &str, now: i64) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO apps(exe_name, first_seen, last_seen) VALUES (?1, ?2, ?2)
         ON CONFLICT(exe_name) DO UPDATE SET last_seen = excluded.last_seen",
        params![exe_name, now],
    )?;
    conn.query_row("SELECT id FROM apps WHERE exe_name = ?1", [exe_name], |r| {
        r.get(0)
    })
}

/// 取得（必要时创建/更新）显示器快照 id。布局变化时更新坐标并刷新 last_seen。
#[allow(clippy::too_many_arguments)]
pub fn upsert_monitor(
    conn: &Connection,
    device_key: &str,
    is_primary: bool,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale: f64,
    now: i64,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO monitors(device_key, is_primary, x, y, width, height, scale, first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
         ON CONFLICT(device_key) DO UPDATE SET
           is_primary = excluded.is_primary,
           x = excluded.x, y = excluded.y,
           width = excluded.width, height = excluded.height,
           scale = excluded.scale,
           last_seen = excluded.last_seen",
        params![device_key, is_primary, x, y, width, height, scale, now],
    )?;
    conn.query_row(
        "SELECT id FROM monitors WHERE device_key = ?1",
        [device_key],
        |r| r.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    /// 造一条会话，满足 raw 明细的外键约束。
    fn seed_session(conn: &Connection) -> i64 {
        create_session(conn, 1_700_000_000_000, "test").unwrap()
    }

    /// 固定时间戳（本地时区 2024-03-05 10:00:00），避免测试依赖真实当前时间。
    fn ts_at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        use chrono::{Local, NaiveDate, TimeZone};
        let nd = NaiveDate::from_ymd_opt(y, mo, d).unwrap();
        Local
            .from_local_datetime(&nd.and_hms_opt(h, mi, 0).unwrap())
            .unwrap()
            .timestamp_millis()
    }

    /// 批量写入：raw 行数与五张聚合表的值必须一致。
    #[test]
    fn write_batch_persists_raw_and_aggregates() {
        let mut conn = open_in_memory().unwrap();
        let sid = seed_session(&conn);
        let app_id = upsert_app(&conn, "code.exe", ts_at(2024, 3, 5, 9, 0)).unwrap();
        let mid = upsert_monitor(
            &conn,
            "MON-A",
            true,
            0,
            0,
            1920,
            1080,
            1.0,
            ts_at(2024, 3, 5, 9, 0),
        )
        .unwrap();

        let t = ts_at(2024, 3, 5, 10, 0);
        let keys = vec![
            KeyEventRow {
                ts: t,
                session_id: sid,
                key_code: "KeyA",
                phase: 0,
                is_repeat: false,
                is_injected: false,
                app_id: Some(app_id),
            },
            KeyEventRow {
                ts: t + 10,
                session_id: sid,
                key_code: "KeyA",
                phase: 1,
                is_repeat: false,
                is_injected: false,
                app_id: Some(app_id),
            },
            KeyEventRow {
                ts: t + 20,
                session_id: sid,
                key_code: "Space",
                phase: 0,
                is_repeat: true,
                is_injected: false,
                app_id: Some(app_id),
            },
        ];
        let mice = vec![MouseEventRow {
            ts: t + 30,
            session_id: sid,
            button: "left",
            x: 100,
            y: 200,
            monitor_id: Some(mid),
            app_id: Some(app_id),
        }];

        let mut agg = AggBatch::default();
        agg.add_key(t, "KeyA", false);
        agg.add_key(t + 20, "Space", true);
        agg.add_click(t + 30, "left");
        agg.add_grid(t + 30, mid, 4, 8);
        agg.add_app_key(t, Some(app_id));
        agg.add_app_click(t + 30, Some(app_id));

        let n = write_batch(&mut conn, &keys, &mice, &agg).unwrap();
        assert_eq!(n, 4, "应写入 3 条键盘 + 1 条鼠标");

        let raw_keys: i64 = conn
            .query_row("SELECT COUNT(*) FROM key_events", [], |r| r.get(0))
            .unwrap();
        let raw_mice: i64 = conn
            .query_row("SELECT COUNT(*) FROM mouse_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(raw_keys, 3);
        assert_eq!(raw_mice, 1);

        let (kc, krep, mc): (i64, i64, i64) = conn
            .query_row(
                "SELECT (SELECT count FROM agg_key_daily WHERE date='2024-03-05' AND key_code='KeyA'),
                        (SELECT repeat_count FROM agg_key_daily WHERE date='2024-03-05' AND key_code='Space'),
                        (SELECT count FROM agg_mouse_daily WHERE date='2024-03-05' AND button='left')",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (kc, krep, mc),
            (1, 1, 1),
            "KeyA 非 repeat=1；Space repeat=1；点击=1"
        );

        let (grid, appk, appc, hk, hr): (i64, i64, i64, i64, i64) = conn
            .query_row(
                "SELECT (SELECT count FROM agg_click_grid_daily WHERE date='2024-03-05' AND cell_x=4 AND cell_y=8),
                        (SELECT key_count FROM agg_app_daily WHERE date='2024-03-05'),
                        (SELECT click_count FROM agg_app_daily WHERE date='2024-03-05'),
                        (SELECT key_count FROM agg_hour_daily WHERE date='2024-03-05' AND hour=10),
                        (SELECT repeat_count FROM agg_hour_daily WHERE date='2024-03-05' AND hour=10)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(
            (grid, appk, appc, hk, hr),
            (1, 1, 1, 1, 1),
            "网格/应用/小时聚合应与事件对应；小时聚合单独保留 repeat 计数"
        );
    }

    /// 批内同键事件先合并再 upsert：3 次 KeyA 只产生 1 行、值=3；
    /// 后续新批继续累加（增量语义）。
    #[test]
    fn agg_batch_merges_within_batch_and_accumulates_across_batches() {
        let mut conn = open_in_memory().unwrap();
        let sid = seed_session(&conn);
        let t = ts_at(2024, 3, 5, 10, 0);

        let mut agg = AggBatch::default();
        agg.add_key(t, "KeyE", false);
        agg.add_key(t + 1, "KeyE", false);
        agg.add_key(t + 2, "KeyE", false);
        write_batch(&mut conn, &[], &[], &agg).unwrap();

        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM agg_key_daily", [], |r| r.get(0))
            .unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count FROM agg_key_daily WHERE key_code='KeyE'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!((rows, count), (1, 3), "批内 3 次同键应合并为 1 行、计数 3");

        // 第二个批次继续累加
        let mut agg2 = AggBatch::default();
        agg2.add_key(t + 100, "KeyE", false);
        write_batch(&mut conn, &[], &[], &agg2).unwrap();
        let count2: i64 = conn
            .query_row(
                "SELECT count FROM agg_key_daily WHERE key_code='KeyE'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count2, 4, "跨批次应为增量累加");

        // repeat 走独立计数列，不影响主口径
        let mut agg3 = AggBatch::default();
        agg3.add_key(t + 200, "KeyE", true);
        agg3.add_key(t + 201, "KeyE", true);
        write_batch(&mut conn, &[], &[], &agg3).unwrap();
        let (c, r): (i64, i64) = conn
            .query_row(
                "SELECT count, repeat_count FROM agg_key_daily WHERE key_code='KeyE'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (c, r),
            (4, 2),
            "repeat 计入 repeat_count 列，主口径 count 不变"
        );
        let _ = sid;
    }

    /// 事务原子性：批内出现违反外键的行 → 整批回滚（raw 与聚合都不落库）。
    #[test]
    fn write_batch_rolls_back_on_error() {
        let mut conn = open_in_memory().unwrap();
        let sid = seed_session(&conn);
        let t = ts_at(2024, 3, 5, 10, 0);

        let keys = vec![
            KeyEventRow {
                ts: t,
                session_id: sid,
                key_code: "KeyA",
                phase: 0,
                is_repeat: false,
                is_injected: false,
                app_id: None,
            },
            // session_id=999 不存在 → 外键冲突
            KeyEventRow {
                ts: t + 1,
                session_id: 999,
                key_code: "KeyB",
                phase: 0,
                is_repeat: false,
                is_injected: false,
                app_id: None,
            },
        ];
        let mut agg = AggBatch::default();
        agg.add_key(t, "KeyA", false);

        let err = write_batch(&mut conn, &keys, &[], &agg);
        assert!(err.is_err(), "外键冲突应返回错误");

        let raw: i64 = conn
            .query_row("SELECT COUNT(*) FROM key_events", [], |r| r.get(0))
            .unwrap();
        let agg_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM agg_key_daily", [], |r| r.get(0))
            .unwrap();
        assert_eq!((raw, agg_rows), (0, 0), "整批必须回滚，不得出现半批状态");
    }

    /// 按日期闭区间删除：raw 按 ts 区间、聚合按 date 区间同步删除，区间外数据保留。
    #[test]
    fn delete_range_removes_raw_and_aggregates_in_sync() {
        let mut conn = open_in_memory().unwrap();
        let sid = seed_session(&conn);
        let mid = upsert_monitor(
            &conn,
            "MON-A",
            true,
            0,
            0,
            1920,
            1080,
            1.0,
            ts_at(2024, 3, 5, 9, 0),
        )
        .unwrap();
        let app_id = upsert_app(&conn, "code.exe", ts_at(2024, 3, 5, 9, 0)).unwrap();

        let d1 = ts_at(2024, 3, 5, 10, 0);
        let d2 = ts_at(2024, 3, 6, 10, 0);
        let keys = vec![
            KeyEventRow {
                ts: d1,
                session_id: sid,
                key_code: "KeyA",
                phase: 0,
                is_repeat: false,
                is_injected: false,
                app_id: Some(app_id),
            },
            KeyEventRow {
                ts: d2,
                session_id: sid,
                key_code: "KeyA",
                phase: 0,
                is_repeat: false,
                is_injected: false,
                app_id: Some(app_id),
            },
        ];
        let mice = vec![
            MouseEventRow {
                ts: d1,
                session_id: sid,
                button: "left",
                x: 1,
                y: 1,
                monitor_id: Some(mid),
                app_id: Some(app_id),
            },
            MouseEventRow {
                ts: d2,
                session_id: sid,
                button: "right",
                x: 2,
                y: 2,
                monitor_id: Some(mid),
                app_id: Some(app_id),
            },
        ];
        let mut agg = AggBatch::default();
        agg.add_key(d1, "KeyA", false);
        agg.add_key(d2, "KeyA", false);
        agg.add_click(d1, "left");
        agg.add_click(d2, "right");
        agg.add_grid(d1, mid, 0, 0);
        agg.add_grid(d2, mid, 0, 0);
        agg.add_app_key(d1, Some(app_id));
        agg.add_app_key(d2, Some(app_id));
        agg.add_app_click(d1, Some(app_id));
        write_batch(&mut conn, &keys, &mice, &agg).unwrap();

        let (s, e) = crate::db::local_date_range_to_ts("2024-03-05", "2024-03-05").unwrap();
        let removed = delete_range(&mut conn, "2024-03-05", "2024-03-05", s, e).unwrap();
        assert_eq!(removed, 2, "应删除 3/5 的 1 条键盘 + 1 条鼠标");

        let raw: i64 = conn
            .query_row("SELECT COUNT(*) FROM key_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(raw, 1, "3/6 的键盘数据应保留");
        let mice_left: i64 = conn
            .query_row("SELECT COUNT(*) FROM mouse_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mice_left, 1, "3/6 的鼠标数据应保留");

        for (table, col) in [
            ("agg_key_daily", "count"),
            ("agg_mouse_daily", "count"),
            ("agg_click_grid_daily", "count"),
            ("agg_app_daily", "key_count"),
            ("agg_hour_daily", "key_count"),
        ] {
            let d1_rows: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE date='2024-03-05'"),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let d2_rows: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE date='2024-03-06'"),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let _ = col;
            assert_eq!(d1_rows, 0, "{table} 的 3/5 聚合应被删除");
            assert!(d2_rows > 0, "{table} 的 3/6 聚合应保留");
        }
    }

    /// 应用字典/显示器快照的 upsert 语义：同名只增一行、重复调用返回同一 id、last_seen 刷新。
    #[test]
    fn upsert_app_and_monitor_are_stable() {
        let conn = open_in_memory().unwrap();
        let t0 = ts_at(2024, 3, 5, 9, 0);
        let t1 = ts_at(2024, 3, 5, 11, 0);

        let a1 = upsert_app(&conn, "chrome.exe", t0).unwrap();
        let a2 = upsert_app(&conn, "chrome.exe", t1).unwrap();
        assert_eq!(a1, a2, "同一 exe 名应返回同一 id");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM apps", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let last: i64 = conn
            .query_row("SELECT last_seen FROM apps", [], |r| r.get(0))
            .unwrap();
        assert_eq!(last, t1, "last_seen 应刷新");

        let m1 =
            upsert_monitor(&conn, "MON\\\\.\\DISPLAY1", true, 0, 0, 1920, 1080, 1.0, t0).unwrap();
        // 分辨率变化：同一 device_key 更新而非新增
        let m2 = upsert_monitor(
            &conn,
            "MON\\\\.\\DISPLAY1",
            true,
            0,
            0,
            2560,
            1440,
            1.25,
            t1,
        )
        .unwrap();
        assert_eq!(m1, m2);
        let (w, scale): (i32, f64) = conn
            .query_row("SELECT width, scale FROM monitors", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((w, scale), (2560, 1.25));
    }

    /// 空批写入安全：不报错、不产生任何行。
    #[test]
    fn empty_batch_is_noop() {
        let mut conn = open_in_memory().unwrap();
        let n = write_batch(&mut conn, &[], &[], &AggBatch::default()).unwrap();
        assert_eq!(n, 0);
        let total: i64 = conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM key_events) + (SELECT COUNT(*) FROM agg_key_daily)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 0);
    }
}
