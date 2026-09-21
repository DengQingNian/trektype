//! 采集层：全局钩子（键盘/鼠标）、键码映射、前台窗口缓存与显示器快照。
//!
//! 线程模型：键盘与鼠标各一个带消息泵的钩子线程；回调只做原子读 + 组装事件 + 非阻塞入队，
//! 任何 IO/锁竞争都可能触发 Win32 低级钩子回调超时（超时会被系统静默摘除钩子）。

pub mod blacklist;
pub mod event;
pub mod foreground;
pub mod keymap;
pub mod monitors;

#[cfg(windows)]
pub mod keyboard_win;
#[cfg(windows)]
pub mod mouse_win;

use crate::capture::blacklist::Blacklist;
use crate::capture::event::{epoch_ms_fast, EventKind, RawEvent};
use crate::capture::foreground::ForegroundCache;
use crate::capture::monitors::MonitorCache;
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering};
use std::sync::RwLock;

/// 事件队列容量：满则丢弃并计数（保命优先于完整，见计划事件流第 1 步）。
pub const QUEUE_CAPACITY: usize = 8192;

/// 采集线程与管线之间共享的运行时状态。
/// 全部为原子量：钩子回调热路径无锁读取，配置变更由 UI 侧写入。
pub struct CaptureShared {
    /// 暂停开关：true 时回调直接丢弃事件（暂停语义在入队之前生效）
    pub paused: AtomicBool,
    /// 是否忽略软件注入事件（AutoHotkey 等自动化用户可关闭）
    pub ignore_injected: AtomicBool,
    /// 键盘采集开关
    pub capture_keyboard: AtomicBool,
    /// 鼠标采集开关
    pub capture_mouse: AtomicBool,
    /// 事件发送通道（有界；try_send 失败即丢弃并计入 dropped）
    pub event_tx: Sender<RawEvent>,
    /// 累计丢弃事件数（队列满）
    pub dropped: AtomicU64,
    /// 钩子回调心跳（unix 毫秒，每次回调刷新；UI 用于展示"最近事件时间"）
    pub heartbeat_ms: AtomicI64,
    /// 键盘钩子是否安装成功（失败时 UI 应显示采集不可用）
    pub keyboard_installed: AtomicBool,
    /// 鼠标钩子是否安装成功
    pub mouse_installed: AtomicBool,
    /// 键盘钩子线程 id（用于停止：PostThreadMessageW WM_QUIT）
    pub keyboard_thread_id: AtomicU32,
    /// 鼠标钩子线程 id
    pub mouse_thread_id: AtomicU32,
    /// 因回调耗时过长触发的钩子重装次数（诊断用）
    pub reinstall_count: AtomicU64,
    /// 停止信号：所有采集侧线程（轮询/监视）观察到后退出
    pub stop_requested: AtomicBool,
    /// 前台窗口 → exe 名缓存（前台监视线程写，写入器读）
    pub foreground: ForegroundCache,
    /// 显示器快照缓存（显示器监视线程写，写入器读；dirty 表示待落库）
    pub monitors: MonitorCache,
    /// 敏感应用黑名单（设置页热更新，写入器在每个事件入库前判定）
    pub blacklist: RwLock<Blacklist>,
    /// 隐私模式：跳过 raw 明细落库，仅写聚合（可热切换）
    pub privacy_mode: AtomicBool,
    /// 被黑名单丢弃的事件数（合规审计/测试用）
    pub blocked_total: AtomicU64,
    /// 累计成功落库的事件数
    pub flushed_events: AtomicU64,
    /// 写入器 flush 次数与最近一次耗时（毫秒，运行状态展示）
    pub flush_count: AtomicU64,
    pub last_flush_ms: AtomicI64,
    /// 批量写入失败次数（本批丢弃）
    pub write_errors: AtomicU64,
    /// 写入器是否已就绪（数据库打开+迁移成功）
    pub writer_ready: AtomicBool,
    /// 采集侧线程是否已启动（幂等保护：同意只生效一次）
    pub capture_started: AtomicBool,
}

impl CaptureShared {
    pub fn new(event_tx: Sender<RawEvent>) -> Self {
        Self {
            paused: AtomicBool::new(false),
            ignore_injected: AtomicBool::new(true),
            capture_keyboard: AtomicBool::new(true),
            capture_mouse: AtomicBool::new(true),
            event_tx,
            dropped: AtomicU64::new(0),
            heartbeat_ms: AtomicI64::new(0),
            keyboard_installed: AtomicBool::new(false),
            mouse_installed: AtomicBool::new(false),
            keyboard_thread_id: AtomicU32::new(0),
            mouse_thread_id: AtomicU32::new(0),
            reinstall_count: AtomicU64::new(0),
            stop_requested: AtomicBool::new(false),
            foreground: ForegroundCache::default(),
            monitors: MonitorCache::new(),
            blacklist: RwLock::new(Blacklist::default()),
            privacy_mode: AtomicBool::new(false),
            blocked_total: AtomicU64::new(0),
            flushed_events: AtomicU64::new(0),
            flush_count: AtomicU64::new(0),
            last_flush_ms: AtomicI64::new(0),
            write_errors: AtomicU64::new(0),
            writer_ready: AtomicBool::new(false),
            capture_started: AtomicBool::new(false),
        }
    }

    /// 请求停止所有采集侧线程（幂等）。
    pub fn request_stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
    }

    /// 替换黑名单（设置页保存时调用）。
    pub fn set_blacklist(&self, list: Blacklist) {
        *self.blacklist.write().unwrap_or_else(|e| e.into_inner()) = list;
    }

    /// 读取黑名单的克隆（写入器逐事件判定用；克隆开销仅在 flush 时一次）。
    pub fn blacklist_snapshot(&self) -> Blacklist {
        self.blacklist
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// 回调热路径：该事件是否应立即丢弃（暂停 / 对应类型开关关闭）。
    #[inline]
    pub fn should_drop(&self, kind: EventKind) -> bool {
        if self.paused.load(Ordering::Relaxed) {
            return true;
        }
        match kind {
            EventKind::KeyDown | EventKind::KeyUp => !self.capture_keyboard.load(Ordering::Relaxed),
            EventKind::Click => !self.capture_mouse.load(Ordering::Relaxed),
        }
    }

    /// 刷新心跳（回调热路径，Relaxed 足够——仅用于展示与诊断）。
    #[inline]
    pub fn touch_heartbeat(&self) {
        self.heartbeat_ms.store(epoch_ms_fast(), Ordering::Relaxed);
    }

    /// 非阻塞投递；失败（队列满）时计数并返回 false。
    #[inline]
    pub fn try_emit(&self, ev: RawEvent) -> bool {
        match self.event_tx.try_send(ev) {
            Ok(()) => true,
            Err(_) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
}

/// 启动全部采集侧线程（键盘/鼠标钩子 + 显示器/前台监视）。
///
/// **仅在用户完成知情同意后调用**；幂等——重复调用只生效一次。
/// 返回是否本次真正启动（用于日志/测试）。
pub fn start_capture(shared: &std::sync::Arc<CaptureShared>) -> bool {
    if shared.capture_started.swap(true, Ordering::SeqCst) {
        return false;
    }
    #[cfg(windows)]
    {
        if let Err(e) = keyboard_win::spawn(shared.clone()) {
            eprintln!("[typetrek] 键盘钩子线程启动失败：{e}");
        }
        if let Err(e) = mouse_win::spawn(shared.clone()) {
            eprintln!("[typetrek] 鼠标钩子线程启动失败：{e}");
        }
        if let Err(e) = monitors::spawn_monitor_watcher(shared.clone()) {
            eprintln!("[typetrek] 显示器监视线程启动失败：{e}");
        }
        if let Err(e) = foreground::spawn_foreground_watcher(shared.clone()) {
            eprintln!("[typetrek] 前台应用监视线程启动失败：{e}");
        }
    }
    true
}

/// 请求停止采集侧线程（退出流程调用；钩子线程需额外投递 WM_QUIT）。
pub fn stop_capture(shared: &CaptureShared) {
    shared.request_stop();
    #[cfg(windows)]
    {
        keyboard_win::request_stop(shared.keyboard_thread_id.load(Ordering::SeqCst));
        mouse_win::request_stop(shared.mouse_thread_id.load(Ordering::SeqCst));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::event::MouseButton;
    use crossbeam_channel::bounded;

    fn make_event(kind: EventKind) -> RawEvent {
        RawEvent {
            ts_ms: 0,
            kind,
            key_code: Some("KeyA"),
            button: Some(MouseButton::Left),
            x: None,
            y: None,
            hwnd_foreground: 0,
            is_repeat: false,
            is_injected: false,
        }
    }

    /// 暂停后所有类型事件都应被丢弃；恢复后不再丢弃。
    #[test]
    fn paused_drops_all_events() {
        let (tx, _rx) = bounded(QUEUE_CAPACITY);
        let shared = CaptureShared::new(tx);
        assert!(!shared.should_drop(EventKind::KeyDown));
        shared.paused.store(true, Ordering::Relaxed);
        assert!(shared.should_drop(EventKind::KeyDown));
        assert!(shared.should_drop(EventKind::Click));
        shared.paused.store(false, Ordering::Relaxed);
        assert!(!shared.should_drop(EventKind::Click));
    }

    /// 分类型开关：关闭鼠标不影响键盘，反之亦然。
    #[test]
    fn per_type_switches_are_independent() {
        let (tx, _rx) = bounded(QUEUE_CAPACITY);
        let shared = CaptureShared::new(tx);
        shared.capture_mouse.store(false, Ordering::Relaxed);
        assert!(shared.should_drop(EventKind::Click));
        assert!(!shared.should_drop(EventKind::KeyDown));
        assert!(!shared.should_drop(EventKind::KeyUp));

        shared.capture_mouse.store(true, Ordering::Relaxed);
        shared.capture_keyboard.store(false, Ordering::Relaxed);
        assert!(!shared.should_drop(EventKind::Click));
        assert!(shared.should_drop(EventKind::KeyDown));
        assert!(shared.should_drop(EventKind::KeyUp));
    }

    /// 队列满时事件被丢弃并累计计数（不阻塞回调）。
    #[test]
    fn full_queue_increments_dropped() {
        let (tx, rx) = bounded(2);
        let shared = CaptureShared::new(tx);
        assert!(shared.try_emit(make_event(EventKind::KeyDown)));
        assert!(shared.try_emit(make_event(EventKind::KeyDown)));
        assert!(
            !shared.try_emit(make_event(EventKind::KeyDown)),
            "第三次应失败"
        );
        assert_eq!(shared.dropped.load(Ordering::Relaxed), 1);
        assert_eq!(rx.len(), 2);
    }

    /// 心跳刷新后可读到非零时间戳。
    #[test]
    fn heartbeat_updates() {
        let (tx, _rx) = bounded(1);
        let shared = CaptureShared::new(tx);
        assert_eq!(shared.heartbeat_ms.load(Ordering::Relaxed), 0);
        shared.touch_heartbeat();
        assert!(shared.heartbeat_ms.load(Ordering::Relaxed) > 0);
    }
}
