//! 前台窗口 → 进程 exe 名解析与缓存。
//!
//! 钩子回调只快照 HWND（`GetForegroundWindow`，微秒级），exe 名解析（`OpenProcess` +
//! `QueryFullProcessImageNameW`）由独立线程以 250ms 轮询完成；解析结果按 HWND 缓存，
//! 写入器在 flush 时按事件携带的 HWND 反查。
//!
//! **窗口标题永不采集**（拷问决策 Q3）：本模块不包含 `GetWindowTextW` 路径——
//! 该 API 还可能在窗口无响应时阻塞，双重理由下彻底不用。

use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;

/// HWND → exe 名的有界缓存（FIFO 淘汰，避免长时间运行后无限增长）。
pub struct ForegroundCache {
    inner: RwLock<Inner>,
    capacity: usize,
}

struct Inner {
    map: HashMap<isize, String>,
    order: VecDeque<isize>,
    /// 最近一次解析成功的 exe 名（UI 展示"当前前台应用"）
    latest: Option<String>,
}

impl Default for ForegroundCache {
    fn default() -> Self {
        Self::with_capacity(64)
    }
}

impl ForegroundCache {
    /// 默认容量 64 个窗口句柄（覆盖正常的多窗口切换场景）。
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: RwLock::new(Inner {
                map: HashMap::with_capacity(capacity),
                order: VecDeque::with_capacity(capacity),
                latest: None,
            }),
            capacity: capacity.max(1),
        }
    }

    /// 记录一次解析结果（重复 HWND 则更新 exe 名，例如窗口换了进程）。
    pub fn record(&self, hwnd: isize, exe: String) {
        if hwnd == 0 {
            return;
        }
        let mut g = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if g.map.insert(hwnd, exe.clone()).is_none() {
            g.order.push_back(hwnd);
        }
        g.latest = Some(exe);
        while g.order.len() > self.capacity {
            if let Some(old) = g.order.pop_front() {
                g.map.remove(&old);
            }
        }
    }

    /// 查询某窗口对应的 exe 名。
    pub fn exe_for(&self, hwnd: isize) -> Option<String> {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .map
            .get(&hwnd)
            .cloned()
    }

    /// 最近解析到的 exe 名。
    pub fn latest(&self) -> Option<String> {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .latest
            .clone()
    }

    pub fn len(&self) -> usize {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .map
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(windows)]
mod win {

    use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    /// 解析窗口所属进程的可执行文件名（小写，如 `chrome.exe`）。
    /// 任何失败（权限不足/系统进程/窗口已销毁）静默返回 None。
    pub fn resolve_exe_name(hwnd: isize) -> Option<String> {
        if hwnd == 0 {
            return None;
        }
        unsafe {
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
            if pid == 0 {
                return None;
            }

            // PROCESS_QUERY_LIMITED_INFORMATION：对多数进程（含更高完整性级别）可用，
            // 无需管理员权限；失败即放弃（事件仍记录，app_id 为空）
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

            let mut buf = [0u16; MAX_PATH as usize];
            let mut len = buf.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(handle);
            result.ok()?;

            let path = String::from_utf16_lossy(&buf[..len as usize]);
            std::path::Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().to_lowercase())
        }
    }

    /// 取当前前台窗口句柄（回调热路径也用它，微秒级）。
    pub fn foreground_hwnd() -> isize {
        unsafe { GetForegroundWindow() }.0 as isize
    }

    /// 启动前台应用监视线程：250ms 轮询，HWND 变化时解析 exe 名并写入缓存。
    pub fn spawn_foreground_watcher(
        shared: std::sync::Arc<crate::capture::CaptureShared>,
    ) -> std::io::Result<std::thread::JoinHandle<()>> {
        std::thread::Builder::new()
            .name("foreground-watcher".to_string())
            .spawn(move || {
                let mut last: isize = 0;
                while !shared
                    .stop_requested
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    let hwnd = foreground_hwnd();
                    if hwnd != last {
                        last = hwnd;
                        if let Some(exe) = resolve_exe_name(hwnd) {
                            shared.foreground.record(hwnd, exe);
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(250));
                }
            })
    }
}

#[cfg(windows)]
pub use win::{foreground_hwnd, resolve_exe_name, spawn_foreground_watcher};

#[cfg(test)]
mod tests {
    use super::*;

    /// 基本读写：记录后可查到，未记录的 HWND 返回 None。
    #[test]
    fn record_and_lookup() {
        let c = ForegroundCache::default();
        assert!(c.exe_for(1234).is_none());
        c.record(1234, "chrome.exe".to_string());
        assert_eq!(c.exe_for(1234).as_deref(), Some("chrome.exe"));
        assert_eq!(c.latest().as_deref(), Some("chrome.exe"));
        assert_eq!(c.len(), 1);
    }

    /// 同一 HWND 重复记录应更新而非新增（窗口切换进程的边界情况）。
    #[test]
    fn duplicate_hwnd_updates_in_place() {
        let c = ForegroundCache::default();
        c.record(7, "old.exe".to_string());
        c.record(7, "new.exe".to_string());
        assert_eq!(c.exe_for(7).as_deref(), Some("new.exe"));
        assert_eq!(c.len(), 1, "不得重复占用容量");
    }

    /// 容量上限：超出后按 FIFO 淘汰最旧项，缓存不无限增长。
    #[test]
    fn fifo_eviction_respects_capacity() {
        let c = ForegroundCache::with_capacity(3);
        c.record(1, "a.exe".to_string());
        c.record(2, "b.exe".to_string());
        c.record(3, "c.exe".to_string());
        c.record(4, "d.exe".to_string());

        assert!(c.exe_for(1).is_none(), "最旧的应被淘汰");
        assert_eq!(c.exe_for(2).as_deref(), Some("b.exe"));
        assert_eq!(c.exe_for(4).as_deref(), Some("d.exe"));
        assert_eq!(c.len(), 3);
    }

    /// 更新已存在的 HWND 不应改变淘汰顺序之外的行为（不重复插入队列）。
    #[test]
    fn update_existing_does_not_grow_queue() {
        let c = ForegroundCache::with_capacity(2);
        c.record(1, "a.exe".to_string());
        c.record(2, "b.exe".to_string());
        for _ in 0..10 {
            c.record(1, "a.exe".to_string());
        }
        c.record(3, "c.exe".to_string());
        assert_eq!(c.len(), 2);
        assert_eq!(
            c.exe_for(2).as_deref(),
            Some("b.exe"),
            "2 应仍在（1 被更新不应挤掉 2）"
        );
        assert!(c.exe_for(3).is_some());
    }

    /// hwnd=0（无前台窗口/锁屏）不记录。
    #[test]
    fn zero_hwnd_is_ignored() {
        let c = ForegroundCache::default();
        c.record(0, "x.exe".to_string());
        assert_eq!(c.len(), 0);
        assert!(c.latest().is_none());
    }
}
