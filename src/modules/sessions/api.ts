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

/** 可能被截断的文本：fullLen 大于 text 的长度说明有截断 */
export interface Clipped {
  text: string;
  fullLen: number;
}

export interface ToolResult {
  content: Clipped;
  isError: boolean;
  images: number;
}

interface Base {
  uuid: string;
  ts: string | null;
}

export type Item =
  | (Base & { kind: "user"; text: string; images: number })
  | (Base & { kind: "assistant"; model: string | null; text: string })
  | (Base & { kind: "thinking"; text: Clipped })
  | (Base & {
      kind: "toolUse";
      id: string;
      name: string;
      input: Clipped;
      result: ToolResult | null;
      agentId: string | null;
      agentAvailable: boolean;
    })
  | (Base & { kind: "compact"; trigger: string | null; preTokens: number | null; summary: Clipped | null })
  | (Base & { kind: "meta"; label: string; text: Clipped });

export interface SessionDetail {
  sessionId: string;
  agentId: string | null;
  project: string | null;
  filePath: string;
  items: Item[];
  badLines: number;
}

export const listSessions = (query: SessionQuery) => invoke<SessionPage>("list_sessions", { query });
export const getSession = (sessionId: string) => invoke<SessionDetail>("get_session", { sessionId });
export const getSubagentSession = (sessionId: string, agentId: string) =>
  invoke<SessionDetail>("get_subagent_session", { sessionId, agentId });
export const openProjectDir = (sessionId: string) => invoke<void>("open_project_dir", { sessionId });
