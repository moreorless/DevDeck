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
  /** 已有中文译文 */
  translated: boolean;
  /** 从 DevDeck 生成与翻译的估算费用（美元）；手动运行 /insights 的报告为 null */
  costUsd: number | null;
}

/** 后台任务状态（生成 / 翻译），同一时刻只有一个 */
export interface JobStatus {
  running: boolean;
  phase: "generating" | "translating" | null;
  startedAt: string | null;
  finishedAt: string | null;
  message: string | null;
  error: string | null;
  costUsd: number | null;
  reportId: number | null;
}

export const listInsights = () => invoke<InsightsReport[]>("list_insights");
export const syncInsights = () => invoke<number>("sync_insights");
export const getInsightHtml = (id: number, zh: boolean) => invoke<string>("get_insight_html", { id, zh });
export const openInsightExternal = (id: number, zh: boolean) => invoke<void>("open_insight_external", { id, zh });
export const deleteInsight = (id: number) => invoke<void>("delete_insight", { id });
export const clearInsights = () => invoke<number>("clear_insights");
export const generateInsights = () => invoke<void>("generate_insights");
export const translateInsight = (id: number) => invoke<void>("translate_insight", { id });
export const getInsightsJob = () => invoke<JobStatus>("get_insights_job");
export const detectClaude = () => invoke<string>("detect_claude");

export const EVT_INSIGHTS_UPDATED = "insights://updated";
export const onInsightsUpdated = (fn: () => void): Promise<UnlistenFn> => listen(EVT_INSIGHTS_UPDATED, fn);
export const EVT_INSIGHTS_JOB = "insights://job";
export const onInsightsJob = (fn: (s: JobStatus) => void): Promise<UnlistenFn> =>
  listen<JobStatus>(EVT_INSIGHTS_JOB, (e) => fn(e.payload));

/** 本地时间 YYYY-MM-DD HH:mm */
export function fmtLocal(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}
