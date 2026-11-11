import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { onDataEvents } from "../../shared/data";
import { listProjects } from "../usage/api";
import { listRecentProjects, type RecentProject } from "../../shared/projects";
import {
  getSession,
  getSubagentSession,
  listSessions,
  type SessionDetail,
  type SessionSummary,
} from "./api";

const PAGE = 50;

export const useSessionsStore = defineStore("sessions", () => {
  // 列表与筛选
  const keyword = ref("");
  const project = ref<string | null>(null);
  const range = ref<[string, string] | null>(null);
  const items = ref<SessionSummary[]>([]);
  const total = ref(0);
  const listLoading = ref(false);
  const listError = ref<string | null>(null);
  const projects = ref<string[]>([]);
  const recentProjects = ref<RecentProject[]>([]);

  // 详情：栈底是主会话，之上是逐层打开的子代理
  const selectedId = ref<string | null>(null);
  const stack = ref<SessionDetail[]>([]);
  const detailLoading = ref(false);
  const detailError = ref<string | null>(null);
  /** 显示系统注入内容（meta、附件、hook 输出等） */
  const showMeta = ref(false);
  /** 仅显示用户请求（隐藏助手回复、工具调用等），便于快速浏览提问脉络 */
  const userOnly = ref(false);

  const selected = computed(() => items.value.find((s) => s.sessionId === selectedId.value) ?? null);
  const current = computed(() => stack.value[stack.value.length - 1] ?? null);
  const hasMore = computed(() => items.value.length < total.value);

  function query(offset: number) {
    return {
      keyword: keyword.value.trim() || undefined,
      project: project.value ?? undefined,
      since: range.value?.[0],
      until: range.value?.[1],
      offset,
      limit: PAGE,
    };
  }

  async function loadList(reset = true) {
    listLoading.value = true;
    try {
      const page = await listSessions(query(reset ? 0 : items.value.length));
      items.value = reset ? page.items : [...items.value, ...page.items];
      total.value = page.total;
      listError.value = null;
    } catch (e) {
      listError.value = String(e);
    } finally {
      listLoading.value = false;
    }
  }

  async function select(id: string) {
    selectedId.value = id;
    stack.value = [];
    detailLoading.value = true;
    detailError.value = null;
    try {
      const d = await getSession(id);
      // 期间可能已切到别的会话
      if (selectedId.value === id) stack.value = [d];
    } catch (e) {
      if (selectedId.value === id) detailError.value = String(e);
    } finally {
      if (selectedId.value === id) detailLoading.value = false;
    }
  }

  async function openSubagent(agentId: string) {
    const id = selectedId.value;
    if (!id) return;
    detailLoading.value = true;
    detailError.value = null;
    try {
      const d = await getSubagentSession(id, agentId);
      if (selectedId.value === id) stack.value = [...stack.value, d];
    } catch (e) {
      detailError.value = String(e);
    } finally {
      detailLoading.value = false;
    }
  }

  /** 回到第 depth 层（0 = 主会话） */
  function popTo(depth: number) {
    stack.value = stack.value.slice(0, depth + 1);
  }

  let listening = false;
  async function init() {
    if (!listening) {
      listening = true;
      // 扫描完成后刷新列表；正在查看的详情不自动重载，避免阅读位置跳动
      await onDataEvents({
        updated: () => {
          loadOptions();
          loadList();
        },
      });
    }
    await loadOptions();
    await loadList();
  }

  async function loadOptions() {
    const [p, r] = await Promise.all([listProjects().catch(() => []), listRecentProjects().catch(() => [])]);
    projects.value = p;
    recentProjects.value = r;
  }

  /** 快捷标签：单选，再点一次取消 */
  function toggleProject(p: string) {
    project.value = project.value === p ? null : p;
    loadList();
  }

  return {
    keyword,
    project,
    range,
    items,
    total,
    listLoading,
    listError,
    projects,
    recentProjects,
    toggleProject,
    selectedId,
    selected,
    stack,
    current,
    detailLoading,
    detailError,
    showMeta,
    userOnly,
    hasMore,
    init,
    loadList,
    select,
    openSubagent,
    popTo,
  };
});
