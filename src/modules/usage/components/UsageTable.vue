<script setup lang="ts">
import { computed, h } from "vue";
import { NCard, NDataTable, NButton, type DataTableColumns } from "naive-ui";
import type { DailyModelUsage } from "../api";
import { exactNumber, exactUsd, fmtTokens, fmtUsd } from "../../../shared/format";

const props = defineProps<{ rows: DailyModelUsage[]; exporting: boolean }>();
const emit = defineEmits<{ export: [] }>();

interface Row {
  key: string;
  date: string;
  model: string;
  input: number;
  output: number;
  cacheWrite: number;
  cacheRead: number;
  total: number;
  cost: number;
  priced: boolean;
  children?: Row[];
}

function toRow(r: DailyModelUsage): Row {
  return {
    key: `${r.date}|${r.model}`,
    date: r.date,
    model: r.model,
    input: r.inputTokens,
    output: r.outputTokens,
    cacheWrite: r.cacheWrite5mTokens + r.cacheWrite1hTokens,
    cacheRead: r.cacheReadTokens,
    total: r.totalTokens,
    cost: r.costUsd,
    priced: r.priced,
  };
}

/** 每天一行汇总，展开后按模型拆分；日期倒序 */
const data = computed<Row[]>(() => {
  const byDay = new Map<string, Row[]>();
  for (const r of props.rows) {
    const list = byDay.get(r.date) ?? [];
    list.push(toRow(r));
    byDay.set(r.date, list);
  }
  return [...byDay.entries()]
    .sort((a, b) => b[0].localeCompare(a[0]))
    .map(([date, list]) => {
      list.sort((a, b) => b.total - a.total);
      const sum = (k: keyof Pick<Row, "input" | "output" | "cacheWrite" | "cacheRead" | "total" | "cost">) =>
        list.reduce((a, r) => a + r[k], 0);
      return {
        key: date,
        date,
        model: `${list.length} 个模型`,
        input: sum("input"),
        output: sum("output"),
        cacheWrite: sum("cacheWrite"),
        cacheRead: sum("cacheRead"),
        total: sum("total"),
        cost: sum("cost"),
        priced: list.every((r) => r.priced),
        children: list.map((r) => ({ ...r, key: r.key, date: "" })),
      };
    });
});

const num = (k: "input" | "output" | "cacheWrite" | "cacheRead" | "total") => (row: Row) =>
  h("span", { title: exactNumber(row[k]) }, fmtTokens(row[k]));

const columns: DataTableColumns<Row> = [
  { title: "日期", key: "date", width: 130 },
  { title: "模型", key: "model", ellipsis: { tooltip: true } },
  { title: "Input", key: "input", align: "right", render: num("input") },
  { title: "Output", key: "output", align: "right", render: num("output") },
  { title: "Cache 写", key: "cacheWrite", align: "right", render: num("cacheWrite") },
  { title: "Cache 读", key: "cacheRead", align: "right", render: num("cacheRead") },
  { title: "合计", key: "total", align: "right", render: num("total") },
  {
    title: "估算费用",
    key: "cost",
    align: "right",
    render: (row) =>
      h(
        "span",
        { title: row.priced ? exactUsd(row.cost) : `${exactUsd(row.cost)}（含价格未知的模型，未计入）` },
        row.priced ? fmtUsd(row.cost) : `${fmtUsd(row.cost)} *`,
      ),
  },
];
</script>

<template>
  <n-card size="small" title="明细">
    <template #header-extra>
      <n-button size="small" :loading="exporting" :disabled="!rows.length" @click="emit('export')">导出 CSV</n-button>
    </template>
    <n-data-table
      :columns="columns"
      :data="data"
      :row-key="(r: Row) => r.key"
      :pagination="{ pageSize: 31 }"
      size="small"
      class="tabular"
    />
  </n-card>
</template>

<style scoped>
.tabular :deep(td) {
  font-variant-numeric: tabular-nums;
}
</style>
