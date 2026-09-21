//! 多显示器快照与坐标归属。
//!
//! - 快照：`EnumDisplayMonitors` 枚举所有显示器（虚拟桌面物理像素坐标，原点可位于主屏左上，
//!   主屏左侧/上方的显示器坐标为负）。
//! - 归属：`point_to_monitor` 纯函数把点击坐标映射到显示器（左上含、右下不含，Windows RECT 约定）。
//! - 刷新：监视线程 30s 轮询，布局变化（含热插拔/分辨率/DPI 变更）时更新内存缓存并标记 dirty，
//!   由写入器在下次 flush 时落库并回填数据库 id。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::RwLock;

/// 一台显示器的快照。
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorInfo {
    /// 数据库 id（由写入器 upsert 后回填；未落库时为 None）
    pub id: Option<i64>,
    /// 稳定标识（Windows 为 `\\.\DISPLAY1` 形式）
    pub device_key: String,
    pub is_primary: bool,
    /// 虚拟桌面坐标（物理像素）
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// DPI 缩放（1.0 = 96 DPI）
    pub scale: f64,
}

/// 显示器内存缓存（监视线程写，写入器/命令层读）。
pub struct MonitorCache {
    inner: RwLock<Vec<MonitorInfo>>,
    /// 布局有变化待落库（写入器消费）
    dirty: AtomicBool,
    /// 布局版本号（每次变化递增；诊断与测试用）
    revision: AtomicU64,
}

impl Default for MonitorCache {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitorCache {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Vec::new()),
            dirty: AtomicBool::new(false),
            revision: AtomicU64::new(0),
        }
    }

    /// 用新快照替换缓存；返回是否发生变化（无变化时不置 dirty，避免无谓落库）。
    pub fn replace(&self, list: Vec<MonitorInfo>) -> bool {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        // 比较时忽略 id：id 由写入器回填，布局是否变化的判据是几何与设备键
        let changed = {
            let old: Vec<&MonitorInfo> = guard.iter().collect();
            let new: Vec<&MonitorInfo> = list.iter().collect();
            if old.len() != new.len() {
                true
            } else {
                old.iter()
                    .zip(new.iter())
                    .any(|(a, b)| strip_id(a) != strip_id(b))
            }
        };
        if changed {
            *guard = list;
            self.dirty.store(true, Ordering::SeqCst);
            self.revision.fetch_add(1, Ordering::SeqCst);
        }
        changed
    }

    pub fn snapshot(&self) -> Vec<MonitorInfo> {
        self.inner.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::SeqCst)
    }

    pub fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::SeqCst);
    }

    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::SeqCst)
    }

    /// 写入器落库后回填数据库 id（按 device_key 匹配）。
    pub fn set_ids(&self, ids: &HashMap<String, i64>) {
        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
        for m in guard.iter_mut() {
            if let Some(id) = ids.get(&m.device_key) {
                m.id = Some(*id);
            }
        }
    }

    /// 点击坐标 → 显示器 id（id 尚未回填时返回 None，事件仍入库但 monitor_id 为空）。
    pub fn point_to_monitor(&self, x: i32, y: i32) -> Option<i64> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        point_to_monitor(x, y, &guard)
    }

    /// 点击坐标 → 命中的显示器完整信息（网格归属需要显示器原点）。
    pub fn monitor_at(&self, x: i32, y: i32) -> Option<MonitorInfo> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        guard
            .iter()
            .find(|m| point_in_rect(x, y, m.x, m.y, m.width, m.height))
            .cloned()
    }
}

/// 屏幕点击热力图网格边长（物理像素，基准值）。
/// 聚合表按此基准存格；查询层只支持向更粗粒度（32/64）合并，不做拆分。
pub const GRID_CELL_PX: i32 = 24;

/// 点击坐标 → 该显示器内的网格索引（以显示器左上角为原点，越界钳制到 0）。
pub fn grid_cell(x: i32, y: i32, m: &MonitorInfo) -> (i32, i32) {
    (
        (x - m.x).max(0) / GRID_CELL_PX,
        (y - m.y).max(0) / GRID_CELL_PX,
    )
}

/// 忽略 id 的比较视图（布局变化判定用）。
fn strip_id(m: &MonitorInfo) -> (&str, bool, i32, i32, i32, i32, u64) {
    (
        m.device_key.as_str(),
        m.is_primary,
        m.x,
        m.y,
        m.width,
        m.height,
        m.scale.to_bits(),
    )
}

/// 点是否在矩形内：左上含、右下不含（Windows RECT 约定），支持负坐标。
pub fn point_in_rect(x: i32, y: i32, rx: i32, ry: i32, w: i32, h: i32) -> bool {
    x >= rx && x < rx.saturating_add(w) && y >= ry && y < ry.saturating_add(h)
}

/// 坐标归属：返回第一个命中的显示器 id；未覆盖（或 id 未回填）返回 None。
pub fn point_to_monitor(x: i32, y: i32, monitors: &[MonitorInfo]) -> Option<i64> {
    monitors
        .iter()
        .find(|m| point_in_rect(x, y, m.x, m.y, m.width, m.height))
        .and_then(|m| m.id)
}

#[cfg(windows)]
mod win {
    use super::MonitorInfo;
    use std::sync::atomic::Ordering;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{LPARAM, RECT, TRUE};
    use windows::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    /// MONITORINFO.dwFlags 的主显示器标志（windows crate 未导出该常量）。
    const MONITORINFOF_PRIMARY: u32 = 0x1;

    /// 枚举当前所有显示器（虚拟桌面物理像素 + DPI 缩放）。
    pub fn snapshot_monitors() -> Vec<MonitorInfo> {
        let mut list: Vec<MonitorInfo> = Vec::new();
        let lparam = LPARAM(&mut list as *mut Vec<MonitorInfo> as isize);

        unsafe extern "system" fn enum_proc(
            hmon: HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            lparam: LPARAM,
        ) -> BOOL {
            let list = unsafe { &mut *(lparam.0 as *mut Vec<MonitorInfo>) };
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;

            if unsafe { GetMonitorInfoW(hmon, &mut info.monitorInfo) }.as_bool() {
                let device_key = String::from_utf16_lossy(&info.szDevice)
                    .trim_end_matches('\0')
                    .to_string();
                let mut dpi_x: u32 = 96;
                let mut dpi_y: u32 = 96;
                let _ =
                    unsafe { GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
                let rc = info.monitorInfo.rcMonitor;
                list.push(MonitorInfo {
                    id: None,
                    device_key,
                    is_primary: (info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY) != 0,
                    x: rc.left,
                    y: rc.top,
                    width: rc.right - rc.left,
                    height: rc.bottom - rc.top,
                    scale: dpi_x as f64 / 96.0,
                });
            }
            TRUE
        }

        unsafe {
            let _ = EnumDisplayMonitors(None, None, Some(enum_proc), lparam);
        }
        list
    }

    /// 启动显示器监视线程：30s 轮询，布局变化时更新缓存并置 dirty。
    pub fn spawn_monitor_watcher(
        shared: std::sync::Arc<crate::capture::CaptureShared>,
    ) -> std::io::Result<std::thread::JoinHandle<()>> {
        std::thread::Builder::new()
            .name("monitor-watcher".to_string())
            .spawn(move || {
                // 启动即拍照一次，保证缓存尽快可用
                shared.monitors.replace(snapshot_monitors());
                while !shared.stop_requested.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    if shared.stop_requested.load(Ordering::SeqCst) {
                        break;
                    }
                    shared.monitors.replace(snapshot_monitors());
                }
            })
    }
}

#[cfg(windows)]
pub use win::{snapshot_monitors, spawn_monitor_watcher};

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(id: i64, key: &str, x: i32, y: i32, w: i32, h: i32, primary: bool) -> MonitorInfo {
        MonitorInfo {
            id: Some(id),
            device_key: key.to_string(),
            is_primary: primary,
            x,
            y,
            width: w,
            height: h,
            scale: 1.0,
        }
    }

    /// 单屏基本归属：内部点命中、右下边界不含、外部点不命中。
    #[test]
    fn single_monitor_bounds_are_half_open() {
        let m = mon(1, "DISPLAY1", 0, 0, 1920, 1080, true);
        let list = vec![m];

        assert_eq!(point_to_monitor(0, 0, &list), Some(1), "左上角含");
        assert_eq!(
            point_to_monitor(1919, 1079, &list),
            Some(1),
            "右下角内最后一点"
        );
        assert_eq!(point_to_monitor(1920, 1079, &list), None, "右边界不含");
        assert_eq!(point_to_monitor(0, 1080, &list), None, "下边界不含");
        assert_eq!(point_to_monitor(-1, 0, &list), None, "左侧负坐标不命中");
    }

    /// 主屏左侧的副屏：负坐标区域必须正确归属（拷问 M3 验收场景）。
    #[test]
    fn negative_coordinate_secondary_monitor() {
        let primary = mon(1, "DISPLAY1", 0, 0, 1920, 1080, true);
        let left = mon(2, "DISPLAY2", -1920, 0, 1920, 1080, false);
        let list = vec![primary, left];

        assert_eq!(point_to_monitor(-1920, 0, &list), Some(2));
        assert_eq!(point_to_monitor(-1, 500, &list), Some(2));
        assert_eq!(point_to_monitor(-1921, 500, &list), None, "最左边界外");
        assert_eq!(point_to_monitor(0, 500, &list), Some(1), "0 属于主屏");
    }

    /// 上下堆叠 + DPI 不同的组合屏：归属互不干扰。
    #[test]
    fn stacked_monitors_with_different_dpi() {
        let bottom = MonitorInfo {
            scale: 1.0,
            ..mon(1, "DISPLAY1", 0, 1080, 2560, 1440, true)
        };
        let top = MonitorInfo {
            scale: 1.5,
            ..mon(2, "DISPLAY2", 0, 0, 1920, 1080, false)
        };
        let list = vec![bottom, top];

        assert_eq!(point_to_monitor(100, 50, &list), Some(2), "上方副屏");
        assert_eq!(point_to_monitor(100, 2000, &list), Some(1), "下方主屏");
        assert_eq!(
            point_to_monitor(2500, 1080, &list),
            Some(1),
            "主屏宽 2560，2500 仍在其内"
        );
        assert_eq!(point_to_monitor(2700, 1080, &list), None, "主屏右边界外");
    }

    /// id 未回填（尚未落库）时返回 None，不 panic。
    #[test]
    fn unassigned_id_returns_none() {
        let mut m = mon(0, "DISPLAY1", 0, 0, 800, 600, true);
        m.id = None;
        assert_eq!(point_to_monitor(10, 10, &[m]), None);
    }

    /// 布局变化检测：几何相同不置 dirty；分辨率/位置变化置 dirty；仅 id 回填不算变化。
    #[test]
    fn cache_replace_detects_layout_changes_only() {
        let cache = MonitorCache::new();
        let a = vec![mon(0, "DISPLAY1", 0, 0, 1920, 1080, true)];
        // 初始替换：有变化
        assert!(cache.replace(a.clone()));
        assert!(cache.is_dirty());
        let rev1 = cache.revision();

        // 仅回填 id：不得视为布局变化
        cache.clear_dirty();
        let mut ids = HashMap::new();
        ids.insert("DISPLAY1".to_string(), 7);
        cache.set_ids(&ids);
        let same_geometry = vec![mon(7, "DISPLAY1", 0, 0, 1920, 1080, true)];
        assert!(
            !cache.replace(same_geometry),
            "只有 id 变化不应判定为布局变化"
        );
        assert!(!cache.is_dirty(), "无变化不应置 dirty");
        assert_eq!(cache.revision(), rev1);

        // 分辨率变化：视为布局变化
        let resized = vec![mon(7, "DISPLAY1", 0, 0, 2560, 1440, true)];
        assert!(cache.replace(resized));
        assert!(cache.is_dirty());
        assert_eq!(cache.revision(), rev1 + 1);
    }

    /// 热插拔模拟：新增显示器后归属正确切换。
    #[test]
    fn hotplug_adds_monitor() {
        let cache = MonitorCache::new();
        let single = vec![mon(1, "DISPLAY1", 0, 0, 1920, 1080, true)];
        cache.replace(single);
        assert_eq!(
            cache.point_to_monitor(-500, 100),
            None,
            "副屏未接入时无归属"
        );

        let dual = vec![
            mon(1, "DISPLAY1", 0, 0, 1920, 1080, true),
            mon(2, "DISPLAY2", -1920, 0, 1920, 1080, false),
        ];
        assert!(cache.replace(dual));
        assert_eq!(
            cache.point_to_monitor(-500, 100),
            Some(2),
            "接入后应归属副屏"
        );
        assert_eq!(cache.snapshot().len(), 2);
    }

    /// 溢出安全：极端坐标不 panic（saturating 边界）。
    #[test]
    fn extreme_coordinates_do_not_panic() {
        let m = mon(1, "DISPLAY1", i32::MAX - 10, 0, 100, 100, true);
        let list = vec![m];
        let _ = point_to_monitor(i32::MAX, 50, &list);
        let _ = point_to_monitor(i32::MIN, i32::MIN, &list);
        assert_eq!(point_to_monitor(i32::MAX - 5, 50, &list), Some(1));
    }

    /// 网格索引：以显示器左上角为原点、24px 为格边长；负偏移钳制到 0。
    #[test]
    fn grid_cell_indexing() {
        let m = mon(1, "DISPLAY1", 0, 0, 1920, 1080, true);
        assert_eq!(grid_cell(0, 0, &m), (0, 0));
        assert_eq!(grid_cell(23, 23, &m), (0, 0));
        assert_eq!(grid_cell(24, 24, &m), (1, 1));
        assert_eq!(grid_cell(1919, 1079, &m), (79, 44));

        // 副屏（负坐标原点）：索引相对该屏自身左上角
        let left = mon(2, "DISPLAY2", -1920, 0, 1920, 1080, false);
        assert_eq!(grid_cell(-1920, 0, &left), (0, 0));
        assert_eq!(grid_cell(-1, 47, &left), (79, 1));
        // 越界（理论上不会发生）钳制到 0，不产生负索引
        assert_eq!(grid_cell(-2000, -50, &left), (0, 0));
    }
}
