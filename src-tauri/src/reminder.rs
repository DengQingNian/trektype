//! 休息提醒：基于已提交的输入事件判断每日上限和短期密集输入。

use crate::config::AppConfig;
use crate::db::ts_to_local_date;
use rusqlite::{params, Connection};
use std::collections::VecDeque;

const COOLDOWN_MS: i64 = 30 * 60 * 1_000;
const MAX_WINDOW_MS: i64 = 30 * 60 * 1_000;

/// 提醒统计仅有两种输入；按键抬起、鼠标移动和滚轮均不进入此层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Key,
    Click,
}

impl Metric {
    fn index(self) -> usize {
        match self {
            Self::Key => 0,
            Self::Click => 1,
        }
    }
    fn db_name(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Click => "click",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Key => "按键",
            Self::Click => "点击",
        }
    }
}

/// 一次已提交的有效输入。自动重复标志只对键盘有意义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReminderInput {
    pub ts_ms: i64,
    pub metric: Metric,
    pub is_repeat: bool,
}

#[derive(Debug)]
struct Reason {
    metric: Metric,
    daily_date: Option<String>,
    count: i64,
    limit: u32,
    window_minutes: Option<u32>,
}

impl Reason {
    fn line(&self) -> String {
        match self.window_minutes {
            Some(minutes) => format!(
                "过去 {minutes} 分钟{} {} 次，已达提醒值 {} 次",
                self.metric.label(),
                self.count,
                self.limit
            ),
            None => format!(
                "{} 每日{} {} 次，已达提醒值 {} 次",
                self.daily_date.as_deref().unwrap_or(""),
                self.metric.label(),
                self.count,
                self.limit
            ),
        }
    }
}

/// 短期窗口和冷却在内存中维护；每日已提醒状态由数据库保存。
#[derive(Default)]
pub struct ReminderEngine {
    recent_keys: VecDeque<(i64, bool)>,
    recent_clicks: VecDeque<i64>,
    last_burst_sent: [Option<i64>; 2],
}

impl ReminderEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// 只应在输入批次成功提交后调用。通知失败时不确认触发，下一批可重试。
    pub fn process_committed(
        &mut self,
        conn: &Connection,
        cfg: &AppConfig,
        inputs: &[ReminderInput],
        now_ms: i64,
        mut send: impl FnMut(&str) -> Result<(), String>,
    ) -> Result<(), String> {
        if inputs.is_empty() {
            return Ok(());
        }
        let mut key_dates = Vec::new();
        let mut click_dates = Vec::new();
        for input in inputs {
            match input.metric {
                Metric::Key => {
                    self.recent_keys.push_back((input.ts_ms, input.is_repeat));
                    if cfg.repeat_counts || !input.is_repeat {
                        let date = ts_to_local_date(input.ts_ms);
                        if !key_dates.contains(&date) {
                            key_dates.push(date);
                        }
                    }
                }
                Metric::Click => {
                    self.recent_clicks.push_back(input.ts_ms);
                    let date = ts_to_local_date(input.ts_ms);
                    if !click_dates.contains(&date) {
                        click_dates.push(date);
                    }
                }
            }
        }
        let max_cutoff = now_ms - MAX_WINDOW_MS;
        self.recent_keys.retain(|(ts, _)| *ts > max_cutoff);
        self.recent_clicks.retain(|ts| *ts > max_cutoff);

        let mut reasons = Vec::new();
        for (metric, dates, daily_on, daily_limit, burst_on, burst_limit) in [
            (
                Metric::Key,
                key_dates,
                cfg.daily_key_reminder_enabled,
                cfg.daily_key_limit,
                cfg.burst_key_reminder_enabled,
                cfg.burst_key_limit,
            ),
            (
                Metric::Click,
                click_dates,
                cfg.daily_click_reminder_enabled,
                cfg.daily_click_limit,
                cfg.burst_click_reminder_enabled,
                cfg.burst_click_limit,
            ),
        ] {
            if dates.is_empty() {
                continue;
            }
            if daily_on {
                for date in dates {
                    let sent: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM reminder_daily_sent WHERE date=?1 AND metric=?2)",
                    params![date, metric.db_name()], |r| r.get(0),
                ).map_err(|e| e.to_string())?;
                    if !sent {
                        let count: i64 = match metric {
                        Metric::Key => {
                            let expr = if cfg.repeat_counts { "count+repeat_count" } else { "count" };
                            conn.query_row(&format!("SELECT COALESCE(SUM({expr}),0) FROM agg_key_daily WHERE date=?1"), [&date], |r| r.get(0))
                        }
                        Metric::Click => conn.query_row("SELECT COALESCE(SUM(count),0) FROM agg_mouse_daily WHERE date=?1", [&date], |r| r.get(0)),
                    }.map_err(|e| e.to_string())?;
                        if count >= daily_limit as i64 {
                            reasons.push(Reason {
                                metric,
                                daily_date: Some(date),
                                count,
                                limit: daily_limit,
                                window_minutes: None,
                            });
                        }
                    }
                }
            }
            let cooled = self.last_burst_sent[metric.index()]
                .map(|last| now_ms.saturating_sub(last) >= COOLDOWN_MS)
                .unwrap_or(true);
            if burst_on && cooled {
                let cutoff = now_ms - i64::from(cfg.burst_window_minutes) * 60_000;
                let count = match metric {
                    Metric::Key => self
                        .recent_keys
                        .iter()
                        .filter(|(ts, repeat)| *ts > cutoff && (cfg.repeat_counts || !repeat))
                        .count(),
                    Metric::Click => self.recent_clicks.iter().filter(|ts| **ts > cutoff).count(),
                } as i64;
                if count >= burst_limit as i64 {
                    reasons.push(Reason {
                        metric,
                        daily_date: None,
                        count,
                        limit: burst_limit,
                        window_minutes: Some(cfg.burst_window_minutes),
                    });
                }
            }
        }
        if reasons.is_empty() {
            return Ok(());
        }
        let body = format!(
            "{}。建议休息一下。",
            reasons
                .iter()
                .map(Reason::line)
                .collect::<Vec<_>>()
                .join("；")
        );
        send(&body)?;
        for reason in reasons {
            if let Some(date) = reason.daily_date {
                conn.execute(
                    "INSERT OR IGNORE INTO reminder_daily_sent(date, metric, notified_at) VALUES (?1, ?2, ?3)",
                    params![date, reason.metric.db_name(), now_ms],
                ).map_err(|e| e.to_string())?;
            } else {
                self.last_burst_sent[reason.metric.index()] = Some(now_ms);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, ts_to_local_date};

    fn key(ts_ms: i64, is_repeat: bool) -> ReminderInput {
        ReminderInput {
            ts_ms,
            metric: Metric::Key,
            is_repeat,
        }
    }
    fn click(ts_ms: i64) -> ReminderInput {
        ReminderInput {
            ts_ms,
            metric: Metric::Click,
            is_repeat: false,
        }
    }
    fn base() -> i64 {
        1_700_000_000_000
    }

    /// 恰好达到每日阈值时提醒一次；新引擎模拟重启后不能重复提醒。
    #[test]
    fn daily_limit_is_exact_and_persists_across_restart() {
        let conn = open_in_memory().unwrap();
        let date = ts_to_local_date(base());
        conn.execute(
            "INSERT INTO agg_key_daily(date,key_code,count) VALUES (?1,'KeyA',2)",
            [&date],
        )
        .unwrap();
        let cfg = AppConfig {
            daily_key_reminder_enabled: true,
            daily_key_limit: 2,
            ..AppConfig::default()
        };
        let mut engine = ReminderEngine::new();
        let mut sent = Vec::new();
        engine
            .process_committed(&conn, &cfg, &[key(base(), false)], base(), |body| {
                sent.push(body.to_string());
                Ok(())
            })
            .unwrap();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].contains("每日按键"));
        let mut restarted = ReminderEngine::new();
        restarted
            .process_committed(&conn, &cfg, &[key(base() + 1, false)], base() + 1, |_| {
                sent.push("duplicate".into());
                Ok(())
            })
            .unwrap();
        assert_eq!(sent.len(), 1);
    }

    /// 滚动窗口左边界不计入；达到阈值后 30 分钟冷却结束才可再提醒。
    #[test]
    fn rolling_boundary_and_cooldown() {
        let conn = open_in_memory().unwrap();
        let cfg = AppConfig {
            burst_click_reminder_enabled: true,
            burst_click_limit: 2,
            burst_window_minutes: 5,
            ..AppConfig::default()
        };
        let mut engine = ReminderEngine::new();
        let mut n = 0;
        engine
            .process_committed(&conn, &cfg, &[click(base())], base(), |_| {
                n += 1;
                Ok(())
            })
            .unwrap();
        engine
            .process_committed(
                &conn,
                &cfg,
                &[click(base() + 300_000)],
                base() + 300_000,
                |_| {
                    n += 1;
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(n, 0, "恰好五分钟前的点击已出窗口");
        engine
            .process_committed(
                &conn,
                &cfg,
                &[click(base() + 300_001)],
                base() + 300_001,
                |_| {
                    n += 1;
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(n, 1);
        let t = base() + 1_800_000;
        engine
            .process_committed(&conn, &cfg, &[click(t), click(t + 1)], t + 1, |_| {
                n += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(n, 1, "冷却内不重复提醒");
        let after_cooldown = base() + 2_100_002;
        engine
            .process_committed(
                &conn,
                &cfg,
                &[click(after_cooldown), click(after_cooldown + 1)],
                after_cooldown + 1,
                |_| {
                    n += 1;
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(n, 2);
    }

    /// 自动重复只在 repeat_counts 打开时计数，短期窗口能跨本地午夜。
    #[test]
    fn repeat_policy_and_midnight_window() {
        let conn = open_in_memory().unwrap();
        let tomorrow = crate::db::local_date_start_ms("2026-09-30").unwrap();
        let mut cfg = AppConfig {
            burst_key_reminder_enabled: true,
            burst_key_limit: 2,
            ..AppConfig::default()
        };
        let mut engine = ReminderEngine::new();
        let mut n = 0;
        engine
            .process_committed(
                &conn,
                &cfg,
                &[key(tomorrow - 1_000, false), key(tomorrow - 500, true)],
                tomorrow - 500,
                |_| {
                    n += 1;
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(n, 0);
        cfg.repeat_counts = true;
        engine
            .process_committed(
                &conn,
                &cfg,
                &[key(tomorrow + 100, true)],
                tomorrow + 100,
                |_| {
                    n += 1;
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    /// 系统通知失败时不记录已提醒状态；下一次输入可重试。
    #[test]
    fn failed_notification_retries() {
        let conn = open_in_memory().unwrap();
        let date = ts_to_local_date(base());
        conn.execute(
            "INSERT INTO agg_mouse_daily(date,button,count) VALUES (?1,'left',2)",
            [&date],
        )
        .unwrap();
        let cfg = AppConfig {
            daily_click_reminder_enabled: true,
            daily_click_limit: 2,
            ..AppConfig::default()
        };
        let mut engine = ReminderEngine::new();
        assert!(engine
            .process_committed(&conn, &cfg, &[click(base())], base(), |_| Err(
                "通知失败".into()
            ))
            .is_err());
        let mut n = 0;
        engine
            .process_committed(&conn, &cfg, &[click(base() + 1)], base() + 1, |_| {
                n += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(n, 1);
    }

    /// 本地日期变化后每日提醒重新开放，且每日按键计数遵循 repeat_counts。
    #[test]
    fn daily_reminder_resets_at_local_midnight_and_counts_repeat_when_enabled() {
        let conn = open_in_memory().unwrap();
        let midnight = crate::db::local_date_start_ms("2026-09-30").unwrap();
        let before = ts_to_local_date(midnight - 1);
        let after = ts_to_local_date(midnight);
        for date in [&before, &after] {
            conn.execute("INSERT INTO agg_key_daily(date,key_code,count,repeat_count) VALUES (?1,'KeyA',1,1)", [date]).unwrap();
        }
        let mut cfg = AppConfig {
            daily_key_reminder_enabled: true,
            daily_key_limit: 2,
            ..AppConfig::default()
        };
        let mut engine = ReminderEngine::new();
        let mut n = 0;
        engine
            .process_committed(
                &conn,
                &cfg,
                &[key(midnight - 1, false)],
                midnight - 1,
                |_| {
                    n += 1;
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(n, 0, "默认排除自动重复，未达 2 次");
        cfg.repeat_counts = true;
        engine
            .process_committed(
                &conn,
                &cfg,
                &[key(midnight - 1, true)],
                midnight - 1,
                |_| {
                    n += 1;
                    Ok(())
                },
            )
            .unwrap();
        engine
            .process_committed(&conn, &cfg, &[key(midnight, false)], midnight, |_| {
                n += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(n, 2, "跨午夜每日提醒应分别触发");
    }

    /// 同一批的两种输入、每日和短期条件合并为一次通知。
    #[test]
    fn simultaneous_reasons_share_one_notification() {
        let conn = open_in_memory().unwrap();
        let date = ts_to_local_date(base());
        conn.execute(
            "INSERT INTO agg_key_daily(date,key_code,count) VALUES (?1,'KeyA',1)",
            [&date],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO agg_mouse_daily(date,button,count) VALUES (?1,'left',1)",
            [&date],
        )
        .unwrap();
        let cfg = AppConfig {
            daily_key_reminder_enabled: true,
            daily_key_limit: 1,
            daily_click_reminder_enabled: true,
            daily_click_limit: 1,
            burst_key_reminder_enabled: true,
            burst_key_limit: 1,
            burst_click_reminder_enabled: true,
            burst_click_limit: 1,
            ..AppConfig::default()
        };
        let mut engine = ReminderEngine::new();
        let mut messages = Vec::new();
        engine
            .process_committed(
                &conn,
                &cfg,
                &[key(base(), false), click(base())],
                base(),
                |body| {
                    messages.push(body.to_string());
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(messages.len(), 1);
        for phrase in [
            "每日按键",
            "每日点击",
            "过去 10 分钟按键",
            "过去 10 分钟点击",
        ] {
            assert!(messages[0].contains(phrase), "缺少 {phrase}");
        }
    }

    /// 一个写入批次恰好跨午夜时，两天的每日上限都应被判断并记录。
    #[test]
    fn one_batch_crossing_midnight_checks_both_dates() {
        let conn = open_in_memory().unwrap();
        let midnight = crate::db::local_date_start_ms("2026-09-30").unwrap();
        for ts in [midnight - 1, midnight] {
            let date = ts_to_local_date(ts);
            conn.execute(
                "INSERT INTO agg_mouse_daily(date,button,count) VALUES (?1,'left',1)",
                [&date],
            )
            .unwrap();
        }
        let cfg = AppConfig {
            daily_click_reminder_enabled: true,
            daily_click_limit: 1,
            ..AppConfig::default()
        };
        let mut engine = ReminderEngine::new();
        let mut n = 0;
        engine
            .process_committed(
                &conn,
                &cfg,
                &[click(midnight - 1), click(midnight)],
                midnight,
                |_| {
                    n += 1;
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(n, 1, "同批多原因只发一条通知");
        let saved: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM reminder_daily_sent WHERE metric='click'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(saved, 2, "午夜两侧的每日提醒状态都应保存");
    }
}
