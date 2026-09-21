//! 数据导出（CSV/JSON，流式写出）与保留期清理。
//!
//! - 聚合导出（scope=agg）：CSV 为统一长表 `date,category,name,count,extra`，
//!   便于 Excel/pandas 直接分析；JSON 为一个包含各类数组的对象。
//! - 明细导出（scope=raw）：逐行流式写出，避免大范围导出时内存膨胀。
//! - 清理策略：只删过期 raw 明细，**聚合永久保留**（拷问决策 Q11）。

use crate::db::{local_date_start_ms, today_local, ts_to_local_date};
use rusqlite::{params, Connection};
use std::io::{BufWriter, Write};
use std::path::Path;

/// 导出结果摘要（UI 反馈用）。
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct ExportSummary {
    pub rows: usize,
    pub bytes: u64,
}

/// 数据库概览（数据管理页展示）。
#[derive(Debug, Clone, serde::Serialize, PartialEq, Default)]
pub struct DbStats {
    pub db_bytes: u64,
    pub key_rows: i64,
    pub mouse_rows: i64,
    pub agg_rows: i64,
    pub app_count: i64,
    pub monitor_count: i64,
    pub session_count: i64,
    /// 最早的 raw 明细日期（本地日期）
    pub oldest_raw_date: Option<String>,
}

/// CSV 字段转义：含逗号/引号/换行/回车时用双引号包裹，内部引号翻倍。
pub fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// unix 毫秒 → 本地 "YYYY-MM-DD HH:MM:SS"（导出可读性）。
pub fn iso_local(ts_ms: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

/// UTF-8 BOM：Excel 识别中文所需。
const BOM: &str = "\u{FEFF}";

/// 聚合 CSV（长表）：
/// `date,category,name,count,extra`
/// - key：name=键码，count=非重复次数，extra=重复次数
/// - button：name=按钮，count=次数
/// - grid：name=`monitor_id:cell_x:cell_y`，count=点击次数
/// - app：name=app_id，count=键盘次数，extra=点击次数
/// - hour：name=小时，count=键盘次数，extra=点击次数
pub fn export_agg_csv(
    conn: &Connection,
    start: &str,
    end: &str,
    path: &Path,
) -> Result<ExportSummary, String> {
    let file = std::fs::File::create(path).map_err(|e| format!("创建文件失败：{e}"))?;
    let mut w = BufWriter::new(file);
    let mut rows = 0usize;
    let write_row = |w: &mut BufWriter<std::fs::File>, cols: &[String]| -> std::io::Result<()> {
        let line: Vec<String> = cols.iter().map(|c| csv_escape(c)).collect();
        writeln!(w, "{}", line.join(","))
    };

    w.write_all(BOM.as_bytes()).map_err(|e| e.to_string())?;
    write_row(
        &mut w,
        &[
            "date".into(),
            "category".into(),
            "name".into(),
            "count".into(),
            "extra".into(),
        ],
    )
    .map_err(|e| e.to_string())?;

    {
        let mut stmt = conn
            .prepare(
                "SELECT date, key_code, count, repeat_count FROM agg_key_daily
                 WHERE date >= ?1 AND date <= ?2 ORDER BY date, key_code",
            )
            .map_err(|e| e.to_string())?;
        let mut r = stmt.query(params![start, end]).map_err(|e| e.to_string())?;
        while let Some(row) = r.next().map_err(|e| e.to_string())? {
            let date: String = row.get(0).map_err(|e| e.to_string())?;
            let code: String = row.get(1).map_err(|e| e.to_string())?;
            let c: i64 = row.get(2).map_err(|e| e.to_string())?;
            let rep: i64 = row.get(3).map_err(|e| e.to_string())?;
            write_row(
                &mut w,
                &[date, "key".into(), code, c.to_string(), rep.to_string()],
            )
            .map_err(|e| e.to_string())?;
            rows += 1;
        }
    }
    {
        let mut stmt = conn
            .prepare(
                "SELECT date, button, count FROM agg_mouse_daily
                 WHERE date >= ?1 AND date <= ?2 ORDER BY date, button",
            )
            .map_err(|e| e.to_string())?;
        let mut r = stmt.query(params![start, end]).map_err(|e| e.to_string())?;
        while let Some(row) = r.next().map_err(|e| e.to_string())? {
            let date: String = row.get(0).map_err(|e| e.to_string())?;
            let b: String = row.get(1).map_err(|e| e.to_string())?;
            let c: i64 = row.get(2).map_err(|e| e.to_string())?;
            write_row(
                &mut w,
                &[date, "button".into(), b, c.to_string(), String::new()],
            )
            .map_err(|e| e.to_string())?;
            rows += 1;
        }
    }
    {
        let mut stmt = conn
            .prepare(
                "SELECT date, monitor_id, cell_x, cell_y, count FROM agg_click_grid_daily
                 WHERE date >= ?1 AND date <= ?2 ORDER BY date, monitor_id, cell_x, cell_y",
            )
            .map_err(|e| e.to_string())?;
        let mut r = stmt.query(params![start, end]).map_err(|e| e.to_string())?;
        while let Some(row) = r.next().map_err(|e| e.to_string())? {
            let date: String = row.get(0).map_err(|e| e.to_string())?;
            let mid: i64 = row.get(1).map_err(|e| e.to_string())?;
            let cx: i32 = row.get(2).map_err(|e| e.to_string())?;
            let cy: i32 = row.get(3).map_err(|e| e.to_string())?;
            let c: i64 = row.get(4).map_err(|e| e.to_string())?;
            write_row(
                &mut w,
                &[
                    date,
                    "grid".into(),
                    format!("{mid}:{cx}:{cy}"),
                    c.to_string(),
                    String::new(),
                ],
            )
            .map_err(|e| e.to_string())?;
            rows += 1;
        }
    }
    {
        let mut stmt = conn
            .prepare(
                "SELECT date, app_id, key_count, click_count FROM agg_app_daily
                 WHERE date >= ?1 AND date <= ?2 ORDER BY date, app_id",
            )
            .map_err(|e| e.to_string())?;
        let mut r = stmt.query(params![start, end]).map_err(|e| e.to_string())?;
        while let Some(row) = r.next().map_err(|e| e.to_string())? {
            let date: String = row.get(0).map_err(|e| e.to_string())?;
            let id: i64 = row.get(1).map_err(|e| e.to_string())?;
            let k: i64 = row.get(2).map_err(|e| e.to_string())?;
            let c: i64 = row.get(3).map_err(|e| e.to_string())?;
            write_row(
                &mut w,
                &[
                    date,
                    "app".into(),
                    id.to_string(),
                    k.to_string(),
                    c.to_string(),
                ],
            )
            .map_err(|e| e.to_string())?;
            rows += 1;
        }
    }
    {
        let mut stmt = conn
            .prepare(
                "SELECT date, hour, key_count, click_count FROM agg_hour_daily
                 WHERE date >= ?1 AND date <= ?2 ORDER BY date, hour",
            )
            .map_err(|e| e.to_string())?;
        let mut r = stmt.query(params![start, end]).map_err(|e| e.to_string())?;
        while let Some(row) = r.next().map_err(|e| e.to_string())? {
            let date: String = row.get(0).map_err(|e| e.to_string())?;
            let h: i64 = row.get(1).map_err(|e| e.to_string())?;
            let k: i64 = row.get(2).map_err(|e| e.to_string())?;
            let c: i64 = row.get(3).map_err(|e| e.to_string())?;
            write_row(
                &mut w,
                &[
                    date,
                    "hour".into(),
                    h.to_string(),
                    k.to_string(),
                    c.to_string(),
                ],
            )
            .map_err(|e| e.to_string())?;
            rows += 1;
        }
    }

    w.flush().map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(ExportSummary { rows, bytes })
}

/// raw 明细 CSV（流式）：
/// `ts,time,kind,key_code,phase,is_repeat,button,x,y,monitor_id,app_id`
pub fn export_raw_csv(
    conn: &Connection,
    start: &str,
    end: &str,
    path: &Path,
) -> Result<ExportSummary, String> {
    let (start_ts, end_ts) = crate::db::local_date_range_to_ts(start, end).ok_or("日期区间无效")?;

    let file = std::fs::File::create(path).map_err(|e| format!("创建文件失败：{e}"))?;
    let mut w = BufWriter::new(file);
    let mut rows = 0usize;

    w.write_all(BOM.as_bytes()).map_err(|e| e.to_string())?;
    writeln!(
        w,
        "ts,time,kind,key_code,phase,is_repeat,button,x,y,monitor_id,app_id"
    )
    .map_err(|e| e.to_string())?;

    {
        let mut stmt = conn
            .prepare(
                "SELECT ts, key_code, phase, is_repeat, app_id FROM key_events
                 WHERE ts >= ?1 AND ts < ?2 ORDER BY ts",
            )
            .map_err(|e| e.to_string())?;
        let mut r = stmt
            .query(params![start_ts, end_ts])
            .map_err(|e| e.to_string())?;
        while let Some(row) = r.next().map_err(|e| e.to_string())? {
            let ts: i64 = row.get(0).map_err(|e| e.to_string())?;
            let code: String = row.get(1).map_err(|e| e.to_string())?;
            let phase: i64 = row.get(2).map_err(|e| e.to_string())?;
            let rep: i64 = row.get(3).map_err(|e| e.to_string())?;
            let app: Option<i64> = row.get(4).map_err(|e| e.to_string())?;
            writeln!(
                w,
                "{ts},{},{},key,{phase},{rep},,,,,{}",
                csv_escape(&iso_local(ts)),
                csv_escape(&code),
                app.map(|a| a.to_string()).unwrap_or_default()
            )
            .map_err(|e| e.to_string())?;
            rows += 1;
        }
    }
    {
        let mut stmt = conn
            .prepare(
                "SELECT ts, button, x, y, monitor_id, app_id FROM mouse_events
                 WHERE ts >= ?1 AND ts < ?2 ORDER BY ts",
            )
            .map_err(|e| e.to_string())?;
        let mut r = stmt
            .query(params![start_ts, end_ts])
            .map_err(|e| e.to_string())?;
        while let Some(row) = r.next().map_err(|e| e.to_string())? {
            let ts: i64 = row.get(0).map_err(|e| e.to_string())?;
            let button: String = row.get(1).map_err(|e| e.to_string())?;
            let x: i32 = row.get(2).map_err(|e| e.to_string())?;
            let y: i32 = row.get(3).map_err(|e| e.to_string())?;
            let mid: Option<i64> = row.get(4).map_err(|e| e.to_string())?;
            let app: Option<i64> = row.get(5).map_err(|e| e.to_string())?;
            writeln!(
                w,
                "{ts},{},click,,,0,{},{x},{y},{},{}",
                csv_escape(&iso_local(ts)),
                csv_escape(&button),
                mid.map(|m| m.to_string()).unwrap_or_default(),
                app.map(|a| a.to_string()).unwrap_or_default()
            )
            .map_err(|e| e.to_string())?;
            rows += 1;
        }
    }

    w.flush().map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(ExportSummary { rows, bytes })
}

/// JSON 导出（流式写出，结构随 scope 不同）。
pub fn export_json(
    conn: &Connection,
    scope: &str,
    start: &str,
    end: &str,
    path: &Path,
) -> Result<ExportSummary, String> {
    let file = std::fs::File::create(path).map_err(|e| format!("创建文件失败：{e}"))?;
    let mut w = BufWriter::new(file);
    let mut rows = 0usize;

    let header = serde_json::json!({
        "exported_at": iso_local(crate::db::now_ms()),
        "range": { "start": start, "end": end },
        "scope": scope,
    });
    writeln!(w, "{}", serde_json::to_string_pretty(&header).unwrap()).map_err(|e| e.to_string())?;

    match scope {
        "agg" => {
            writeln!(w, "{{").map_err(|e| e.to_string())?;
            let section = |w: &mut BufWriter<std::fs::File>,
                           key: &str,
                           sql: &str,
                           name_col: usize|
             -> Result<usize, String> {
                let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
                let mut r = stmt.query(params![start, end]).map_err(|e| e.to_string())?;
                let mut first = true;
                write!(w, "  \"{key}\": [").map_err(|e| e.to_string())?;
                let mut n = 0usize;
                while let Some(row) = r.next().map_err(|e| e.to_string())? {
                    let date: String = row.get(0).map_err(|e| e.to_string())?;
                    // name 列可能是文本（键码/按钮）或整数（小时/app_id），统一转字符串
                    let name = match row
                        .get::<_, rusqlite::types::Value>(name_col)
                        .map_err(|e| e.to_string())?
                    {
                        rusqlite::types::Value::Text(t) => t,
                        rusqlite::types::Value::Integer(i) => i.to_string(),
                        other => format!("{other:?}"),
                    };
                    let c1: i64 = row.get(name_col + 1).map_err(|e| e.to_string())?;
                    let obj = serde_json::json!({ "date": date, "name": name, "count": c1 });
                    if !first {
                        write!(w, ",").map_err(|e| e.to_string())?;
                    }
                    write!(w, "\n    {}", serde_json::to_string(&obj).unwrap())
                        .map_err(|e| e.to_string())?;
                    first = false;
                    n += 1;
                }
                writeln!(w, "\n  ],").map_err(|e| e.to_string())?;
                Ok(n)
            };

            rows += section(&mut w, "key_daily", "SELECT date, key_code, count FROM agg_key_daily WHERE date >= ?1 AND date <= ?2 ORDER BY date, key_code", 1)?;
            rows += section(&mut w, "mouse_daily", "SELECT date, button, count FROM agg_mouse_daily WHERE date >= ?1 AND date <= ?2 ORDER BY date, button", 1)?;
            rows += section(&mut w, "hourly", "SELECT date, hour, key_count FROM agg_hour_daily WHERE date >= ?1 AND date <= ?2 ORDER BY date, hour", 1)?;
            rows += section(&mut w, "app_daily", "SELECT date, app_id, key_count FROM agg_app_daily WHERE date >= ?1 AND date <= ?2 ORDER BY date, app_id", 1)?;
            writeln!(
                w,
                "  \"note\": \"grid 数据请使用 CSV 导出（包含 monitor/cell 索引）\""
            )
            .map_err(|e| e.to_string())?;
            writeln!(w, "}}").map_err(|e| e.to_string())?;
        }
        "raw" => {
            let (start_ts, end_ts) =
                crate::db::local_date_range_to_ts(start, end).ok_or("日期区间无效")?;
            writeln!(w, "{{").map_err(|e| e.to_string())?;
            writeln!(w, "  \"key_events\": [").map_err(|e| e.to_string())?;
            {
                let mut stmt = conn
                    .prepare(
                        "SELECT ts, key_code, phase, is_repeat, is_injected, app_id FROM key_events
                         WHERE ts >= ?1 AND ts < ?2 ORDER BY ts",
                    )
                    .map_err(|e| e.to_string())?;
                let mut r = stmt
                    .query(params![start_ts, end_ts])
                    .map_err(|e| e.to_string())?;
                let mut first = true;
                while let Some(row) = r.next().map_err(|e| e.to_string())? {
                    let obj = serde_json::json!({
                        "ts": row.get::<_, i64>(0).map_err(|e| e.to_string())?,
                        "time": iso_local(row.get(0).map_err(|e| e.to_string())?),
                        "key_code": row.get::<_, String>(1).map_err(|e| e.to_string())?,
                        "phase": row.get::<_, i64>(2).map_err(|e| e.to_string())?,
                        "is_repeat": row.get::<_, i64>(3).map_err(|e| e.to_string())? != 0,
                        "app_id": row.get::<_, Option<i64>>(5).map_err(|e| e.to_string())?,
                    });
                    if !first {
                        write!(w, ",").map_err(|e| e.to_string())?;
                    }
                    write!(w, "\n    {}", serde_json::to_string(&obj).unwrap())
                        .map_err(|e| e.to_string())?;
                    first = false;
                    rows += 1;
                }
                writeln!(w, "\n  ],").map_err(|e| e.to_string())?;
            }
            writeln!(w, "  \"mouse_events\": [").map_err(|e| e.to_string())?;
            {
                let mut stmt = conn
                    .prepare(
                        "SELECT ts, button, x, y, monitor_id, app_id FROM mouse_events
                         WHERE ts >= ?1 AND ts < ?2 ORDER BY ts",
                    )
                    .map_err(|e| e.to_string())?;
                let mut r = stmt
                    .query(params![start_ts, end_ts])
                    .map_err(|e| e.to_string())?;
                let mut first = true;
                while let Some(row) = r.next().map_err(|e| e.to_string())? {
                    let ts: i64 = row.get(0).map_err(|e| e.to_string())?;
                    let obj = serde_json::json!({
                        "ts": ts,
                        "time": iso_local(ts),
                        "button": row.get::<_, String>(1).map_err(|e| e.to_string())?,
                        "x": row.get::<_, i32>(2).map_err(|e| e.to_string())?,
                        "y": row.get::<_, i32>(3).map_err(|e| e.to_string())?,
                        "monitor_id": row.get::<_, Option<i64>>(4).map_err(|e| e.to_string())?,
                        "app_id": row.get::<_, Option<i64>>(5).map_err(|e| e.to_string())?,
                    });
                    if !first {
                        write!(w, ",").map_err(|e| e.to_string())?;
                    }
                    write!(w, "\n    {}", serde_json::to_string(&obj).unwrap())
                        .map_err(|e| e.to_string())?;
                    first = false;
                    rows += 1;
                }
                writeln!(w, "\n  ]").map_err(|e| e.to_string())?;
            }
            writeln!(w, "}}").map_err(|e| e.to_string())?;
        }
        other => return Err(format!("未知导出范围：{other}")),
    }

    w.flush().map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    Ok(ExportSummary { rows, bytes })
}

/// 清理过期 raw 明细：删除 `ts < 今天(retention_days) 零点` 的 raw 行；聚合不受影响。
/// 返回删除的 raw 行数（键盘 + 鼠标）。
pub fn cleanup_expired(conn: &mut Connection, retention_days: u32) -> Result<u64, String> {
    use chrono::{Days, NaiveDate};

    let today = NaiveDate::parse_from_str(&today_local(), "%Y-%m-%d")
        .map_err(|e| format!("日期解析失败：{e}"))?;
    let cutoff_date = today
        .checked_sub_days(Days::new(retention_days.max(1) as u64))
        .ok_or("保留期计算失败")?;
    let cutoff_ts = local_date_start_ms(&cutoff_date.format("%Y-%m-%d").to_string())
        .ok_or("截止时间计算失败")?;

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let k = tx
        .execute("DELETE FROM key_events WHERE ts < ?1", params![cutoff_ts])
        .map_err(|e| e.to_string())? as u64;
    let m = tx
        .execute("DELETE FROM mouse_events WHERE ts < ?1", params![cutoff_ts])
        .map_err(|e| e.to_string())? as u64;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(k + m)
}

/// 数据库概览。
pub fn db_stats(conn: &Connection, db_path: &Path) -> Result<DbStats, String> {
    let count = |sql: &str| -> Result<i64, String> {
        conn.query_row(sql, [], |r| r.get(0))
            .map_err(|e| e.to_string())
    };
    let oldest_ts: Option<i64> = conn
        .query_row(
            "SELECT MIN(ts) FROM (SELECT MIN(ts) AS ts FROM key_events UNION ALL SELECT MIN(ts) FROM mouse_events)",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    Ok(DbStats {
        db_bytes: std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0),
        key_rows: count("SELECT COUNT(*) FROM key_events")?,
        mouse_rows: count("SELECT COUNT(*) FROM mouse_events")?,
        agg_rows: count(
            "SELECT (SELECT COUNT(*) FROM agg_key_daily) + (SELECT COUNT(*) FROM agg_mouse_daily)
                  + (SELECT COUNT(*) FROM agg_click_grid_daily) + (SELECT COUNT(*) FROM agg_app_daily)
                  + (SELECT COUNT(*) FROM agg_hour_daily)",
        )?,
        app_count: count("SELECT COUNT(*) FROM apps")?,
        monitor_count: count("SELECT COUNT(*) FROM monitors")?,
        session_count: count("SELECT COUNT(*) FROM sessions")?,
        oldest_raw_date: oldest_ts.map(ts_to_local_date),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{dao, open_in_memory};

    /// CSV 转义：逗号/引号/换行必须被正确包裹与翻倍；普通文本原样。
    #[test]
    fn csv_escape_cases() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_escape("line1\nline2"), "\"line1\nline2\"");
        assert_eq!(csv_escape("win\r\n"), "\"win\r\n\"");
    }

    /// 导出文件内的引号/逗号必须可被标准 CSV 解析（此处验证转义结果出现在输出中）。
    #[test]
    fn raw_csv_escapes_quotes_in_key_codes() {
        let mut conn = open_in_memory().unwrap();
        let sid = dao::create_session(&conn, 0, "t").unwrap();
        let keys = vec![dao::KeyEventRow {
            ts: 1_700_000_000_000,
            session_id: sid,
            key_code: "Weird,\"Key\"",
            phase: 0,
            is_repeat: false,
            is_injected: false,
            app_id: None,
        }];
        dao::write_batch(&mut conn, &keys, &[], &dao::AggBatch::default()).unwrap();

        let path = std::env::temp_dir().join(format!(
            "typetrek_csv_{}.csv",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let today = ts_to_local_date(1_700_000_000_000); // 数据所在日期，避免依赖"今天"
        let sum = export_raw_csv(&conn, &today, &today, &path).unwrap();
        assert_eq!(sum.rows, 1);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("\"Weird,\"\"Key\"\"\""),
            "转义后应可被解析：{text}"
        );
        assert!(text.starts_with('\u{FEFF}'), "应带 UTF-8 BOM 供 Excel 识别");
        let _ = std::fs::remove_file(&path);
    }

    /// 聚合 CSV：五类 category 各就各位，行数与数据一致。
    #[test]
    fn agg_csv_contains_all_categories() {
        let mut conn = open_in_memory().unwrap();
        let _sid = dao::create_session(&conn, 0, "t").unwrap();
        let mid = dao::upsert_monitor(&conn, "D1", true, 0, 0, 800, 600, 1.0, 0).unwrap();
        let app = dao::upsert_app(&conn, "code.exe", 0).unwrap();
        let t = {
            use chrono::{Local, TimeZone};
            let nd = chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
            Local
                .from_local_datetime(&nd.and_hms_opt(9, 0, 0).unwrap())
                .unwrap()
                .timestamp_millis()
        };
        let mut agg = dao::AggBatch::default();
        agg.add_key(t, "KeyA", false);
        agg.add_key(t, "KeyA", true);
        agg.add_click(t, "left");
        agg.add_grid(t, mid, 1, 2);
        agg.add_app_key(t, Some(app));
        agg.add_app_click(t, Some(app));
        dao::write_batch(&mut conn, &[], &[], &agg).unwrap();

        let path = std::env::temp_dir().join(format!(
            "typetrek_agg_{}.csv",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let sum = export_agg_csv(&conn, "2026-03-01", "2026-03-31", &path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        for cat in ["key", "button", "grid", "app", "hour"] {
            assert!(text.contains(&format!(",{cat},")), "缺少 {cat} 类数据");
            assert!(sum.rows >= 5, "五类聚合至少各一行");
        }
        assert!(
            text.contains("2026-03-05,key,KeyA,1,1"),
            "键行为含重复计数列"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// JSON 导出（agg/raw）结构可被解析，字段齐全。
    #[test]
    fn json_export_is_parseable() {
        let mut conn = open_in_memory().unwrap();
        let sid = dao::create_session(&conn, 0, "t").unwrap();
        let t = {
            use chrono::{Local, TimeZone};
            let nd = chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
            Local
                .from_local_datetime(&nd.and_hms_opt(9, 0, 0).unwrap())
                .unwrap()
                .timestamp_millis()
        };
        let keys = vec![dao::KeyEventRow {
            ts: t,
            session_id: sid,
            key_code: "KeyA",
            phase: 0,
            is_repeat: false,
            is_injected: false,
            app_id: None,
        }];
        let mut agg = dao::AggBatch::default();
        agg.add_key(t, "KeyA", false);
        dao::write_batch(&mut conn, &keys, &[], &agg).unwrap();

        let dir = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let p_agg = dir.join(format!("typetrek_agg_{stamp}.json"));
        export_json(&conn, "agg", "2026-03-01", "2026-03-31", &p_agg).unwrap();
        let text = std::fs::read_to_string(&p_agg).unwrap();
        // 头部是独立 JSON 对象，正文是另一个对象——合并验证关键片段
        assert!(text.contains("\"scope\": \"agg\""));
        assert!(text.contains("\"key_daily\""));
        assert!(
            text.contains("\"count\":1"),
            "紧凑序列化的计数对象应含 count:1"
        );
        let _ = std::fs::remove_file(&p_agg);

        let p_raw = dir.join(format!("typetrek_raw_{stamp}.json"));
        export_json(&conn, "raw", "2026-03-01", "2026-03-31", &p_raw).unwrap();
        let text = std::fs::read_to_string(&p_raw).unwrap();
        assert!(text.contains("\"key_events\""));
        assert!(
            text.contains("\"key_code\":\"KeyA\""),
            "raw JSON 应含键码：{text}"
        );
        assert!(text.contains("\"mouse_events\""));
        let _ = std::fs::remove_file(&p_raw);

        assert!(export_json(
            &conn,
            "bogus",
            "2026-03-01",
            "2026-03-31",
            &dir.join("x.json")
        )
        .is_err());
    }

    /// 保留期清理：只删过期 raw，聚合与字典保留；边界为"今天-retention 天"。
    #[test]
    fn cleanup_only_removes_expired_raw() {
        use chrono::{Duration, Local};
        let mut conn = open_in_memory().unwrap();
        let sid = dao::create_session(&conn, 0, "t").unwrap();

        let now = Local::now().timestamp_millis();
        let old = now - Duration::days(100).num_milliseconds();
        let recent = now - Duration::days(10).num_milliseconds();

        let keys = vec![
            dao::KeyEventRow {
                ts: old,
                session_id: sid,
                key_code: "KeyA",
                phase: 0,
                is_repeat: false,
                is_injected: false,
                app_id: None,
            },
            dao::KeyEventRow {
                ts: recent,
                session_id: sid,
                key_code: "KeyA",
                phase: 0,
                is_repeat: false,
                is_injected: false,
                app_id: None,
            },
        ];
        let mut agg = dao::AggBatch::default();
        agg.add_key(old, "KeyA", false);
        agg.add_key(recent, "KeyA", false);
        dao::write_batch(&mut conn, &keys, &[], &agg).unwrap();

        let removed = cleanup_expired(&mut conn, 90).unwrap();
        assert_eq!(removed, 1, "只应删除 100 天前的 1 条");

        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM key_events", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 1, "10 天前的数据应保留");
        let agg_left: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(count),0) FROM agg_key_daily",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(agg_left, 2, "聚合永久保留，不受 raw 清理影响");
        let sessions: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sessions, 1);
    }

    /// 数据库概览统计正确。
    #[test]
    fn db_stats_counts_rows() {
        let mut conn = open_in_memory().unwrap();
        let sid = dao::create_session(&conn, 0, "t").unwrap();
        let keys = vec![dao::KeyEventRow {
            ts: 1_700_000_000_000,
            session_id: sid,
            key_code: "KeyA",
            phase: 0,
            is_repeat: false,
            is_injected: false,
            app_id: None,
        }];
        let mut agg = dao::AggBatch::default();
        agg.add_key(1_700_000_000_000, "KeyA", false);
        dao::write_batch(&mut conn, &keys, &[], &agg).unwrap();

        let stats = db_stats(&conn, Path::new("nonexistent.db")).unwrap();
        assert_eq!(stats.key_rows, 1);
        assert_eq!(stats.mouse_rows, 0);
        assert!(stats.agg_rows >= 2, "键+小时聚合至少 2 行");
        assert_eq!(stats.session_count, 1);
        assert_eq!(
            stats.oldest_raw_date,
            Some(ts_to_local_date(1_700_000_000_000))
        );
    }
}
