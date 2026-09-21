/**
 * 后端 IPC 封装：类型与 Rust 侧结构一一对应（`src-tauri/src/{config,commands,db/query,db/export}.rs`）。
 * 所有调用集中在此，页面不直接使用 `invoke`。
 */
import { invoke } from "@tauri-apps/api/core";

// ---------- 与 src-tauri/src/config.rs 对应 ----------

export interface AppConfig {
  /** 首启知情同意时间（RFC3339）；null = 未同意 */
  consented_at: string | null;
  capture_enabled: boolean;
  capture_keyboard: boolean;
  capture_mouse: boolean;
  repeat_counts: boolean;
  ignore_injected: boolean;
  privacy_mode: boolean;
  raw_retention_days: number;
  grid_cell_size: number;
  blacklist_keys: string[];
  blacklist_mouse: string[];
  pause_hotkey: string;
  autostart: boolean;
  db_encrypted: boolean;
  tray_hint_shown: boolean;
}

// ---------- 与 src-tauri/src/commands/mod.rs 对应 ----------

export interface RuntimeState {
  paused: boolean;
  consented: boolean;
  capture_keyboard: boolean;
  capture_mouse: boolean;
  keyboard_installed: boolean;
  mouse_installed: boolean;
  writer_ready: boolean;
  queue_len: number;
  queue_capacity: number;
  dropped_total: number;
  blocked_total: number;
  flushed_events: number;
  flush_count: number;
  last_flush_ms: number;
  write_errors: number;
  current_exe: string | null;
  monitor_count: number;
  db_bytes: number;
}

// ---------- 与 src-tauri/src/db/query.rs 对应 ----------

export interface DayCount {
  date: string;
  key_count: number;
  click_count: number;
}

export interface Overview {
  key_total: number;
  click_total: number;
  active_hours: number;
  day_count: number;
  by_day: DayCount[];
}

export interface KeyCount {
  code: string;
  count: number;
  repeat_count: number;
}

export interface KeyboardStats {
  total: number;
  keys: KeyCount[];
  by_hour: number[];
}

export interface ButtonCount {
  button: string;
  count: number;
}

export interface GridCell {
  monitor_id: number;
  cell_x: number;
  cell_y: number;
  count: number;
}

export interface MonitorRow {
  id: number;
  device_key: string;
  is_primary: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
}

export interface MouseStats {
  total: number;
  by_button: ButtonCount[];
  monitors: MonitorRow[];
  cells: GridCell[];
}

export interface DayDetail {
  date: string;
  key_total: number;
  click_total: number;
  key_top: KeyCount[];
  by_button: ButtonCount[];
  by_hour: number[];
}

export interface AppRow {
  id: number;
  exe_name: string;
  friendly_name: string | null;
  last_seen: number;
}

// ---------- 与 src-tauri/src/db/export.rs 对应 ----------

export interface ExportSummary {
  rows: number;
  bytes: number;
}

export interface DbStats {
  db_bytes: number;
  key_rows: number;
  mouse_rows: number;
  agg_rows: number;
  app_count: number;
  monitor_count: number;
  session_count: number;
  oldest_raw_date: string | null;
}

// ---------- 调用封装 ----------

export const api = {
  getRuntimeState: () => invoke<RuntimeState>("get_runtime_state"),
  getSettings: () => invoke<AppConfig>("get_settings"),
  setSettings: (config: AppConfig) => invoke<AppConfig>("set_settings", { config }),
  setPaused: (paused: boolean) => invoke<AppConfig>("set_paused", { paused }),
  grantConsent: () => invoke<AppConfig>("grant_consent"),

  getOverview: (start: string, end: string) =>
    invoke<Overview>("get_overview", { startDate: start, endDate: end }),
  getKeyboardStats: (start: string, end: string) =>
    invoke<KeyboardStats>("get_keyboard_stats", { startDate: start, endDate: end }),
  getMouseStats: (start: string, end: string) =>
    invoke<MouseStats>("get_mouse_stats", { startDate: start, endDate: end }),
  getCalendarStats: (month: string) => invoke<DayCount[]>("get_calendar_stats", { month }),
  getDayDetail: (date: string) => invoke<DayDetail>("get_day_detail", { date }),
  getMonitors: () => invoke<MonitorRow[]>("get_monitors"),
  getKnownApps: () => invoke<AppRow[]>("get_known_apps"),
  getDbStats: () => invoke<DbStats>("get_db_stats"),

  exportData: (format: "csv" | "json", scope: "agg" | "raw", start: string, end: string) =>
    invoke<ExportSummary | null>("export_data", { format, scope, startDate: start, endDate: end }),
  deleteRange: (start: string, end: string) =>
    invoke<number>("delete_range", { startDate: start, endDate: end }),
  cleanupNow: () => invoke<number>("cleanup_now"),
  openDataDir: () => invoke<void>("open_data_dir"),
  getAppVersion: () => invoke<string>("get_app_version"),
};

/** 人类可读的字节数。 */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

/** 千分位数字。 */
export function formatNumber(n: number): string {
  return n.toLocaleString("zh-CN");
}

/** 按钮标识 → 中文名（与后端 mouse_events.button 值域一致）。 */
export const BUTTON_LABELS: Record<string, string> = {
  left: "左键",
  right: "右键",
  middle: "中键",
  x1: "侧键 X1（后退）",
  x2: "侧键 X2（前进）",
};
