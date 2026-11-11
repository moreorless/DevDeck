import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** 归档的 /insights 报告；统计字段从报告头部解析，格式变化时为 null */
export interface InsightsReport {
  id: number;
  /** 源文件修改时间（UTC，RFC 3339） */
  generatedAt: string;
  subtitle: string | null;
  sessionsAnalyzed: number | null;
  sessionsTotal: number | null;
  periodStart: string | null;
  periodEnd: string | null;
  bytes: number;
  sourcePath: string;
}

export const listInsights = () => invoke<InsightsReport[]>("list_insights");
export const syncInsights = () => invoke<number>("sync_insights");
export const getInsightHtml = (id: number) => invoke<string>("get_insight_html", { id });
export const openInsightExternal = (id: number) => invoke<void>("open_insight_external", { id });
export const deleteInsight = (id: number) => invoke<void>("delete_insight", { id });
export const clearInsights = () => invoke<number>("clear_insights");

export const EVT_INSIGHTS_UPDATED = "insights://updated";
export const onInsightsUpdated = (fn: () => void): Promise<UnlistenFn> => listen(EVT_INSIGHTS_UPDATED, fn);

/** 本地时间 YYYY-MM-DD HH:mm */
export function fmtLocal(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
