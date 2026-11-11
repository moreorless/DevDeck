import { invoke } from "@tauri-apps/api/core";

export interface SessionQuery {
  project?: string;
  /** YYYY-MM-DD，与会话起止时间有交集 */
  since?: string;
  until?: string;
  keyword?: string;
  offset?: number;
  limit?: number;
}

export interface SessionSummary {
  sessionId: string;
  project: string | null;
  gitBranch: string | null;
  title: string | null;
  firstPrompt: string | null;
  lastPrompt: string | null;
  startedAt: string | null;
  endedAt: string | null;
  promptCount: number;
  models: string[];
  hasSubagents: boolean;
  totalTokens: number;
  costUsd: number;
  priced: boolean;
}

export interface SessionPage {
  total: number;
  items: SessionSummary[];
}

export const listSessions = (query: SessionQuery) => invoke<SessionPage>("list_sessions", { query });
