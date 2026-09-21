//! 基于 `PRAGMA user_version` 的迁移器。
//!
//! 约定：`LATEST_VERSION` 是当前代码期望的版本号；每个版本对应 `schema.rs` 中一个常量。
//! 迁移在连接打开时执行，幂等——已是最新版本时不做任何事。

use rusqlite::Connection;

/// 当前代码期望的 schema 版本（新增迁移时递增并在此登记 DDL 常量）。
pub const LATEST_VERSION: i32 = 1;

/// 应用所有未执行的迁移。幂等：可重复调用。
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if current < 1 {
        conn.execute_batch(crate::db::schema::V1)?;
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

    /// 迁移应创建全部 10 张表并把 user_version 置为 1。
    #[test]
    fn migrate_creates_all_tables() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);

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
}
