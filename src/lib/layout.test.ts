import { describe, expect, it } from "vitest";
import {
  ANSI_104,
  LAYOUTS,
  TKL_87,
  defaultLabel,
  keysOf,
  layoutBounds,
  placeKeys,
} from "./layout";

describe("布局数据完整性", () => {
  it("104 键布局恰好 104 个按键（无重复 code）", () => {
    const keys = keysOf(ANSI_104);
    expect(keys).toHaveLength(104);
    const codes = keys.map((k) => k.code);
    expect(new Set(codes).size).toBe(104);
  });

  it("87 键布局（TKL）恰好 87 个按键，且为 104 布局去掉小键盘", () => {
    const keys = keysOf(TKL_87);
    expect(keys).toHaveLength(87);
    const codes = new Set(keys.map((k) => k.code));
    const full = new Set(keysOf(ANSI_104).map((k) => k.code));
    // TKL = 全尺寸 - 小键盘 17 键
    const numpad = [...full].filter((c) => c.startsWith("Numpad") || c === "NumLock");
    expect(numpad).toHaveLength(17);
    for (const n of numpad) expect(codes.has(n)).toBe(false);
    for (const c of codes) expect(full.has(c)).toBe(true);
  });

  it("所有布局都包含核心修饰键与空格", () => {
    for (const layout of LAYOUTS) {
      const codes = new Set(keysOf(layout).map((k) => k.code));
      for (const required of [
        "Space",
        "Enter",
        "Backspace",
        "Tab",
        "Escape",
        "ShiftLeft",
        "ShiftRight",
        "ControlLeft",
        "ControlRight",
        "AltLeft",
        "AltRight",
        "MetaLeft",
        "ArrowUp",
        "ArrowLeft",
      ]) {
        expect(codes.has(required), `${layout.id} 缺少 ${required}`).toBe(true);
      }
    }
  });
});

describe("placeKeys 几何展开", () => {
  it("同一行内按键不重叠且按序排列", () => {
    const placed = placeKeys(ANSI_104);
    // 按 (y, x) 排序后，同一 y 上的相邻键应满足 prev.x + prev.w <= next.x
    const byRow = new Map<number, typeof placed>();
    for (const k of placed) {
      const row = byRow.get(k.y) ?? [];
      row.push(k);
      byRow.set(k.y, row);
    }
    for (const [y, row] of byRow) {
      row.sort((a, b) => a.x - b.x);
      for (let i = 1; i < row.length; i++) {
        expect(row[i - 1].x + row[i - 1].w, `行 ${y} 第 ${i} 键重叠`).toBeLessThanOrEqual(row[i].x);
      }
    }
  });

  it("宽度换算正确：Space 为 6.25u，Backspace 为 2u", () => {
    const placed = placeKeys(ANSI_104);
    const space = placed.find((k) => k.code === "Space")!;
    expect(space.w).toBe(6.25);
    const bksp = placed.find((k) => k.code === "Backspace")!;
    expect(bksp.w).toBe(2);
  });

  it("包围盒覆盖全部按键（含功能键行的负 y 偏移）", () => {
    const bounds = layoutBounds(ANSI_104);
    const placed = placeKeys(ANSI_104);
    expect(bounds.minY).toBeLessThan(0); // 功能键行在 y=-1.5
    for (const k of placed) {
      expect(k.x + k.w).toBeLessThanOrEqual(bounds.width);
      expect(k.y + 1).toBeLessThanOrEqual(bounds.height + bounds.minY);
    }
    // 宽度应容纳主区(15u)+间隙+导航(3u)+小键盘(4u) ≈ 23u
    expect(bounds.width).toBeGreaterThanOrEqual(23);
  });

  it("间隙不产生按键", () => {
    const placed = placeKeys(ANSI_104);
    expect(placed.every((k) => k.code.length > 0)).toBe(true);
  });
});

describe("defaultLabel", () => {
  it("字母/数字键取简洁标签，其余保留原名", () => {
    expect(defaultLabel("KeyA")).toBe("A");
    expect(defaultLabel("Digit5")).toBe("5");
    expect(defaultLabel("Space")).toBe("Space");
    expect(defaultLabel("ShiftLeft")).toBe("ShiftLeft");
  });
});
