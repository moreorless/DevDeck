import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 一个额度窗口，来自 Claude Code 的 /usage */
export interface QuotaWindow {
  /** session：当前 5 小时会话；week：7 天 */
  kind: "session" | "week";
  /** 周额度的范围：null 为全部模型，否则为模型名（如 Fable） */
  scope: string | null;
  usedPercent: number;
  /** 重置时间（UTC，RFC 3339）；解析不了时为 null，界面显示 resetsText 原文 */
  resetsAt: string | null;
  resetsText: string | null;
}

export interface QuotaSnapshot {
  windows: QuotaWindow[];
  /** 最近一次成功读取的时间（UTC） */
  fetchedAt: string | null;
  /** 没有额度信息时的说明（如 API Key 计费） */
  note: string | null;
  /** 最近一次读取失败的原因 */
  error: string | null;
  refreshing: boolean;
}

export const getQuota = () => invoke<QuotaSnapshot>("get_quota");
export const refreshQuota = () => invoke<QuotaSnapshot>("refresh_quota");

export const EVT_QUOTA_UPDATED = "quota://updated";
export const onQuotaUpdated = (fn: (s: QuotaSnapshot) => void): Promise<UnlistenFn> =>
  listen<QuotaSnapshot>(EVT_QUOTA_UPDATED, (e) => fn(e.payload));

export function windowLabel(w: QuotaWindow): string {
  if (w.kind === "session") return "当前会话（5 小时）";
  return w.scope ? `本周 · ${w.scope}` : "本周 · 全部模型";
}

/** 本地时间 MM-DD HH:mm */
export function fmtLocalShort(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 距现在的剩余时间：`2 天 3 小时`、`3 小时 5 分`、`12 分`；已过去返回 null */
export function fmtRemaining(ts: string, now: number): string | null {
  const ms = new Date(ts).getTime() - now;
  if (Number.isNaN(ms) || ms <= 0) return null;
  const min = Math.ceil(ms / 60000);
  const d = Math.floor(min / 1440);
  const h = Math.floor((min % 1440) / 60);
  const m = min % 60;
  if (d > 0) return h ? `${d} 天 ${h} 小时` : `${d} 天`;
  if (h > 0) return m ? `${h} 小时 ${m} 分` : `${h} 小时`;
  return `${m} 分`;
}
