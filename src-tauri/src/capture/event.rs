//! 采集事件模型与热路径工具。

/// 事件类型（鼠标只记按下）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    KeyDown,
    KeyUp,
    Click,
}

/// 鼠标按钮：只采集 5 种按下（拷问决策 Q4，无滚轮/移动）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

impl MouseButton {
    /// 入库用的小写标识（与 agg_mouse_daily.button 值域一致）。
    pub fn as_str(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
            MouseButton::X1 => "x1",
            MouseButton::X2 => "x2",
        }
    }
}

/// 钩子回调产出的原始事件。
///
/// 不含 session_id——会话在进程启动时创建一次，由写入器 flush 时统一填充。
/// `hwnd_foreground` 是回调时刻的前台窗口句柄快照，由管线的前台缓存解析为 app_id。
#[derive(Debug, Clone, Copy)]
pub struct RawEvent {
    pub ts_ms: i64,
    pub kind: EventKind,
    /// 键盘事件的标准化键码（"KeyA" 等）；鼠标事件为 None
    pub key_code: Option<&'static str>,
    /// 鼠标事件按钮；键盘事件为 None
    pub button: Option<MouseButton>,
    /// 虚拟桌面物理坐标（仅 Click）
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub hwnd_foreground: isize,
    pub is_repeat: bool,
    pub is_injected: bool,
}

/// 热路径取时间戳：直接读系统时钟的 unix 毫秒。
/// 不使用 chrono（其 `Local::now()` 含时区换算，比 SystemTime 慢一个量级）。
#[inline]
pub fn epoch_ms_fast() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 按钮标识必须与数据库 agg_mouse_daily.button 的值域完全一致。
    #[test]
    fn button_str_matches_db_value_domain() {
        assert_eq!(MouseButton::Left.as_str(), "left");
        assert_eq!(MouseButton::Right.as_str(), "right");
        assert_eq!(MouseButton::Middle.as_str(), "middle");
        assert_eq!(MouseButton::X1.as_str(), "x1");
        assert_eq!(MouseButton::X2.as_str(), "x2");
    }

    /// 热路径时间戳与系统时钟一致（允许秒级误差，验证量纲为毫秒）。
    #[test]
    fn epoch_ms_is_millis() {
        let t = epoch_ms_fast();
        // 2020-01-01 之后且 2100 之前，量纲应为毫秒
        assert!(t > 1_577_836_800_000, "应为毫秒级时间戳");
        assert!(t < 4_102_444_800_000);
    }
}
