import { invoke } from "@tauri-apps/api/core";
import type { SelectOption, SelectGroupOption } from "naive-ui";

export interface RecentProject {
  project: string;
  lastActiveAt: string;
  sessionCount: number;
}

/** 最近在 Claude Code 中活跃的项目（按最后活动时间倒序） */
export const listRecentProjects = (limit = 6) => invoke<RecentProject[]>("list_recent_projects", { limit });

/** 项目路径只显示最后一段 */
export const shortProject = (p: string | null | undefined) =>
  p ? (p.split(/[\\/]/).filter(Boolean).pop() ?? p) : "-";

/** 下拉选项：「最近使用」在前，其余按名称排在「全部项目」 */
export function groupedProjectOptions(all: string[], recent: RecentProject[]): (SelectOption | SelectGroupOption)[] {
  const recentNames = recent.map((r) => r.project).filter((p) => all.includes(p));
  const rest = all.filter((p) => !recentNames.includes(p));
  const opt = (p: string) => ({ label: p, value: p });
  if (!recentNames.length) return all.map(opt);
  return [
    { type: "group", label: "最近使用", key: "recent", children: recentNames.map(opt) },
    { type: "group", label: "全部项目", key: "all", children: rest.map(opt) },
  ];
}
