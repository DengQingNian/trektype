//! Windows 键盘低级钩子（WH_KEYBOARD_LL）。
//!
//! 回调内只做：心跳 → 暂停/开关检查 → 注入过滤 → 相位与 repeat 判定 → 组装事件 → 非阻塞入队。
//! **禁止任何 IO/锁竞争**：Win32 对低级钩子回调有超时机制（`LowLevelHooksTimeout`），
//! 超时会被系统静默摘除钩子；回调耗时异常时通过消息泵重装钩子（见 `WM_APP_REINSTALL`）。
//! 本钩子**只观测不拦截**：必须始终 `CallNextHookEx` 放行，绝不吞按键。

use crate::capture::event::{epoch_ms_fast, EventKind, RawEvent};
use crate::capture::keymap::vk_to_code;
use crate::capture::CaptureShared;
use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, KBDLLHOOKSTRUCT, LLKHF_EXTENDED,
    LLKHF_INJECTED, LLKHF_UP, MSG, WH_KEYBOARD_LL, WM_QUIT,
};

/// 自定义消息：请求钩子线程重装钩子（WM_APP + 1）。
const WM_APP_REINSTALL: u32 = 0x8001;

/// 回调单次耗时告警阈值：超过即认为存在被系统摘钩风险。
const CALLBACK_SLOW_MS: u128 = 200;

/// 进程内唯一共享状态（钩子回调是无捕获的 `extern "system"` 函数，只能通过全局访问）。
static SHARED: OnceLock<Arc<CaptureShared>> = OnceLock::new();

thread_local! {
    /// 按键状态跟踪：回调总在安装钩子的同一线程执行，thread_local 无锁且安全。
    static TRACKER: RefCell<KeyStateTracker> = RefCell::new(KeyStateTracker::default());
}

/// 按键状态跟踪：区分"首次按下"与"OS 自动重复"。
///
/// Windows 不提供 repeat 标志——自动重复表现为*同一 VK 在没有 KeyUp 的情况下的连续 KeyDown*。
/// 因此维护按下集合：KeyDown 时集合中已存在即为 repeat；KeyUp 时移除。
#[derive(Default, Debug)]
pub struct KeyStateTracker {
    pressed: HashSet<u32>,
}

impl KeyStateTracker {
    /// 处理 KeyDown，返回是否为自动重复。
    pub fn on_key_down(&mut self, vk: u32) -> bool {
        !self.pressed.insert(vk)
    }

    /// 处理 KeyUp。
    pub fn on_key_up(&mut self, vk: u32) {
        self.pressed.remove(&vk);
    }

    /// 当前处于按下状态的键数（诊断/测试用）。
    pub fn pressed_count(&self) -> usize {
        self.pressed.len()
    }
}

/// 键盘钩子输入的纯数据视图（与 Windows 类型解耦，便于单测）。
#[derive(Debug, Clone, Copy)]
pub struct KeyHookInput {
    pub vk: u32,
    pub scan: u32,
    pub ext: bool,
    pub is_up: bool,
    pub is_injected: bool,
}

/// 事件分类结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyClassification {
    pub kind: EventKind,
    pub key_code: &'static str,
    pub is_repeat: bool,
}

/// 纯逻辑：把钩子输入分类为可入库事件；返回 `None` 表示键码无法识别（应丢弃）。
/// 该函数**始终**推进状态跟踪（含暂停/注入期间），保证恢复采集后 repeat 判定不误判。
pub fn classify_key_event(
    input: KeyHookInput,
    tracker: &mut KeyStateTracker,
) -> Option<KeyClassification> {
    let code = vk_to_code(input.vk, input.scan, input.ext);

    if input.is_up {
        // 无论能否识别都清理按下状态，避免残留污染后续 repeat 判定
        tracker.on_key_up(input.vk);
        return code.map(|c| KeyClassification {
            kind: EventKind::KeyUp,
            key_code: c,
            is_repeat: false,
        });
    }

    // 未知键不登记按下状态（其 KeyDown 不产出事件，也无需求 repeat 语义）
    let c = code?;
    let is_repeat = tracker.on_key_down(input.vk);
    Some(KeyClassification {
        kind: EventKind::KeyDown,
        key_code: c,
        is_repeat,
    })
}

/// 钩子过程：只观测不拦截，任何分支都必须最终调用 `CallNextHookEx`。
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        if let Some(shared) = SHARED.get() {
            let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            let flags = kb.flags.0;
            let input = KeyHookInput {
                vk: kb.vkCode,
                scan: kb.scanCode,
                ext: (flags & LLKHF_EXTENDED.0) != 0,
                is_up: (flags & LLKHF_UP.0) != 0,
                is_injected: (flags & LLKHF_INJECTED.0) != 0,
            };

            shared.touch_heartbeat();

            // 状态跟踪始终推进（暂停/过滤期间也维护物理按键状态），仅入库受开关控制
            let probe_kind = if input.is_up {
                EventKind::KeyUp
            } else {
                EventKind::KeyDown
            };
            let skip = shared.should_drop(probe_kind)
                || (input.is_injected && shared.ignore_injected.load(Ordering::Relaxed));

            let classification = TRACKER.with(|t| classify_key_event(input, &mut t.borrow_mut()));

            if !skip {
                if let Some(c) = classification {
                    let ev = RawEvent {
                        ts_ms: epoch_ms_fast(),
                        kind: c.kind,
                        key_code: Some(c.key_code),
                        button: None,
                        x: None,
                        y: None,
                        hwnd_foreground: unsafe { GetForegroundWindow() }.0 as isize,
                        is_repeat: c.is_repeat,
                        is_injected: input.is_injected,
                    };
                    shared.try_emit(ev);
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// 启动键盘钩子线程（含消息泵与自动重装）。进程内只应调用一次。
pub fn spawn(shared: Arc<CaptureShared>) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("keyboard-hook".to_string())
        .spawn(move || run_hook(shared))
}

fn run_hook(shared: Arc<CaptureShared>) {
    let _ = SHARED.set(shared.clone());
    // 若已有共享对象（重复启动），沿用已注册的那个，保证回调访问到的状态与调用方一致
    let shared = SHARED.get().cloned().unwrap_or(shared);

    unsafe {
        let mut hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[typetrek] 键盘钩子安装失败：{e}（采集不可用）");
                shared.keyboard_installed.store(false, Ordering::SeqCst);
                return;
            }
        };
        shared.keyboard_installed.store(true, Ordering::SeqCst);
        shared
            .keyboard_thread_id
            .store(GetCurrentThreadId(), Ordering::SeqCst);
        shared.touch_heartbeat();

        let mut msg = MSG::default();
        // GetMessageW 返回 0（WM_QUIT）时退出消息泵
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_APP_REINSTALL {
                let _ = UnhookWindowsHookEx(hook);
                match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), None, 0) {
                    Ok(h) => {
                        hook = h;
                        shared.reinstall_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        eprintln!("[typetrek] 键盘钩子重装失败：{e}");
                        shared.keyboard_installed.store(false, Ordering::SeqCst);
                    }
                }
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(hook);
        shared.keyboard_installed.store(false, Ordering::SeqCst);
        shared.keyboard_thread_id.store(0, Ordering::SeqCst);
    }
}

/// 请求重装钩子（回调耗时异常时由看门狗或外部触发）。
pub fn request_reinstall() {
    if let Some(shared) = SHARED.get() {
        let tid = shared.keyboard_thread_id.load(Ordering::SeqCst);
        if tid != 0 {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_APP_REINSTALL, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// 请求钩子线程退出（投递 WM_QUIT；线程退出前会卸载钩子并清理安装标志）。
pub fn request_stop(thread_id: u32) {
    if thread_id != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

/// 回调耗时阈值（供看门狗使用）。
pub fn callback_slow_threshold_ms() -> u128 {
    CALLBACK_SLOW_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(vk: u32, is_up: bool) -> KeyHookInput {
        KeyHookInput {
            vk,
            scan: 0x1E,
            ext: false,
            is_up,
            is_injected: false,
        }
    }

    /// 首次按下不是 repeat；无 KeyUp 的连续按下是 repeat；KeyUp 后恢复为非 repeat。
    #[test]
    fn repeat_detection_follows_press_state() {
        let mut t = KeyStateTracker::default();

        let c1 = classify_key_event(input(0x41, false), &mut t).unwrap();
        assert_eq!(c1.kind, EventKind::KeyDown);
        assert!(!c1.is_repeat, "首次按下不应标记 repeat");

        let c2 = classify_key_event(input(0x41, false), &mut t).unwrap();
        assert!(c2.is_repeat, "无 KeyUp 的连续 KeyDown 应标记 repeat");

        let c3 = classify_key_event(input(0x41, true), &mut t).unwrap();
        assert_eq!(c3.kind, EventKind::KeyUp);
        assert!(!c3.is_repeat, "KeyUp 永远不是 repeat");

        let c4 = classify_key_event(input(0x41, false), &mut t).unwrap();
        assert!(!c4.is_repeat, "松开后再按应恢复为非 repeat");
        assert_eq!(t.pressed_count(), 1);
    }

    /// 多键独立跟踪：A 按住不放不影响 B 的 repeat 判定。
    #[test]
    fn repeat_tracking_is_per_key() {
        let mut t = KeyStateTracker::default();
        classify_key_event(input(0x41, false), &mut t).unwrap();
        let b1 = classify_key_event(input(0x42, false), &mut t).unwrap();
        assert!(!b1.is_repeat, "不同键之间互不影响");
        assert_eq!(t.pressed_count(), 2);

        classify_key_event(input(0x41, true), &mut t).unwrap();
        assert_eq!(t.pressed_count(), 1);
        let b2 = classify_key_event(input(0x42, false), &mut t).unwrap();
        assert!(b2.is_repeat, "B 仍未松开，应仍为 repeat");
    }

    /// 相位映射：KBDLLHOOKSTRUCT 的 UP 标志决定 KeyDown/KeyUp。
    #[test]
    fn phase_maps_to_event_kind() {
        let mut t = KeyStateTracker::default();
        assert_eq!(
            classify_key_event(input(0x20, false), &mut t).unwrap().kind,
            EventKind::KeyDown
        );
        assert_eq!(
            classify_key_event(input(0x20, true), &mut t).unwrap().kind,
            EventKind::KeyUp
        );
    }

    /// 未知键码返回 None，但状态跟踪仍需清理（否则残留状态污染后续判定）。
    #[test]
    fn unknown_vk_returns_none_but_cleans_state() {
        let mut t = KeyStateTracker::default();
        let unknown = KeyHookInput {
            vk: 0xE7, // VK_PACKET
            scan: 0,
            ext: false,
            is_up: false,
            is_injected: true,
        };
        assert!(classify_key_event(unknown, &mut t).is_none());
        assert_eq!(t.pressed_count(), 0, "未知键不应残留按下状态");

        // 已知键的 up 事件即使未配对 down 也不得 panic
        let mut t2 = KeyStateTracker::default();
        assert!(classify_key_event(input(0x41, true), &mut t2).is_some());
    }

    /// 注入事件：分类函数本身不做过滤（过滤在回调层按配置执行），
    /// 因此注入键也能正确产出事件，供 ignore_injected=false 的用户统计自动化输入。
    #[test]
    fn injected_events_are_classified_but_flagged() {
        let mut t = KeyStateTracker::default();
        let injected = KeyHookInput {
            vk: 0x41,
            scan: 0x1E,
            ext: false,
            is_up: false,
            is_injected: true,
        };
        let c = classify_key_event(injected, &mut t).unwrap();
        assert_eq!(c.key_code, "KeyA");
        assert!(injected.is_injected, "注入标记应透传给 RawEvent");
    }
}
