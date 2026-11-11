import { defineStore } from "pinia";
import { computed, ref } from "vue";
import { save } from "@tauri-apps/plugin-dialog";
import { addDays, today, ymd } from "../../shared/date";
import { getScanStatus, onDataEvents, refresh as runRefresh, type ScanStatus } from "../../shared/data";
import {
  exportCsv,
  getDailyUsage,
  getSummary,
  listModels,
  listProjects,
  type DailyModelUsage,
  type Summary,
  type UsageFilter,
} from "./api";

export type RangePreset = "today" | "7d" | "30d" | "month" | "custom";
export type StackBy = "type" | "model";

function presetRange(p: Exclude<RangePreset, "custom">): [string, string] {
  const t = today();
  switch (p) {
    case "today":
      return [t, t];
    case "7d":
      return [addDays(t, -6), t];
    case "30d":
      return [addDays(t, -29), t];
    case "month": {
      const d = new Date();
      return [ymd(new Date(d.getFullYear(), d.getMonth(), 1)), t];
    }
  }
}

export const useUsageStore = defineStore("usage", () => {
  const preset = ref<RangePreset>("30d");
  const range = ref<[string, string]>(presetRange("30d"));
  const projects = ref<string[]>([]);
  const models = ref<string[]>([]);
  const stackBy = ref<StackBy>("type");

  const rows = ref<DailyModelUsage[]>([]);
  const summary = ref<Summary | null>(null);
  /** 全部项目与模型（筛选项，也用于固定模型颜色） */
  const allProjects = ref<string[]>([]);
  const allModels = ref<string[]>([]);

  const status = ref<ScanStatus | null>(null);
  const progress = ref<{ done: number; total: number } | null>(null);
  const loading = ref(false);
  const refreshing = ref(false);
  const scanError = ref<string | null>(null);
  const loadError = ref<string | null>(null);

  const filter = computed<UsageFilter>(() => ({
    since: range.value[0],
    until: range.value[1],
    projects: projects.value,
    models: models.value,
  }));

  /** 按全量用量从大到小排列的模型，颜色按此顺序固定分配 */
  const modelOrder = ref<string[]>([]);

  async function loadOptions() {
    const [p, m] = await Promise.all([listProjects(), listModels()]);
    allProjects.value = p;
    allModels.value = m;
    // 剔除已不存在的选项，避免筛选落空
    projects.value = projects.value.filter((x) => p.includes(x));
    models.value = models.value.filter((x) => m.includes(x));
  }

  async function loadModelOrder() {
    const all = await getDailyUsage({ since: "1970-01-01", until: "9999-12-31" });
    const totals = new Map<string, number>();
    for (const r of all) totals.set(r.model, (totals.get(r.model) ?? 0) + r.totalTokens);
    modelOrder.value = [...totals.entries()].sort((a, b) => b[1] - a[1]).map(([m]) => m);
  }

  async function loadData() {
    loading.value = true;
    try {
      const [r, s] = await Promise.all([getDailyUsage(filter.value), getSummary()]);
      rows.value = r;
      summary.value = s;
      loadError.value = null;
    } catch (e) {
      loadError.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  async function loadStatus() {
    try {
      status.value = await getScanStatus();
      const last = status.value.lastScan;
      scanError.value = last && !last.ok ? last.error : null;
    } catch (e) {
      scanError.value = String(e);
    }
  }

  async function reloadAll() {
    await Promise.all([loadOptions(), loadModelOrder(), loadStatus()]);
    await loadData();
  }

  let listening = false;
  async function init() {
    if (!listening) {
      listening = true;
      await onDataEvents({
        updated: () => {
          progress.value = null;
          reloadAll();
        },
        error: (msg) => {
          progress.value = null;
          scanError.value = msg;
          loadStatus();
        },
        progress: (p) => (progress.value = p),
      });
    }
    await reloadAll();
  }

  function setPreset(p: RangePreset) {
    preset.value = p;
    if (p !== "custom") range.value = presetRange(p);
    loadData();
  }

  function setRange(r: [string, string]) {
    preset.value = "custom";
    range.value = r;
    loadData();
  }

  function setProjects(v: string[]) {
    projects.value = v;
    loadData();
  }

  function setModels(v: string[]) {
    models.value = v;
    loadData();
  }

  /** 手动刷新：增量扫描，完成后由 data://updated 事件触发重新加载 */
  async function refresh() {
    refreshing.value = true;
    try {
      await runRefresh("incremental");
    } catch (e) {
      scanError.value = String(e);
    } finally {
      refreshing.value = false;
    }
  }

  /** 导出 CSV；用户取消返回 null */
  async function exportToCsv(): Promise<{ path: string; rows: number } | null> {
    const path = await save({
      defaultPath: `devdeck-usage-${filter.value.since}_${filter.value.until}.csv`,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return null;
    const n = await exportCsv(path, filter.value);
    return { path, rows: n };
  }

  return {
    preset,
    range,
    projects,
    models,
    stackBy,
    rows,
    summary,
    allProjects,
    allModels,
    modelOrder,
    status,
    progress,
    loading,
    refreshing,
    scanError,
    loadError,
    filter,
    init,
    setPreset,
    setRange,
    setProjects,
    setModels,
    refresh,
    exportToCsv,
  };
});
