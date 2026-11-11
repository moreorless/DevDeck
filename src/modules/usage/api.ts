import { invoke } from "@tauri-apps/api/core";

export interface UsageFilter {
  /** YYYY-MM-DD，含 */
  since: string;
  until: string;
  projects?: string[];
  models?: string[];
}

export interface DailyModelUsage {
  date: string;
  model: string;
  inputTokens: number;
  outputTokens: number;
  cacheWrite5mTokens: number;
  cacheWrite1hTokens: number;
  cacheReadTokens: number;
  totalTokens: number;
  costUsd: number;
  /** false 表示该模型不在价格表中，费用未计入 */
  priced: boolean;
}

export interface Totals {
  tokens: number;
  costUsd: number;
}

export interface Summary {
  today: Totals;
  yesterday: Totals;
  last7: Totals;
  prev7: Totals;
  last30: Totals;
  prev30: Totals;
  unpricedModels: string[];
}

export const getDailyUsage = (filter: UsageFilter) => invoke<DailyModelUsage[]>("get_daily_usage", { filter });
export const getSummary = () => invoke<Summary>("get_summary");
export const listProjects = () => invoke<string[]>("list_projects");
export const listModels = () => invoke<string[]>("list_models");
/** 写出 CSV，返回行数 */
export const exportCsv = (path: string, filter: UsageFilter) => invoke<number>("export_csv", { path, filter });
