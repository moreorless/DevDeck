import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 与 Rust 侧 scan_service 对应的扫描接口与事件 */

export type ScanMode = "incremental" | "full";

export interface ScanStats {
  filesTotal: number;
  filesRead: number;
  bytesRead: number;
  lines: number;
  badLines: number;
  usageRecords: number;
  durationMs: number;
}

export interface LastScan {
  startedAt: string;
  finishedAt: string | null;
  mode: ScanMode;
  filesTotal: number | null;
  badLines: number | null;
  ok: boolean;
  error: string | null;
}

export interface ScanStatus {
  scanning: boolean;
  lastScan: LastScan | null;
  checkedDirs: string[];
  foundDirs: string[];
  sessionCount: number;
  usageCount: number;
  pricingCheckedAt: string;
}

export const refresh = (mode: ScanMode) => invoke<ScanStats>("refresh", { mode });
export const getScanStatus = () => invoke<ScanStatus>("get_scan_status");

export const EVT_UPDATED = "data://updated";
export const EVT_ERROR = "data://error";
export const EVT_PROGRESS = "data://progress";

/** 监听数据更新、失败、进度事件，返回取消函数 */
export async function onDataEvents(handlers: {
  updated?: (s: ScanStats) => void;
  error?: (msg: string) => void;
  progress?: (p: { done: number; total: number }) => void;
}): Promise<UnlistenFn> {
  const fns = await Promise.all([
    listen<ScanStats>(EVT_UPDATED, (e) => handlers.updated?.(e.payload)),
    listen<string>(EVT_ERROR, (e) => handlers.error?.(e.payload)),
    listen<{ done: number; total: number }>(EVT_PROGRESS, (e) => handlers.progress?.(e.payload)),
  ]);
  return () => fns.forEach((f) => f());
}
