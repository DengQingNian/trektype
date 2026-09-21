//! 聚合查询：所有统计只打聚合表（复杂度 O(天数×键种)，不随事件量增长）。
//!
//! 时间参数统一为**本地日期闭区间** `start_date..=end_date`（"YYYY-MM-DD"）。
//! `with_repeat` 控制按键口径：false 只算非重复按下，true 把 OS 自动重复计入（拷问 Q1）。

use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::BTreeMap;

/// 单日计数（日历/趋势共用）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DayCount {
    pub date: String,
    pub key_count: i64,
    pub click_count: i64,
}

/// 趋势图数据点：hour=YYYY-MM-DDTHH，day=YYYY-MM-DD，month=YYYY-MM。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TrendPoint {
    pub bucket: String,
    pub key_count: i64,
    pub click_count: i64,
}

/// 概览页数据。
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Overview {
    pub key_total: i64,
    pub click_total: i64,
    /// 活跃小时数（当天有键鼠事件的整点小时计数之和；不细到分钟，UI 明确标注为估算）
    pub active_hours: i64,
    /// 范围内有数据的天数
    pub day_count: i64,
    pub by_day: Vec<DayCount>,
}

/// 单键计数。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct KeyCount {
    pub code: String,
    pub count: i64,
    pub repeat_count: i64,
}

/// 键盘统计。
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct KeyboardStats {
    /// 按当前口径的按键总数
    pub total: i64,
    /// 按键明细（按有效次数降序；占比由前端用 total 计算）
    pub keys: Vec<KeyCount>,
    /// 24 小时分布（跨日期求和）
    pub by_hour: Vec<i64>,
}

/// 按钮计数。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ButtonCount {
    pub button: String,
    pub count: i64,
}

/// 网格点击计数（cell 索引基于显示器左上角、当前粒度的像素坐标）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GridCell {
    pub monitor_id: i64,
    pub cell_x: i32,
    pub cell_y: i32,
    pub count: i64,
}

/// 显示器记录（供前端重建布局）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MonitorRow {
    pub id: i64,
    pub device_key: String,
    pub is_primary: bool,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
}

/// 鼠标统计。
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct MouseStats {
    pub total: i64,
    pub by_button: Vec<ButtonCount>,
    pub monitors: Vec<MonitorRow>,
    pub cells: Vec<GridCell>,
}

/// 单日详情（日历下钻）。
#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct DayDetail {
    pub date: String,
    pub key_total: i64,
    pub click_total: i64,
    pub key_top: Vec<KeyCount>,
    pub by_button: Vec<ButtonCount>,
    pub by_hour: Vec<i64>,
}

/// 应用字典行（黑名单"从最近应用添加"用）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AppRow {
    pub id: i64,
    pub exe_name: String,
    pub friendly_name: Option<String>,
    pub last_seen: i64,
}

/// 键盘有效计数表达式。`with_repeat` 为编译期常量分支（非用户输入直接拼接，无注入面）。
fn key_expr(with_repeat: bool) -> &'static str {
    if with_repeat {
        "(count + repeat_count)"
    } else {
        "count"
    }
}

/// 概览：键/点击总量、活跃小时、按天趋势。
pub fn overview(
    conn: &Connection,
    start: &str,
    end: &str,
    with_repeat: bool,
) -> rusqlite::Result<Overview> {
    let expr = key_expr(with_repeat);
    let key_total: i64 = conn.query_row(
        &format!(
            "SELECT COALESCE(SUM({expr}),0) FROM agg_key_daily WHERE date >= ?1 AND date <= ?2"
        ),
        params![start, end],
        |r| r.get(0),
    )?;
    let click_total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(count),0) FROM agg_mouse_daily WHERE date >= ?1 AND date <= ?2",
        params![start, end],
        |r| r.get(0),
    )?;
    let active_hours: i64 = conn.query_row(
        "SELECT COUNT(*) FROM agg_hour_daily
         WHERE date >= ?1 AND date <= ?2 AND (key_count > 0 OR click_count > 0)",
        params![start, end],
        |r| r.get(0),
    )?;

    let mut days: BTreeMap<String, DayCount> = BTreeMap::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT date, SUM({expr}) FROM agg_key_daily
             WHERE date >= ?1 AND date <= ?2 GROUP BY date"
        ))?;
        let rows = stmt.query_map(params![start, end], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (date, count) = row?;
            days.entry(date.clone())
                .or_insert(DayCount {
                    date,
                    key_count: 0,
                    click_count: 0,
                })
                .key_count = count;
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT date, SUM(count) FROM agg_mouse_daily
             WHERE date >= ?1 AND date <= ?2 GROUP BY date",
        )?;
        let rows = stmt.query_map(params![start, end], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (date, count) = row?;
            days.entry(date.clone())
                .or_insert(DayCount {
                    date,
                    key_count: 0,
                    click_count: 0,
                })
                .click_count = count;
        }
    }

    Ok(Overview {
        key_total,
        click_total,
        active_hours,
        day_count: days.len() as i64,
        by_day: days.into_values().collect(),
    })
}

/// 活动趋势：当天按小时、周/月按天、年按月。
///
/// 返回有数据的桶，缺失桶由前端补零，这样数据库查询不会为长时间范围制造大量空行。
pub fn activity_trend(
    conn: &Connection,
    start: &str,
    end: &str,
    granularity: &str,
    with_repeat: bool,
) -> rusqlite::Result<Vec<TrendPoint>> {
    match granularity {
        "hour" => {
            let hour_expr = if with_repeat {
                "(key_count + repeat_count)"
            } else {
                "key_count"
            };
            let mut stmt = conn.prepare(&format!(
                "SELECT date || 'T' || printf('%02d', hour), SUM({hour_expr}), SUM(click_count)
                 FROM agg_hour_daily
                 WHERE date >= ?1 AND date <= ?2
                 GROUP BY date, hour ORDER BY date, hour"
            ))?;
            let rows = stmt.query_map(params![start, end], |r| {
                Ok(TrendPoint {
                    bucket: r.get(0)?,
                    key_count: r.get(1)?,
                    click_count: r.get(2)?,
                })
            })?;
            rows.collect()
        }
        "day" | "month" => {
            let expr = key_expr(with_repeat);
            let bucket_expr = if granularity == "day" {
                "date"
            } else {
                "substr(date, 1, 7)"
            };
            let mut points: BTreeMap<String, TrendPoint> = BTreeMap::new();
            {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {bucket_expr}, SUM({expr}) FROM agg_key_daily
                     WHERE date >= ?1 AND date <= ?2 GROUP BY {bucket_expr}"
                ))?;
                let rows = stmt.query_map(params![start, end], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })?;
                for row in rows {
                    let (bucket, count) = row?;
                    points
                        .entry(bucket.clone())
                        .or_insert(TrendPoint {
                            bucket,
                            key_count: 0,
                            click_count: 0,
                        })
                        .key_count = count;
                }
            }
            {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {bucket_expr}, SUM(count) FROM agg_mouse_daily
                     WHERE date >= ?1 AND date <= ?2 GROUP BY {bucket_expr}"
                ))?;
                let rows = stmt.query_map(params![start, end], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })?;
                for row in rows {
                    let (bucket, count) = row?;
                    points
                        .entry(bucket.clone())
                        .or_insert(TrendPoint {
                            bucket,
                            key_count: 0,
                            click_count: 0,
                        })
                        .click_count = count;
                }
            }
            Ok(points.into_values().collect())
        }
        _ => Err(rusqlite::Error::InvalidParameterName(format!(
            "不支持的趋势粒度：{granularity}"
        ))),
    }
}

/// 键盘统计：按键明细（降序）+ 24 小时分布。
pub fn keyboard_stats(
    conn: &Connection,
    start: &str,
    end: &str,
    with_repeat: bool,
) -> rusqlite::Result<KeyboardStats> {
    let expr = key_expr(with_repeat);
    let mut keys = Vec::new();
    let mut total = 0i64;
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT key_code, SUM(count), SUM(repeat_count), SUM({expr}) AS eff
             FROM agg_key_daily WHERE date >= ?1 AND date <= ?2
             GROUP BY key_code ORDER BY eff DESC, key_code ASC"
        ))?;
        let rows = stmt.query_map(params![start, end], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (code, count, repeat_count, eff) = row?;
            total += eff;
            keys.push(KeyCount {
                code,
                count,
                repeat_count,
            });
        }
    }

    let mut by_hour = vec![0i64; 24];
    {
        let mut stmt = conn.prepare(
            "SELECT hour, SUM(key_count) FROM agg_hour_daily
             WHERE date >= ?1 AND date <= ?2 GROUP BY hour",
        )?;
        let rows = stmt.query_map(params![start, end], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (hour, count) = row?;
            if (0..24).contains(&hour) {
                by_hour[hour as usize] = count;
            }
        }
    }

    Ok(KeyboardStats {
        total,
        keys,
        by_hour,
    })
}

/// 鼠标统计：按钮分布 + 显示器列表 + 网格点击（按期粒度合并基准 24px 格）。
///
/// 合并方式：取基准格中心点映射到目标格（`(cell*24+12)/size`），
/// 因此支持任意粒度（24/32/64）且总计数不丢失；格边界最多有半个基准格的偏移。
pub fn mouse_stats(
    conn: &Connection,
    start: &str,
    end: &str,
    cell_size: u32,
) -> rusqlite::Result<MouseStats> {
    let size = cell_size.max(1) as i64;

    let mut by_button = Vec::new();
    let mut total = 0i64;
    {
        let mut stmt = conn.prepare(
            "SELECT button, SUM(count) FROM agg_mouse_daily
             WHERE date >= ?1 AND date <= ?2 GROUP BY button ORDER BY 2 DESC, button ASC",
        )?;
        let rows = stmt.query_map(params![start, end], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (button, count) = row?;
            total += count;
            by_button.push(ButtonCount { button, count });
        }
    }

    let mut cells = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT monitor_id,
                    (cell_x * 24 + 12) / ?3 AS gx,
                    (cell_y * 24 + 12) / ?3 AS gy,
                    SUM(count)
             FROM agg_click_grid_daily
             WHERE date >= ?1 AND date <= ?2
             GROUP BY monitor_id, gx, gy",
        )?;
        let rows = stmt.query_map(params![start, end, size], |r| {
            Ok(GridCell {
                monitor_id: r.get(0)?,
                cell_x: r.get(1)?,
                cell_y: r.get(2)?,
                count: r.get(3)?,
            })
        })?;
        for row in rows {
            cells.push(row?);
        }
    }

    Ok(MouseStats {
        total,
        by_button,
        monitors: monitors(conn)?,
        cells,
    })
}

/// 全部显示器记录（按主屏优先、坐标排序）。
pub fn monitors(conn: &Connection) -> rusqlite::Result<Vec<MonitorRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, device_key, is_primary, x, y, width, height, scale
         FROM monitors ORDER BY is_primary DESC, x ASC, y ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(MonitorRow {
            id: r.get(0)?,
            device_key: r.get(1)?,
            is_primary: r.get::<_, i64>(2)? != 0,
            x: r.get(3)?,
            y: r.get(4)?,
            width: r.get(5)?,
            height: r.get(6)?,
            scale: r.get(7)?,
        })
    })?;
    rows.collect()
}

/// 日历统计：`month` 为 "YYYY-MM"，返回该月**有数据**的日期（缺失日期由前端补零渲染）。
pub fn calendar(
    conn: &Connection,
    month: &str,
    with_repeat: bool,
) -> rusqlite::Result<Vec<DayCount>> {
    let expr = key_expr(with_repeat);
    let like = format!("{month}-%");
    let mut days: BTreeMap<String, DayCount> = BTreeMap::new();

    {
        let mut stmt = conn.prepare(&format!(
            "SELECT date, SUM({expr}) FROM agg_key_daily WHERE date LIKE ?1 GROUP BY date"
        ))?;
        let rows = stmt.query_map([&like], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (date, count) = row?;
            days.entry(date.clone())
                .or_insert(DayCount {
                    date,
                    key_count: 0,
                    click_count: 0,
                })
                .key_count = count;
        }
    }
    {
        let mut stmt = conn.prepare(
            "SELECT date, SUM(count) FROM agg_mouse_daily WHERE date LIKE ?1 GROUP BY date",
        )?;
        let rows = stmt.query_map([&like], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (date, count) = row?;
            days.entry(date.clone())
                .or_insert(DayCount {
                    date,
                    key_count: 0,
                    click_count: 0,
                })
                .click_count = count;
        }
    }

    Ok(days.into_values().collect())
}

/// 单日详情（日历下钻）：Top 键、按钮分布、24 小时曲线。
pub fn day_detail(conn: &Connection, date: &str, with_repeat: bool) -> rusqlite::Result<DayDetail> {
    let expr = key_expr(with_repeat);
    let key_total: i64 = conn.query_row(
        &format!("SELECT COALESCE(SUM({expr}),0) FROM agg_key_daily WHERE date = ?1"),
        [date],
        |r| r.get(0),
    )?;
    let click_total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(count),0) FROM agg_mouse_daily WHERE date = ?1",
        [date],
        |r| r.get(0),
    )?;

    let mut key_top = Vec::new();
    {
        let mut stmt = conn.prepare(&format!(
            "SELECT key_code, SUM(count), SUM(repeat_count) FROM agg_key_daily
             WHERE date = ?1 GROUP BY key_code ORDER BY SUM({expr}) DESC, key_code ASC LIMIT 20"
        ))?;
        let rows = stmt.query_map([date], |r| {
            Ok(KeyCount {
                code: r.get(0)?,
                count: r.get(1)?,
                repeat_count: r.get(2)?,
            })
        })?;
        for row in rows {
            key_top.push(row?);
        }
    }

    let mut by_button = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT button, SUM(count) FROM agg_mouse_daily
             WHERE date = ?1 GROUP BY button ORDER BY 2 DESC, button ASC",
        )?;
        let rows = stmt.query_map([date], |r| {
            Ok(ButtonCount {
                button: r.get(0)?,
                count: r.get(1)?,
            })
        })?;
        for row in rows {
            by_button.push(row?);
        }
    }

    let mut by_hour = vec![0i64; 24];
    {
        let mut stmt = conn
            .prepare("SELECT hour, key_count, click_count FROM agg_hour_daily WHERE date = ?1")?;
        let rows = stmt.query_map([date], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        for row in rows {
            let (hour, k, c) = row?;
            if (0..24).contains(&hour) {
                by_hour[hour as usize] = k + c;
            }
        }
    }

    Ok(DayDetail {
        date: date.to_string(),
        key_total,
        click_total,
        key_top,
        by_button,
        by_hour,
    })
}

/// 已知应用字典（按最近使用排序，黑名单编辑器用）。
pub fn known_apps(conn: &Connection, limit: u32) -> rusqlite::Result<Vec<AppRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, exe_name, friendly_name, last_seen FROM apps
         ORDER BY last_seen DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| {
        Ok(AppRow {
            id: r.get(0)?,
            exe_name: r.get(1)?,
            friendly_name: r.get(2)?,
            last_seen: r.get(3)?,
        })
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{dao, open_in_memory};

    /// 本地时区构造时间戳（避免测试依赖固定 UTC 值，任何时区下都成立）。
    fn ts_at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        use chrono::{Local, NaiveDate, TimeZone};
        let nd = NaiveDate::from_ymd_opt(y, mo, d).unwrap();
        Local
            .from_local_datetime(&nd.and_hms_opt(h, mi, 0).unwrap())
            .unwrap()
            .timestamp_millis()
    }

    fn date_of(ts: i64) -> String {
        crate::db::ts_to_local_date(ts)
    }

    /// 造数：跨月的 3 天数据、两类按钮、两个网格点、含 repeat 计数。
    fn seed(conn: &mut Connection) {
        let _sid = dao::create_session(conn, 0, "test").unwrap();
        let mid = dao::upsert_monitor(conn, "D1", true, 0, 0, 1920, 1080, 1.0, 0).unwrap();
        let app = dao::upsert_app(conn, "code.exe", 0).unwrap();

        let t1 = ts_at(2026, 1, 31, 10, 0);
        let t2 = ts_at(2026, 2, 1, 10, 0);
        let t3 = ts_at(2026, 2, 2, 10, 0);

        let mut agg = dao::AggBatch::default();
        // 1 月 31 日
        agg.add_key(t1, "KeyA", false);
        agg.add_key(t1 + 1, "KeyA", false);
        agg.add_key(t1 + 2, "KeyA", true);
        agg.add_key(t1 + 3, "Space", false);
        agg.add_click(t1, "left");
        agg.add_click(t1 + 1, "left");
        agg.add_click(t1 + 2, "right");
        agg.add_grid(t1, mid, 4, 8);
        agg.add_grid(t1, mid, 5, 8);
        agg.add_app_key(t1, Some(app));
        // 2 月 1 日
        agg.add_key(t2, "KeyB", false);
        agg.add_click(t2, "middle");
        // 2 月 2 日
        agg.add_key(t3, "KeyB", false);
        dao::write_batch(conn, &[], &[], &agg).unwrap();
    }

    /// 概览：总量、活跃小时、按天趋势；repeat 口径可切换；空范围全零。
    #[test]
    fn overview_respects_repeat_switch() {
        let mut conn = open_in_memory().unwrap();
        seed(&mut conn);
        let d1 = date_of(ts_at(2026, 1, 31, 10, 0));
        let d3 = date_of(ts_at(2026, 2, 2, 10, 0));

        let without = overview(&conn, &d1, &d3, false).unwrap();
        // KeyA×2 + Space×1 + KeyB×2 = 5（不含 repeat）
        assert_eq!(without.key_total, 5);
        assert_eq!(without.click_total, 4);
        assert!(without.active_hours >= 2, "至少两天有活跃小时");
        assert_eq!(without.day_count, 3);
        assert_eq!(without.by_day.len(), 3);
        assert!(
            without.by_day[0].date < without.by_day[1].date,
            "按日期升序"
        );

        let with = overview(&conn, &d1, &d3, true).unwrap();
        assert_eq!(with.key_total, 6, "开启 repeat 口径后 KeyA 的重复计入");

        let empty = overview(&conn, "2025-01-01", "2025-01-02", false).unwrap();
        assert_eq!(empty.key_total, 0);
        assert_eq!(empty.day_count, 0);
    }

    /// 趋势按小时/天/月聚合且排序稳定；小时趋势也遵循 repeat 开关。
    #[test]
    fn activity_trend_switches_granularity_and_repeat() {
        let mut conn = open_in_memory().unwrap();
        seed(&mut conn);
        let d1 = date_of(ts_at(2026, 1, 31, 10, 0));
        let d3 = date_of(ts_at(2026, 2, 2, 10, 0));

        let hourly = activity_trend(&conn, &d1, &d3, "hour", false).unwrap();
        assert_eq!(hourly.len(), 3, "3 个有数据的日期小时桶");
        assert_eq!(hourly[0].bucket, format!("{d1}T10"));
        assert_eq!((hourly[0].key_count, hourly[0].click_count), (3, 3));
        assert_eq!(hourly.iter().map(|p| p.key_count).sum::<i64>(), 5);

        let hourly_with_repeat = activity_trend(&conn, &d1, &d3, "hour", true).unwrap();
        assert_eq!(
            hourly_with_repeat[0].key_count, 4,
            "开启 repeat 后小时桶应包含自动重复"
        );

        let daily = activity_trend(&conn, &d1, &d3, "day", false).unwrap();
        assert_eq!(daily.iter().map(|p| p.key_count).sum::<i64>(), 5);
        assert_eq!(daily.iter().map(|p| p.click_count).sum::<i64>(), 4);

        let monthly = activity_trend(&conn, &d1, &d3, "month", false).unwrap();
        assert_eq!(monthly.len(), 2);
        assert_eq!(monthly[0].bucket, &d1[..7]);
        assert_eq!((monthly[0].key_count, monthly[0].click_count), (3, 3));
        assert_eq!((monthly[1].key_count, monthly[1].click_count), (2, 1));
    }

    /// 键盘统计：降序排序、总数、24 小时分布合计一致。
    #[test]
    fn keyboard_stats_sorted_and_hourly() {
        let mut conn = open_in_memory().unwrap();
        seed(&mut conn);
        let d1 = date_of(ts_at(2026, 1, 31, 10, 0));
        let d3 = date_of(ts_at(2026, 2, 2, 10, 0));

        let stats = keyboard_stats(&conn, &d1, &d3, false).unwrap();
        assert_eq!(stats.total, 5);
        assert_eq!(stats.keys[0].code, "KeyA", "同数时按 code 升序");
        assert_eq!(stats.keys[0].count, 2);
        assert_eq!(stats.keys[0].repeat_count, 1);
        assert_eq!(stats.keys[1].code, "KeyB");
        assert_eq!(stats.keys[2].code, "Space");
        assert_eq!(stats.by_hour.len(), 24);
        assert_eq!(
            stats.by_hour.iter().sum::<i64>(),
            5,
            "小时分布合计应等于总数"
        );
    }

    /// 鼠标统计：按钮分布与网格粒度合并（24→64 不丢计数）。
    #[test]
    fn mouse_stats_merges_grid_cells() {
        let mut conn = open_in_memory().unwrap();
        seed(&mut conn);
        let d1 = date_of(ts_at(2026, 1, 31, 10, 0));
        let d3 = date_of(ts_at(2026, 2, 2, 10, 0));
        let mid = monitors(&conn).unwrap()[0].id;

        let s24 = mouse_stats(&conn, &d1, &d3, 24).unwrap();
        assert_eq!(s24.total, 4);
        assert_eq!(s24.by_button[0].button, "left");
        assert_eq!(s24.by_button[0].count, 2);
        assert_eq!(
            s24.cells.iter().map(|c| c.count).sum::<i64>(),
            2,
            "两个网格点各 1 次"
        );
        assert!(s24.cells.iter().all(|c| c.monitor_id == mid));
        assert!(s24.cells.iter().any(|c| c.cell_x == 4 && c.cell_y == 8));
        assert!(s24.cells.iter().any(|c| c.cell_x == 5 && c.cell_y == 8));

        // 64px 粒度：基准格 4 中心 108 → gx=1；格 5 中心 132 → gx=2；y 中心 204 → gy=3
        let s64 = mouse_stats(&conn, &d1, &d3, 64).unwrap();
        assert_eq!(
            s64.cells.iter().map(|c| c.count).sum::<i64>(),
            2,
            "总计数不丢"
        );
        let gx: Vec<i32> = s64.cells.iter().map(|c| c.cell_x).collect();
        assert!(
            gx.contains(&1) && gx.contains(&2),
            "两个格分别映射到 1 与 2"
        );
        assert!(s64.cells.iter().all(|c| c.cell_y == 3));
    }

    /// 日历：只返回有数据的日期，月份过滤正确（跨月边界）。
    #[test]
    fn calendar_filters_by_month() {
        let mut conn = open_in_memory().unwrap();
        seed(&mut conn);
        let d1 = date_of(ts_at(2026, 1, 31, 10, 0));
        let d2 = date_of(ts_at(2026, 2, 1, 10, 0));
        let month1 = d1[..7].to_string();
        let month2 = d2[..7].to_string();
        assert_ne!(month1, month2, "测试数据必须跨月");

        let m1 = calendar(&conn, &month1, false).unwrap();
        assert_eq!(m1.len(), 1, "1 月末只有一天有数据");
        assert_eq!(m1[0].date, d1);
        assert_eq!(m1[0].key_count, 3, "KeyA×2 + Space×1");
        assert_eq!(m1[0].click_count, 3);

        let m2 = calendar(&conn, &month2, false).unwrap();
        assert_eq!(m2.len(), 2);
        assert!(m2.iter().all(|d| d.date.starts_with(&month2)));

        let m3 = calendar(&conn, "1999-01", false).unwrap();
        assert!(m3.is_empty());
    }

    /// 单日详情：Top 键、按钮分布、小时曲线。
    #[test]
    fn day_detail_contents() {
        let mut conn = open_in_memory().unwrap();
        seed(&mut conn);
        let d1 = date_of(ts_at(2026, 1, 31, 10, 0));

        let detail = day_detail(&conn, &d1, false).unwrap();
        assert_eq!(detail.date, d1);
        assert_eq!(detail.key_total, 3);
        assert_eq!(detail.click_total, 3);
        assert_eq!(detail.key_top[0].code, "KeyA");
        assert_eq!(detail.key_top[0].count, 2);
        assert_eq!(detail.by_button[0].button, "left");
        assert_eq!(detail.by_hour.iter().sum::<i64>(), 6, "键+点击合计 6");

        let none = day_detail(&conn, "1999-01-01", false).unwrap();
        assert_eq!(none.key_total, 0);
        assert_eq!(none.key_top.len(), 0);
    }

    /// 应用字典按最近使用排序，limit 生效。
    #[test]
    fn known_apps_sorted_and_limited() {
        let conn = open_in_memory().unwrap();
        let a1 = dao::upsert_app(&conn, "old.exe", 100).unwrap();
        let a2 = dao::upsert_app(&conn, "new.exe", 200).unwrap();
        let list = known_apps(&conn, 10).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, a2, "最近使用在前");
        assert_eq!(list[1].id, a1);

        let limited = known_apps(&conn, 1).unwrap();
        assert_eq!(limited.len(), 1);
    }
}
