//! 基于 `PRAGMA user_version` 的迁移器。
//!
//! 约定：`LATEST_VERSION` 是当前代码期望的版本号；每个版本对应 `schema.rs` 中一个常量。
//! 迁移在连接打开时执行，幂等——已是最新版本时不做任何事。

use rusqlite::Connection;

/// 当前代码期望的 schema 版本（新增迁移时递增并在此登记 DDL 常量）。
pub const LATEST_VERSION: i32 = 2;

/// 应用所有未执行的迁移。幂等：可重复调用。
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if current < 1 {
        conn.execute_batch(crate::db::schema::V1)?;
    }

    if current < 2 {
        conn.execute_batch(crate::db::schema::V2)?;
    }

    if current < LATEST_VERSION {
        conn.pragma_update(None, "user_version", LATEST_VERSION)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// 迁移应创建全部 10 张表、补齐小时 repeat 列并把 user_version 置为最新版本。
    #[test]
    fn migrate_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);

        let repeat_column: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('agg_hour_daily') WHERE name='repeat_count'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(repeat_column, 1, "小时聚合应包含 repeat_count 列");

        for table in [
            "sessions",
            "apps",
            "monitors",
            "key_events",
            "mouse_events",
            "agg_key_daily",
            "agg_mouse_daily",
            "agg_click_grid_daily",
            "agg_app_daily",
            "agg_hour_daily",
        ] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "表 {table} 应存在");
        }
    }

    /// 迁移必须幂等：连续执行两次不报错、版本不变、表不重复创建。
    #[test]
    fn migrate_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();

        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
    }

    /// 已存在 v1 小时表的数据库升级到 v2 时，应只增加新列且不影响原有数据。
    #[test]
    fn migrate_upgrades_v1_hour_table() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE agg_hour_daily (
               date TEXT NOT NULL,
               hour INTEGER NOT NULL,
               key_count INTEGER NOT NULL DEFAULT 0,
               click_count INTEGER NOT NULL DEFAULT 0,
               PRIMARY KEY (date, hour)
             );
             INSERT INTO agg_hour_daily(date, hour, key_count, click_count)
             VALUES ('2026-09-21', 10, 3, 2);
             PRAGMA user_version = 1;",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let (keys, clicks, repeat): (i64, i64, i64) = conn
            .query_row(
                "SELECT key_count, click_count, repeat_count FROM agg_hour_daily",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((keys, clicks, repeat), (3, 2, 0));
    }
}
