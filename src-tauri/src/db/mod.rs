//! 数据层：连接管理、PRAGMA 配置、迁移、本地日期工具与 DAO。
//!
//! 数据库位置由调用方（Tauri setup）经 `app_data_dir` 解析后传入。

pub mod crypto;
pub mod dao;
pub mod export;
pub mod migrations;
pub mod query;
pub mod schema;

use chrono::{Days, Local, NaiveDate, TimeZone};
use rusqlite::Connection;
use std::path::Path;

/// 打开（不存在则创建）指定路径的数据库，应用 PRAGMA 并执行迁移。
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    migrations::migrate(&conn)?;
    Ok(conn)
}

/// 打开内存库并迁移（单测用；内存库不支持 WAL，journal_mode 会保持 memory）。
pub fn open_in_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    migrations::migrate(&conn)?;
    Ok(conn)
}

/// 统一 PRAGMA 配置：
/// - `journal_mode=WAL`：读写并发，采集写入不阻塞 UI 查询（内存库下降级为 memory，属预期）；
/// - `synchronous=NORMAL`：WAL 下兼顾安全与写入速度（崩溃最多丢最后一个未提交事务，不影响一致性）；
/// - `busy_timeout=2000ms`：读写并发争锁时等待而非立刻报错；
/// - `foreign_keys=ON`：开启引用完整性约束（raw 明细的 session/app/monitor 外键）。
pub fn configure(conn: &Connection) -> rusqlite::Result<()> {
    // journal_mode 的 PRAGMA 会返回一行结果，必须用 query_row 读取而不能用 pragma_update
    let _mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.busy_timeout(std::time::Duration::from_millis(2000))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    Ok(())
}

/// unix 毫秒 → 本地日期字符串（"YYYY-MM-DD"）。聚合表按本地日期分组。
pub fn ts_to_local_date(ts_ms: i64) -> String {
    Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}

/// unix 毫秒 → 本地小时（0-23），用于小时聚合。
pub fn ts_to_local_hour(ts_ms: i64) -> i32 {
    use chrono::Timelike;
    Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .map(|dt| dt.hour() as i32)
        .unwrap_or(0)
}

/// 本地日期字符串 → 当日 00:00:00.000 的 unix 毫秒。解析失败返回 None。
pub fn local_date_start_ms(date: &str) -> Option<i64> {
    let nd = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
    // earliest()：极端时区 DST 边界下取最早可能时刻，避免 single() 因歧义返回 None
    let dt = Local
        .from_local_datetime(&nd.and_hms_opt(0, 0, 0)?)
        .earliest()?;
    Some(dt.timestamp_millis())
}

/// 本地日期闭区间 `[start, end]` → unix 毫秒半开区间 `[start_ts, end_ts)`。
/// 结束边界取 `end` 次日 00:00（按日历加天，DST 安全），用于按日期删除/导出 raw 明细。
pub fn local_date_range_to_ts(start: &str, end: &str) -> Option<(i64, i64)> {
    let start_ts = local_date_start_ms(start)?;
    let end_nd = NaiveDate::parse_from_str(end, "%Y-%m-%d").ok()?;
    let next_nd = end_nd.checked_add_days(Days::new(1))?;
    let end_ts = local_date_start_ms(&next_nd.format("%Y-%m-%d").to_string())?;
    Some((start_ts, end_ts))
}

/// 当前本地日期（"YYYY-MM-DD"）。
pub fn today_local() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// 当前 unix 毫秒时间戳。
pub fn now_ms() -> i64 {
    Local::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 文件库必须启用 WAL（内存库降级为 memory 属预期，不在此断言）。
    #[test]
    fn wal_enabled_on_file_db() {
        let path = std::env::temp_dir().join(format!(
            "typetrek_wal_test_{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        {
            let conn = open(&path).unwrap();
            let mode: String = conn
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode.to_lowercase(), "wal", "文件库应启用 WAL");
        }
        // 清理主库与 WAL 附属文件
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", path.display(), suffix));
        }
    }

    /// 日期工具：时间戳→本地日期、日期→当日零点、闭区间→半开时间戳区间。
    #[test]
    fn date_helpers_roundtrip() {
        // 2024-03-05 12:30:00 本地时间
        let nd = NaiveDate::from_ymd_opt(2024, 3, 5).unwrap();
        let ts = Local
            .from_local_datetime(&nd.and_hms_opt(12, 30, 0).unwrap())
            .unwrap()
            .timestamp_millis();

        assert_eq!(ts_to_local_date(ts), "2024-03-05");
        assert_eq!(ts_to_local_hour(ts), 12);

        let start = local_date_start_ms("2024-03-05").unwrap();
        assert_eq!(ts_to_local_date(start), "2024-03-05");
        assert_eq!(
            start,
            Local
                .from_local_datetime(&nd.and_hms_opt(0, 0, 0).unwrap())
                .unwrap()
                .timestamp_millis()
        );
    }

    /// 跨月/跨年边界：日期区间换算必须按日历加天（2024-02 是闰月）。
    #[test]
    fn date_range_crosses_month_and_leap_year() {
        let (s, e) = local_date_range_to_ts("2024-02-28", "2024-02-29").unwrap();
        assert_eq!(ts_to_local_date(s), "2024-02-28");
        // end 次日零点 = 2024-03-01 00:00
        assert_eq!(ts_to_local_date(e), "2024-03-01");
        assert_eq!(
            e,
            local_date_start_ms("2024-03-01").unwrap(),
            "结束边界应为 end 次日零点"
        );

        let (s2, e2) = local_date_range_to_ts("2025-12-31", "2025-12-31").unwrap();
        assert_eq!(ts_to_local_date(s2), "2025-12-31");
        assert_eq!(ts_to_local_date(e2), "2026-01-01");
    }

    /// 非法日期字符串返回 None，不 panic。
    #[test]
    fn invalid_date_returns_none() {
        assert!(local_date_start_ms("2024-13-40").is_none());
        assert!(local_date_start_ms("not-a-date").is_none());
        assert!(local_date_range_to_ts("2024-01-01", "bad").is_none());
    }
}
