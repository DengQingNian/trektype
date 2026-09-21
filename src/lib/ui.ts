/**
 * 全局 UI 反馈（naive-ui 离散 API）：
 * 无需在组件树里包 provider，任何模块/页面都能提示或确认。
 */
import { createDiscreteApi } from "naive-ui";

export const { message, dialog } = createDiscreteApi(["message", "dialog"]);

/** 统一的错误提示（命令返回 string 错误）。 */
export function toastError(e: unknown, context: string) {
  const detail = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  message.error(`${context}：${detail}`, { duration: 6000 });
}
