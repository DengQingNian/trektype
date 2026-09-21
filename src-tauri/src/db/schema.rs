//! 数据库 DDL 常量。每个版本一个常量，由 `migrations.rs` 按序应用。
//!
//! 表结构设计要点（详见计划文档第 5 节）：
//! - `key_events` / `mouse_events` 为 raw 明细（受保留期清理）；五个 `agg_*` 表为聚合（永久保留）。
//! - 聚合表以本地日期 `YYYY-MM-DD` 为主键维度，查询只打聚合表。
//! - `mouse_events` 只记 5 种按钮的按下（无滚轮、无移动轨迹）。

/// v1 初始 schema。
pub const V1: &str = r#"
-- 会话：每次进程启动一条，用于数据血缘与调试
CREATE TABLE sessions (
  id          INTEGER PRIMARY KEY,
  started_at  INTEGER NOT NULL,
  ended_at    INTEGER,
  app_version TEXT NOT NULL
);

-- 前台应用字典（exe 名去重；不含路径与窗口标题——窗口标题永不采集）
CREATE TABLE apps (
  id            INTEGER PRIMARY KEY,
  exe_name      TEXT NOT NULL UNIQUE,
  friendly_name TEXT,
  first_seen    INTEGER NOT NULL,
  last_seen     INTEGER NOT NULL
);

-- 显示器快照（布局/分辨率变化时更新 last_seen；历史快照保留以解释旧坐标）
CREATE TABLE monitors (
  id         INTEGER PRIMARY KEY,
  device_key TEXT NOT NULL UNIQUE,
  is_primary INTEGER NOT NULL DEFAULT 0,
  x          INTEGER NOT NULL,
  y          INTEGER NOT NULL,
  width      INTEGER NOT NULL,
  height     INTEGER NOT NULL,
  scale      REAL NOT NULL DEFAULT 1.0,
  first_seen INTEGER NOT NULL,
  last_seen  INTEGER NOT NULL
);

-- 键盘明细：只存键码标识+相位，不做字符合成；敏感应用事件在入库前已被丢弃
CREATE TABLE key_events (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  ts          INTEGER NOT NULL,
  session_id  INTEGER NOT NULL REFERENCES sessions(id),
  key_code    TEXT NOT NULL,
  phase       INTEGER NOT NULL,
  is_repeat   INTEGER NOT NULL DEFAULT 0,
  is_injected INTEGER NOT NULL DEFAULT 0,
  app_id      INTEGER REFERENCES apps(id)
);
CREATE INDEX idx_key_events_ts ON key_events(ts);
CREATE INDEX idx_key_events_code_ts ON key_events(key_code, ts);

-- 鼠标明细：只记 left/right/middle/x1/x2 的按下
CREATE TABLE mouse_events (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  ts         INTEGER NOT NULL,
  session_id INTEGER NOT NULL REFERENCES sessions(id),
  button     TEXT NOT NULL,
  x          INTEGER NOT NULL,
  y          INTEGER NOT NULL,
  monitor_id INTEGER REFERENCES monitors(id),
  app_id     INTEGER REFERENCES apps(id)
);
CREATE INDEX idx_mouse_ts ON mouse_events(ts);

-- ===== 聚合表（批量写入同事务增量 upsert；查询只打这里） =====

-- 键盘日聚合：热力图 / Top 键 / 占比。
-- 双计数列以支持 repeat 口径切换（拷问 Q1）：count 为非 repeat 按下，repeat_count 为 OS 自动重复；
-- 查询层按设置选择 count 或 count+repeat_count，无需回扫 raw。
CREATE TABLE agg_key_daily (
  date         TEXT NOT NULL,
  key_code     TEXT NOT NULL,
  count        INTEGER NOT NULL DEFAULT 0,
  repeat_count INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (date, key_code)
);

-- 鼠标日聚合：五类按钮分布
CREATE TABLE agg_mouse_daily (
  date   TEXT NOT NULL,
  button TEXT NOT NULL,
  count  INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (date, button)
);

-- 点击网格日聚合：屏幕热力图专用，避免渲染时扫 raw（cell 基准 24px）
CREATE TABLE agg_click_grid_daily (
  date       TEXT NOT NULL,
  monitor_id INTEGER NOT NULL REFERENCES monitors(id),
  cell_x     INTEGER NOT NULL,
  cell_y     INTEGER NOT NULL,
  count      INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (date, monitor_id, cell_x, cell_y)
);

-- 应用日聚合：按应用统计
CREATE TABLE agg_app_daily (
  date        TEXT NOT NULL,
  app_id      INTEGER NOT NULL REFERENCES apps(id),
  key_count   INTEGER NOT NULL DEFAULT 0,
  click_count INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (date, app_id)
);

-- 小时聚合：日内趋势
CREATE TABLE agg_hour_daily (
  date        TEXT NOT NULL,
  hour        INTEGER NOT NULL,
  key_count   INTEGER NOT NULL DEFAULT 0,
  click_count INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (date, hour)
);
"#;

/// v2：小时聚合补充自动重复按键计数，使趋势图与全局 repeat_counts 口径一致。
pub const V2: &str = r#"
ALTER TABLE agg_hour_daily ADD COLUMN repeat_count INTEGER NOT NULL DEFAULT 0;
"#;
