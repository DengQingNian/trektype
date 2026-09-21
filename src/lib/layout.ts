/**
 * 键盘布局：以"块（Block）+ 行（Row）"描述，按键宽度用 u 单位（1u = 标准字母键宽）。
 *
 * 布局数据与渲染解耦：新增布局（如 108 键/自定义）只需追加一个 LayoutDef。
 * 键位 code 与后端 `capture/keymap.rs` 的 KeyboardEvent.code 风格命名一一对应。
 */

export interface KeyDef {
  /** 标准键位标识（"KeyA"/"Digit1"/"Space"）；空串表示占位间隙（不渲染按键） */
  code: string;
  /** 键帽显示文字（缺省由 code 推导） */
  label?: string;
  /** 宽度（u），默认 1 */
  w?: number;
}

export interface Block {
  /** 块左上角相对布局原点的 u 坐标 */
  x: number;
  y: number;
  /** 块内每行：键按顺序排列，宽度自动累加 */
  rows: KeyDef[][];
}

export interface LayoutDef {
  id: string;
  name: string;
  blocks: Block[];
}

/** 间隙占位（渲染时跳过、仅占宽度）。 */
export const GAP: KeyDef = { code: "", w: 0.5 };

/** 便捷构造键。 */
function k(code: string, label?: string, w = 1): KeyDef {
  return { code, label, w };
}

/** 占位（用于对齐空位）。 */
function g(w = 1): KeyDef {
  return { code: "", w };
}

/** 字母键行：把连续字母序列展开为 KeyX 键位（如 "QWERTYUIOP"）。 */
function keyRun(seq: string): KeyDef[] {
  return [...seq].map((ch) => k(`Key${ch}`));
}

function digits(): KeyDef[] {
  return Array.from({ length: 10 }, (_, i) => k(`Digit${(i + 1) % 10}`));
}

function fkeys(): KeyDef[] {
  const out: KeyDef[] = [k("Escape", "Esc")];
  for (let i = 1; i <= 12; i++) {
    out.push(k(`F${i}`));
    if (i === 4 || i === 8) out.push(GAP);
  }
  return out;
}

/** 主键区（61 键），所有布局共用。 */
const MAIN_BLOCK: Block = {
  x: 0,
  y: 0,
  rows: [
    [
      k("Backquote", "`"),
      ...digits(),
      k("Minus", "-"),
      k("Equal", "="),
      k("Backspace", "Bksp", 2),
    ],
    [
      k("Tab", "Tab", 1.5),
      ...keyRun("QWERTYUIOP"),
      k("BracketLeft", "["),
      k("BracketRight", "]"),
      k("Backslash", "\\", 1.5),
    ],
    [
      k("CapsLock", "Caps", 1.75),
      ...keyRun("ASDFGHJKL"),
      k("Semicolon", ";"),
      k("Quote", "'"),
      k("Enter", "Enter", 2.25),
    ],
    [
      k("ShiftLeft", "Shift", 2.25),
      ...keyRun("ZXCVBNM"),
      k("Comma", ","),
      k("Period", "."),
      k("Slash", "/"),
      k("ShiftRight", "Shift", 2.75),
    ],
    [
      k("ControlLeft", "Ctrl", 1.25),
      k("MetaLeft", "Win", 1.25),
      k("AltLeft", "Alt", 1.25),
      k("Space", "Space", 6.25),
      k("AltRight", "Alt", 1.25),
      k("MetaRight", "Win", 1.25),
      k("ContextMenu", "Menu", 1.25),
      k("ControlRight", "Ctrl", 1.25),
    ],
  ],
};

/** 功能键行（16 键），独立于主区顶部。 */
const FUNCTION_BLOCK: Block = {
  x: 0,
  y: -1.5,
  rows: [[...fkeys(), GAP, k("PrintScreen", "PrtSc"), k("ScrollLock", "ScrLk"), k("Pause", "Pause")]],
};

/** 导航区（10 键）。 */
const NAV_BLOCK: Block = {
  x: 15.5,
  y: 0,
  rows: [
    [k("Insert", "Ins"), k("Home", "Home"), k("PageUp", "PgUp")],
    [k("Delete", "Del"), k("End", "End"), k("PageDown", "PgDn")],
    // 空行占位（与主区第 3 行对齐）
    [g(3)],
    [g(1), k("ArrowUp", "↑"), g(1)],
    [k("ArrowLeft", "←"), k("ArrowDown", "↓"), k("ArrowRight", "→")],
  ],
};

/** 小键盘区（17 键）。 */
const NUMPAD_BLOCK: Block = {
  x: 19,
  y: 0,
  rows: [
    [k("NumLock", "Num"), k("NumpadDivide", "/"), k("NumpadMultiply", "*"), k("NumpadSubtract", "-")],
    [k("Numpad7", "7"), k("Numpad8", "8"), k("Numpad9", "9"), k("NumpadAdd", "+")],
    [k("Numpad4", "4"), k("Numpad5", "5"), k("Numpad6", "6"), g(1)],
    [k("Numpad1", "1"), k("Numpad2", "2"), k("Numpad3", "3"), k("NumpadEnter", "Enter")],
    [k("Numpad0", "0", 2), k("NumpadDecimal", "."), g(1)],
  ],
};

/** 104 键 ANSI 全尺寸。 */
export const ANSI_104: LayoutDef = {
  id: "ansi-104",
  name: "104 键（全尺寸）",
  blocks: [FUNCTION_BLOCK, MAIN_BLOCK, NAV_BLOCK, NUMPAD_BLOCK],
};

/** 87 键 TKL（无小键盘）。 */
export const TKL_87: LayoutDef = {
  id: "tkl-87",
  name: "87 键（TKL，无小键盘）",
  blocks: [FUNCTION_BLOCK, MAIN_BLOCK, NAV_BLOCK],
};

export const LAYOUTS: LayoutDef[] = [ANSI_104, TKL_87];

/** 布局中所有真实按键（不含间隙），用于校验与"未映射键"检查。 */
export function keysOf(layout: LayoutDef): KeyDef[] {
  const out: KeyDef[] = [];
  for (const block of layout.blocks) {
    for (const row of block.rows) {
      for (const key of row) {
        if (key.code) out.push(key);
      }
    }
  }
  return out;
}

/** 展开后的键几何信息（渲染用）：x/y/w 均已换算为 u 坐标。 */
export interface PlacedKey {
  code: string;
  label: string;
  x: number;
  y: number;
  w: number;
}

/** 键帽默认标签：KeyA → A，Digit1 → 1，其余原名。 */
export function defaultLabel(code: string): string {
  if (code.startsWith("Key")) return code.slice(3);
  if (code.startsWith("Digit")) return code.slice(5);
  return code;
}

/**
 * 展开布局为绝对坐标的键列表。
 * 行内 x 依次累加宽度；`y` 为行号（每行 1u 高），块内行号 + 块 y 偏移。
 */
export function placeKeys(layout: LayoutDef): PlacedKey[] {
  const out: PlacedKey[] = [];
  for (const block of layout.blocks) {
    block.rows.forEach((row, rowIdx) => {
      let cursor = block.x;
      for (const key of row) {
        const w = key.w ?? 1;
        if (key.code) {
          out.push({
            code: key.code,
            label: key.label ?? defaultLabel(key.code),
            x: cursor,
            y: block.y + rowIdx,
            w,
          });
        }
        cursor += w;
      }
    });
  }
  return out;
}

/** 布局包围盒（u 单位），渲染缩放用。 */
export function layoutBounds(layout: LayoutDef): { width: number; height: number; minY: number } {
  const keys = placeKeys(layout);
  let maxX = 0;
  let maxY = 0;
  let minY = 0;
  for (const k of keys) {
    maxX = Math.max(maxX, k.x + k.w);
    maxY = Math.max(maxY, k.y + 1);
    minY = Math.min(minY, k.y);
  }
  return { width: maxX, height: maxY - minY, minY };
}
