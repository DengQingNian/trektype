//! Windows 虚拟键码（VK）→ 标准键位标识映射。
//!
//! 命名遵循 Web `KeyboardEvent.code` 风格（`KeyA` / `Digit1` / `Space` / `ShiftLeft`）：
//! 跨平台稳定、与前端 104 键布局 JSON 直接对应、不含字母明文歧义。
//! 本模块是**纯函数**，不依赖 Windows API，便于单测覆盖全表。

/// 字母键 VK 0x41-0x5A → KeyA..KeyZ
const LETTERS: [&str; 26] = [
    "KeyA", "KeyB", "KeyC", "KeyD", "KeyE", "KeyF", "KeyG", "KeyH", "KeyI", "KeyJ", "KeyK", "KeyL",
    "KeyM", "KeyN", "KeyO", "KeyP", "KeyQ", "KeyR", "KeyS", "KeyT", "KeyU", "KeyV", "KeyW", "KeyX",
    "KeyY", "KeyZ",
];

/// 主键盘数字 VK 0x30-0x39 → Digit0..Digit9
const DIGITS: [&str; 10] = [
    "Digit0", "Digit1", "Digit2", "Digit3", "Digit4", "Digit5", "Digit6", "Digit7", "Digit8",
    "Digit9",
];

/// 功能键 VK 0x70-0x87 → F1..F24
const FKEYS: [&str; 24] = [
    "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14", "F15",
    "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
];

/// 需要按 scan code / 扩展位区分左右的**泛化**修饰键 VK：
/// 低级钩子在多数情况下上报泛化码而非左右具体码，必须结合 scanCode 或 KBDLLHOOKSTRUCT.flags。
pub const GENERIC_VK_SHIFT: u32 = 0x10;
pub const GENERIC_VK_CONTROL: u32 = 0x11;
pub const GENERIC_VK_MENU: u32 = 0x12;
pub const GENERIC_VK_RETURN: u32 = 0x0D;

/// 左 Shift 的 scan code（右 Shift 为 0x36）。
const SCAN_SHIFT_LEFT: u32 = 0x2A;
const SCAN_SHIFT_RIGHT: u32 = 0x36;

/// VK + scan code + 扩展位 → 标准键位标识。
///
/// - `vk`：虚拟键码（`KBDLLHOOKSTRUCT.vkCode`）
/// - `scan`：扫描码（`KBDLLHOOKSTRUCT.scanCode`），用于区分左右 Shift
/// - `ext`：扩展键标志（`LLKHF_EXTENDED`），用于区分右 Ctrl/Alt 与小键盘 Enter
///
/// 无法识别的键（VK_PACKET 注入、保留码等）返回 `None`，由调用方丢弃。
pub fn vk_to_code(vk: u32, scan: u32, ext: bool) -> Option<&'static str> {
    // 泛化修饰键：先于主表处理，左右由 scan/ext 判定
    match vk {
        GENERIC_VK_SHIFT => {
            return Some(if scan == SCAN_SHIFT_RIGHT {
                "ShiftRight"
            } else if scan == SCAN_SHIFT_LEFT {
                "ShiftLeft"
            } else {
                // scan 缺失（少数驱动）时按不扩展处理，归左，避免丢键
                "ShiftLeft"
            });
        }
        GENERIC_VK_CONTROL => return Some(if ext { "ControlRight" } else { "ControlLeft" }),
        GENERIC_VK_MENU => return Some(if ext { "AltRight" } else { "AltLeft" }),
        // 主键盘 Enter 与小键盘 Enter 同 VK，靠扩展位区分
        GENERIC_VK_RETURN => return Some(if ext { "NumpadEnter" } else { "Enter" }),
        _ => {}
    }

    let code = match vk {
        // 字母 / 数字 / 功能键
        0x41..=0x5A => LETTERS[(vk - 0x41) as usize],
        0x30..=0x39 => DIGITS[(vk - 0x30) as usize],
        0x70..=0x87 => FKEYS[(vk - 0x70) as usize],

        // 左右具体修饰键（部分驱动直接上报）
        0xA0 => "ShiftLeft",
        0xA1 => "ShiftRight",
        0xA2 => "ControlLeft",
        0xA3 => "ControlRight",
        0xA4 => "AltLeft",
        0xA5 => "AltRight",
        0x5B => "MetaLeft",
        0x5C => "MetaRight",

        // 编辑与控制键
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0C => "Clear",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x1B => "Escape",
        0x20 => "Space",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "ArrowLeft",
        0x26 => "ArrowUp",
        0x27 => "ArrowRight",
        0x28 => "ArrowDown",
        0x29 => "Select",
        0x2A => "Print",
        0x2B => "Execute",
        0x2C => "PrintScreen",
        0x2D => "Insert",
        0x2E => "Delete",
        0x2F => "Help",
        0x5D => "ContextMenu",
        0x5F => "Sleep",
        0x90 => "NumLock",
        0x91 => "ScrollLock",

        // 小键盘
        0x60..=0x69 => NUMPAD_DIGITS[(vk - 0x60) as usize],
        0x6A => "NumpadMultiply",
        0x6B => "NumpadAdd",
        0x6C => "NumpadComma",
        0x6D => "NumpadSubtract",
        0x6E => "NumpadDecimal",
        0x6F => "NumpadDivide",

        // OEM 键（US 布局）
        0xBA => "Semicolon",
        0xBB => "Equal",
        0xBC => "Comma",
        0xBD => "Minus",
        0xBE => "Period",
        0xBF => "Slash",
        0xC0 => "Backquote",
        0xDB => "BracketLeft",
        0xDC => "Backslash",
        0xDD => "BracketRight",
        0xDE => "Quote",
        0xE2 => "IntlBackslash",

        // IME / 输入法（中文输入法组合期间仍会产生字母键码，此处仅归类 IME 专用键）
        0x15 => "KanaMode",
        0x16 => "IMEOn",
        0x17 => "IMEJunja",
        0x18 => "IMEFinal",
        0x19 => "KanjiMode",
        0x1A => "IMEOff",
        0x1C => "Convert",
        0x1D => "NonConvert",
        0x1E => "IMEAccept",
        0x1F => "IMEModeChange",
        0xE5 => "IMEProcess",

        // 浏览器与多媒体键
        0xA6 => "BrowserBack",
        0xA7 => "BrowserForward",
        0xA8 => "BrowserRefresh",
        0xA9 => "BrowserStop",
        0xAA => "BrowserSearch",
        0xAB => "BrowserFavorites",
        0xAC => "BrowserHome",
        0xAD => "AudioVolumeMute",
        0xAE => "AudioVolumeDown",
        0xAF => "AudioVolumeUp",
        0xB0 => "MediaTrackNext",
        0xB1 => "MediaTrackPrevious",
        0xB2 => "MediaStop",
        0xB3 => "MediaPlayPause",
        0xB4 => "LaunchMail",
        0xB5 => "LaunchMediaPlayer",
        0xB6 => "LaunchApplication1",
        0xB7 => "LaunchApplication2",

        // 未识别（VK_PACKET 注入、鼠标 VK、保留码等）→ 丢弃
        _ => return None,
    };
    Some(code)
}

/// 小键盘数字 VK 0x60-0x69 → Numpad0..Numpad9
const NUMPAD_DIGITS: [&str; 10] = [
    "Numpad0", "Numpad1", "Numpad2", "Numpad3", "Numpad4", "Numpad5", "Numpad6", "Numpad7",
    "Numpad8", "Numpad9",
];

/// 该 VK 是否为泛化修饰键（需要 scan/ext 才能定左右）。
pub fn is_generic_modifier(vk: u32) -> bool {
    matches!(
        vk,
        GENERIC_VK_SHIFT | GENERIC_VK_CONTROL | GENERIC_VK_MENU | GENERIC_VK_RETURN
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// 26 字母 + 10 数字 + 24 功能键必须全表可映射，且命名符合 KeyboardEvent.code 风格。
    #[test]
    fn letters_digits_and_fkeys_are_complete() {
        assert_eq!(vk_to_code(0x41, 0, false), Some("KeyA"));
        assert_eq!(vk_to_code(0x5A, 0, false), Some("KeyZ"));
        assert_eq!(vk_to_code(0x30, 0, false), Some("Digit0"));
        assert_eq!(vk_to_code(0x39, 0, false), Some("Digit9"));
        assert_eq!(vk_to_code(0x70, 0, false), Some("F1"));
        assert_eq!(vk_to_code(0x87, 0, false), Some("F24"));

        for vk in 0x41..=0x5Au32 {
            assert!(vk_to_code(vk, 0, false).is_some(), "VK {vk:#X} 应可映射");
        }
        for vk in 0x30..=0x39u32 {
            assert!(vk_to_code(vk, 0, false).is_some(), "VK {vk:#X} 应可映射");
        }
        for vk in 0x70..=0x87u32 {
            assert!(vk_to_code(vk, 0, false).is_some(), "VK {vk:#X} 应可映射");
        }
    }

    /// 左右修饰键必须区分：泛化 VK + scan/ext，以及驱动直接上报的左右具体码。
    #[test]
    fn modifier_left_right_distinction() {
        // 泛化 Shift：靠 scan code 区分
        assert_eq!(vk_to_code(0x10, 0x2A, false), Some("ShiftLeft"));
        assert_eq!(vk_to_code(0x10, 0x36, false), Some("ShiftRight"));
        // 泛化 Ctrl / Alt：靠扩展位区分
        assert_eq!(vk_to_code(0x11, 0, false), Some("ControlLeft"));
        assert_eq!(vk_to_code(0x11, 0, true), Some("ControlRight"));
        assert_eq!(vk_to_code(0x12, 0, false), Some("AltLeft"));
        assert_eq!(vk_to_code(0x12, 0, true), Some("AltRight"));
        // 具体码直接映射
        assert_eq!(vk_to_code(0xA0, 0, false), Some("ShiftLeft"));
        assert_eq!(vk_to_code(0xA1, 0, false), Some("ShiftRight"));
        assert_eq!(vk_to_code(0xA2, 0, false), Some("ControlLeft"));
        assert_eq!(vk_to_code(0xA3, 0, false), Some("ControlRight"));
        assert_eq!(vk_to_code(0x5B, 0, false), Some("MetaLeft"));
        assert_eq!(vk_to_code(0x5C, 0, false), Some("MetaRight"));
        // is_generic_modifier 与实现一致
        assert!(is_generic_modifier(0x10));
        assert!(is_generic_modifier(0x11));
        assert!(is_generic_modifier(0x12));
        assert!(is_generic_modifier(0x0D));
        assert!(!is_generic_modifier(0xA0));
    }

    /// 小键盘：数字/运算符/小数点齐全，NumpadEnter 靠扩展位与主 Enter 区分。
    #[test]
    fn numpad_keys_including_enter() {
        assert_eq!(vk_to_code(0x60, 0, false), Some("Numpad0"));
        assert_eq!(vk_to_code(0x69, 0, false), Some("Numpad9"));
        assert_eq!(vk_to_code(0x6A, 0, false), Some("NumpadMultiply"));
        assert_eq!(vk_to_code(0x6B, 0, false), Some("NumpadAdd"));
        assert_eq!(vk_to_code(0x6D, 0, false), Some("NumpadSubtract"));
        assert_eq!(vk_to_code(0x6E, 0, false), Some("NumpadDecimal"));
        assert_eq!(vk_to_code(0x6F, 0, false), Some("NumpadDivide"));
        assert_eq!(vk_to_code(0x90, 0, false), Some("NumLock"));

        // 主 Enter（ext=false）vs 小键盘 Enter（ext=true），同一 VK
        assert_eq!(vk_to_code(0x0D, 0x1C, false), Some("Enter"));
        assert_eq!(vk_to_code(0x0D, 0x1C, true), Some("NumpadEnter"));
    }

    /// OEM 符号键与编辑键命名正确（US 布局）。
    #[test]
    fn oem_and_editing_keys() {
        assert_eq!(vk_to_code(0xBA, 0, false), Some("Semicolon"));
        assert_eq!(vk_to_code(0xBB, 0, false), Some("Equal"));
        assert_eq!(vk_to_code(0xBC, 0, false), Some("Comma"));
        assert_eq!(vk_to_code(0xBD, 0, false), Some("Minus"));
        assert_eq!(vk_to_code(0xBE, 0, false), Some("Period"));
        assert_eq!(vk_to_code(0xBF, 0, false), Some("Slash"));
        assert_eq!(vk_to_code(0xC0, 0, false), Some("Backquote"));
        assert_eq!(vk_to_code(0xDB, 0, false), Some("BracketLeft"));
        assert_eq!(vk_to_code(0xDC, 0, false), Some("Backslash"));
        assert_eq!(vk_to_code(0xDD, 0, false), Some("BracketRight"));
        assert_eq!(vk_to_code(0xDE, 0, false), Some("Quote"));
        assert_eq!(vk_to_code(0xE2, 0, false), Some("IntlBackslash"));

        assert_eq!(vk_to_code(0x08, 0, false), Some("Backspace"));
        assert_eq!(vk_to_code(0x09, 0, false), Some("Tab"));
        assert_eq!(vk_to_code(0x14, 0, false), Some("CapsLock"));
        assert_eq!(vk_to_code(0x20, 0, false), Some("Space"));
        assert_eq!(vk_to_code(0x25, 0, true), Some("ArrowLeft"));
        assert_eq!(vk_to_code(0x2E, 0, true), Some("Delete"));
        assert_eq!(vk_to_code(0x2C, 0, false), Some("PrintScreen"));
    }

    /// IME 专用键归入独立命名（不与其他键冲突），VK_PROCESSKEY → IMEProcess。
    #[test]
    fn ime_keys_are_classified() {
        assert_eq!(vk_to_code(0xE5, 0, false), Some("IMEProcess"));
        assert_eq!(vk_to_code(0x15, 0, false), Some("KanaMode"));
        assert_eq!(vk_to_code(0x19, 0, false), Some("KanjiMode"));
        assert_eq!(vk_to_code(0x1C, 0, false), Some("Convert"));
        assert_eq!(vk_to_code(0x1D, 0, false), Some("NonConvert"));
        assert_eq!(vk_to_code(0x1F, 0, false), Some("IMEModeChange"));
    }

    /// 未识别键返回 None：VK_PACKET（注入）、鼠标键 VK、0x00/0xFF 保留码。
    #[test]
    fn unknown_vk_returns_none() {
        assert_eq!(vk_to_code(0xE7, 0, false), None, "VK_PACKET 应丢弃");
        assert_eq!(
            vk_to_code(0x01, 0, false),
            None,
            "鼠标左键 VK 不应出现在键盘钩子"
        );
        assert_eq!(vk_to_code(0x00, 0, false), None);
        assert_eq!(vk_to_code(0xFF, 0, false), None);
        assert_eq!(vk_to_code(0x92, 0, false), None, "VK 0x92 保留码");
    }

    /// 映射值唯一性：任何两个**不同物理键**不得映射到同一 code。
    /// 泛化修饰键（0x10/0x11/0x12/0x0D）是左右具体码的别名入口，从唯一性检查中排除。
    #[test]
    fn codes_are_unique_across_distinct_vks() {
        let mut seen: HashMap<&'static str, u32> = HashMap::new();
        for vk in 0u32..=0xFF {
            if is_generic_modifier(vk) {
                continue;
            }
            if let Some(code) = vk_to_code(vk, 0, false) {
                if let Some(prev) = seen.insert(code, vk) {
                    panic!("code {code} 同时来自 VK {prev:#X} 与 VK {vk:#X}（物理键冲突）");
                }
            }
        }
        // 泛化码在两侧 scan/ext 下产生不同 code，不构成冲突
        assert_ne!(vk_to_code(0x10, 0x2A, false), vk_to_code(0x10, 0x36, false));
    }

    /// 中文输入法场景：组合期间字母键码照常上报（可统计），仅 IME 专用键单独归类。
    #[test]
    fn ime_composition_still_reports_letter_keys() {
        // 拼音输入 "ni" 时按下 N/I 仍为字母 VK，热力图应统计到
        assert_eq!(vk_to_code(0x4E, 0, false), Some("KeyN"));
        assert_eq!(vk_to_code(0x49, 0, false), Some("KeyI"));
        // 空格选字 → Space 键
        assert_eq!(vk_to_code(0x20, 0, false), Some("Space"));
    }
}
