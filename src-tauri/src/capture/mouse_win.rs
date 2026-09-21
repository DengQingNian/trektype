//! Windows 鼠标低级钩子（WH_MOUSE_LL）。
//!
//! 只采集 5 种按钮的**按下**（左/右/中/侧键 X1/X2）；移动与滚轮在钩子层直接忽略。
//! 与键盘钩子同构：回调极简 + 消息泵 + 只观测不拦截。

use crate::capture::event::{epoch_ms_fast, EventKind, MouseButton, RawEvent};
use crate::capture::CaptureShared;
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, MSG, MSLLHOOKSTRUCT, WH_MOUSE_LL,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_QUIT, WM_RBUTTONDOWN, WM_XBUTTONDOWN,
};

/// MSLLHOOKSTRUCT.flags 的注入标志（LLMHF_INJECTED）。
const LLMHF_INJECTED: u32 = 0x0000_0001;

/// 进程内唯一共享状态（回调无捕获，只能通过全局访问）。
static SHARED: OnceLock<Arc<CaptureShared>> = OnceLock::new();

/// 鼠标钩子输入的纯数据视图（与 Windows 类型解耦，便于单测）。
#[derive(Debug, Clone, Copy)]
pub struct MouseHookInput {
    /// 窗口消息（WM_LBUTTONDOWN 等）
    pub msg: u32,
    /// MSLLHOOKSTRUCT.mouseData（X 按钮标识在高 16 位）
    pub mouse_data: u32,
    pub x: i32,
    pub y: i32,
    pub is_injected: bool,
}

/// 纯逻辑：把鼠标钩子输入分类为按钮；返回 `None` 表示不采集（移动/滚轮/未知消息）。
pub fn classify_mouse_event(input: &MouseHookInput) -> Option<MouseButton> {
    match input.msg {
        WM_LBUTTONDOWN => Some(MouseButton::Left),
        WM_RBUTTONDOWN => Some(MouseButton::Right),
        WM_MBUTTONDOWN => Some(MouseButton::Middle),
        WM_XBUTTONDOWN => {
            // XBUTTON1=1（后退）/ XBUTTON2=2（前进）位于 mouseData 高 16 位
            match (input.mouse_data >> 16) as u16 {
                1 => Some(MouseButton::X1),
                2 => Some(MouseButton::X2),
                _ => None,
            }
        }
        // WM_MOUSEMOVE / WM_MOUSEWHEEL 等：明确不采集（拷问决策 Q4）
        _ => None,
    }
}

/// 钩子过程：只观测不拦截，任何分支都必须最终调用 `CallNextHookEx`。
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        if let Some(shared) = SHARED.get() {
            let ms = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            let input = MouseHookInput {
                msg: wparam.0 as u32,
                mouse_data: ms.mouseData,
                x: ms.pt.x,
                y: ms.pt.y,
                is_injected: (ms.flags & LLMHF_INJECTED) != 0,
            };

            shared.touch_heartbeat();

            let skip = shared.should_drop(EventKind::Click)
                || (input.is_injected && shared.ignore_injected.load(Ordering::Relaxed));

            if !skip {
                if let Some(button) = classify_mouse_event(&input) {
                    let ev = RawEvent {
                        ts_ms: epoch_ms_fast(),
                        kind: EventKind::Click,
                        key_code: None,
                        button: Some(button),
                        // 虚拟桌面物理坐标（进程 DPI 感知为 Per-Monitor-V2）
                        x: Some(input.x),
                        y: Some(input.y),
                        hwnd_foreground: unsafe { GetForegroundWindow() }.0 as isize,
                        is_repeat: false,
                        is_injected: input.is_injected,
                    };
                    shared.try_emit(ev);
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// 启动鼠标钩子线程（含消息泵）。进程内只应调用一次。
pub fn spawn(shared: Arc<CaptureShared>) -> std::io::Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("mouse-hook".to_string())
        .spawn(move || run_hook(shared))
}

fn run_hook(shared: Arc<CaptureShared>) {
    let _ = SHARED.set(shared.clone());
    let shared = SHARED.get().cloned().unwrap_or(shared);

    unsafe {
        let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), None, 0) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("[typetrek] 鼠标钩子安装失败：{e}（采集不可用）");
                shared.mouse_installed.store(false, Ordering::SeqCst);
                return;
            }
        };
        shared.mouse_installed.store(true, Ordering::SeqCst);
        shared
            .mouse_thread_id
            .store(GetCurrentThreadId(), Ordering::SeqCst);
        shared.touch_heartbeat();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = UnhookWindowsHookEx(hook);
        shared.mouse_installed.store(false, Ordering::SeqCst);
        shared.mouse_thread_id.store(0, Ordering::SeqCst);
    }
}

/// 请求钩子线程退出（投递 WM_QUIT）。
pub fn request_stop(thread_id: u32) {
    if thread_id != 0 {
        unsafe {
            let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(msg: u32, mouse_data: u32) -> MouseHookInput {
        MouseHookInput {
            msg,
            mouse_data,
            x: 100,
            y: 200,
            is_injected: false,
        }
    }

    /// 三种标准按钮的按下事件正确分类。
    #[test]
    fn three_standard_buttons() {
        assert_eq!(
            classify_mouse_event(&input(WM_LBUTTONDOWN, 0)),
            Some(MouseButton::Left)
        );
        assert_eq!(
            classify_mouse_event(&input(WM_RBUTTONDOWN, 0)),
            Some(MouseButton::Right)
        );
        assert_eq!(
            classify_mouse_event(&input(WM_MBUTTONDOWN, 0)),
            Some(MouseButton::Middle)
        );
    }

    /// 侧键：X 按钮标识位于 mouseData 高 16 位（1=后退 X1，2=前进 X2），其余值丢弃。
    #[test]
    fn side_buttons_from_high_word() {
        assert_eq!(
            classify_mouse_event(&input(WM_XBUTTONDOWN, 1 << 16)),
            Some(MouseButton::X1)
        );
        assert_eq!(
            classify_mouse_event(&input(WM_XBUTTONDOWN, 2 << 16)),
            Some(MouseButton::X2)
        );
        assert_eq!(
            classify_mouse_event(&input(WM_XBUTTONDOWN, 0)),
            None,
            "无 X 标识应丢弃"
        );
        assert_eq!(
            classify_mouse_event(&input(WM_XBUTTONDOWN, 3 << 16)),
            None,
            "未知 X 标识应丢弃"
        );
        // 低 16 位是高精度滚轮等其他信息，不得误判
        assert_eq!(
            classify_mouse_event(&input(WM_XBUTTONDOWN, (1 << 16) | 0x00FF)),
            Some(MouseButton::X1),
            "高位 X 标识应在低位信息存在时仍被正确解析"
        );
    }

    /// 移动与滚轮消息（含按钮抬起）一律不采集。
    #[test]
    fn move_wheel_and_button_up_are_ignored() {
        const WM_MOUSEMOVE: u32 = 0x0200;
        const WM_MOUSEWHEEL: u32 = 0x020A;
        const WM_LBUTTONUP: u32 = 0x0202;
        const WM_RBUTTONUP: u32 = 0x0205;
        const WM_MBUTTONUP: u32 = 0x0208;
        const WM_XBUTTONUP: u32 = 0x020C;

        for msg in [
            WM_MOUSEMOVE,
            WM_MOUSEWHEEL,
            WM_LBUTTONUP,
            WM_RBUTTONUP,
            WM_MBUTTONUP,
            WM_XBUTTONUP,
        ] {
            assert_eq!(
                classify_mouse_event(&input(msg, 1 << 16)),
                None,
                "消息 {msg:#X} 不应产生事件"
            );
        }
    }
}
