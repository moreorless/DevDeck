<script setup lang="ts">
import { computed } from "vue";
import { use } from "echarts/core";
import { BarChart } from "echarts/charts";
import { GridComponent, LegendComponent, TooltipComponent } from "echarts/components";
import { CanvasRenderer } from "echarts/renderers";
import VChart from "vue-echarts";
import type { EChartsOption } from "echarts";
import { NCard, NRadioGroup, NRadioButton, NSpace, useThemeVars } from "naive-ui";
import type { DailyModelUsage } from "../api";
import type { StackBy } from "../store";
import { eachDay, shortDate } from "../../../shared/date";
import { exactNumber, fmtTokens, fmtTokensAxis, fmtUsd } from "../../../shared/format";
import { chartTheme, MAX_CATEGORIES } from "../../../shared/palette";

use([BarChart, GridComponent, LegendComponent, TooltipComponent, CanvasRenderer]);

const props = defineProps<{
  rows: DailyModelUsage[];
  since: string;
  until: string;
  stackBy: StackBy;
  /** 模型的固定颜色顺序（按全量用量排序） */
  modelOrder: string[];
  dark: boolean;
}>();
const emit = defineEmits<{ "update:stackBy": [StackBy] }>();

const themeVars = useThemeVars();
const theme = computed(() => chartTheme(props.dark));
const days = computed(() => eachDay(props.since, props.until));

interface Series {
  name: string;
  color: string;
  data: number[];
}

/** 按日期把某个取值函数累加成数组 */
function perDay(pick: (r: DailyModelUsage) => number, keep: (r: DailyModelUsage) => boolean = () => true): number[] {
  const idx = new Map(days.value.map((d, i) => [d, i]));
  const out = new Array(days.value.length).fill(0);
  for (const r of props.rows) {
    const i = idx.get(r.date);
    if (i !== undefined && keep(r)) out[i] += pick(r);
  }
  return out;
}

const tokenSeries = computed<Series[]>(() => {
  const c = theme.value.categorical;
  if (props.stackBy === "type") {
    return [
      { name: "Input", color: c[0], data: perDay((r) => r.inputTokens) },
      { name: "Output", color: c[1], data: perDay((r) => r.outputTokens) },
      { name: "Cache 写", color: c[2], data: perDay((r) => r.cacheWrite5mTokens + r.cacheWrite1hTokens) },
      { name: "Cache 读", color: c[3], data: perDay((r) => r.cacheReadTokens) },
    ];
  }
  // 按模型：颜色按全量用量顺序固定，前 MAX_CATEGORIES 个独立着色，其余归入「其他」
  const present = new Set(props.rows.map((r) => r.model));
  const colored = props.modelOrder.slice(0, MAX_CATEGORIES);
  const series: Series[] = colored
    .map((m, i) => ({ m, i }))
    .filter(({ m }) => present.has(m))
    .map(({ m, i }) => ({ name: m, color: c[i], data: perDay((r) => r.totalTokens, (r) => r.model === m) }));
  const rest = [...present].filter((m) => !colored.includes(m));
  if (rest.length) {
    series.push({
      name: "其他",
      color: theme.value.other,
      data: perDay((r) => r.totalTokens, (r) => rest.includes(r.model)),
    });
  }
  return series;
});

const costData = computed(() => perDay((r) => r.costUsd));

function baseOption(): EChartsOption {
  const t = theme.value;
  return {
    backgroundColor: "transparent",
    animationDuration: 300,
    animationDurationUpdate: 200,
    textStyle: { fontFamily: 'system-ui, -apple-system, "Segoe UI", "Microsoft YaHei UI", sans-serif' },
    grid: { left: 56, right: 16, top: 36, bottom: 28 },
    xAxis: {
      type: "category",
      data: days.value.map(shortDate),
      axisLine: { lineStyle: { color: t.axis } },
      axisTick: { show: false },
      axisLabel: { color: t.muted, fontSize: 11 },
    },
    tooltip: {
      trigger: "axis",
      axisPointer: { type: "shadow", shadowStyle: { color: props.dark ?"rgba(255,255,255,0.05)" : "rgba(0,0,0,0.04)" } },
      backgroundColor: t.tooltipBg,
      borderColor: t.border,
      textStyle: { color: t.textPrimary, fontSize: 12 },
    },
  } as EChartsOption;
}

/** 堆叠段之间留 2px 表面色间隙；最上一段顶部圆角 4px */
function barSeries(list: Series[], surface: string) {
  return list.map((s, i) => ({
    type: "bar" as const,
    name: s.name,
    stack: "total",
    data: s.data,
    barMaxWidth: 28,
    emphasis: { focus: "series" as const },
    itemStyle: {
      color: s.color,
      borderColor: surface,
      borderWidth: 1,
      borderRadius: i === list.length - 1 ? [4, 4, 0, 0] : 0,
    },
  }));
}

const tokenOption = computed<EChartsOption>(() => {
  const t = theme.value;
  const list = tokenSeries.value;
  return {
    ...baseOption(),
    legend: {
      top: 0,
      left: 0,
      icon: "roundRect",
      itemWidth: 10,
      itemHeight: 10,
      textStyle: { color: t.textSecondary, fontSize: 12 },
    },
    yAxis: {
      type: "value",
      axisLabel: { color: t.muted, fontSize: 11, formatter: (v: number) => fmtTokensAxis(v) },
      splitLine: { lineStyle: { color: t.grid } },
    },
    tooltip: {
      ...(baseOption().tooltip as object),
      formatter: (params: unknown) => {
        const ps = params as { axisIndex: number; dataIndex: number; seriesName: string; value: number; color: string }[];
        if (!ps.length) return "";
        const day = days.value[ps[0].dataIndex];
        const total = ps.reduce((a, p) => a + (p.value || 0), 0);
        const lines = ps
          .filter((p) => p.value > 0)
          .reverse()
          .map(
            (p) =>
              `<div style="display:flex;justify-content:space-between;gap:16px"><span><span style="display:inline-block;width:8px;height:8px;border-radius:2px;background:${p.color};margin-right:6px"></span>${p.seriesName}</span><span title="${exactNumber(p.value)}">${fmtTokens(p.value)}</span></div>`,
          );
        return `<div style="font-weight:600;margin-bottom:4px">${day}</div>${lines.join("")}<div style="display:flex;justify-content:space-between;gap:16px;margin-top:4px;color:${t.textSecondary}"><span>合计</span><span>${fmtTokens(total)}</span></div>`;
      },
    },
    series: barSeries(list, themeVars.value.cardColor),
  } as EChartsOption;
});

const costOption = computed<EChartsOption>(() => {
  const t = theme.value;
  return {
    ...baseOption(),
    grid: { left: 56, right: 16, top: 12, bottom: 28 },
    yAxis: {
      type: "value",
      axisLabel: { color: t.muted, fontSize: 11, formatter: (v: number) => `$${v}` },
      splitLine: { lineStyle: { color: t.grid } },
    },
    tooltip: {
      ...(baseOption().tooltip as object),
      formatter: (params: unknown) => {
        const p = (params as { dataIndex: number; value: number }[])[0];
        return `<div style="font-weight:600;margin-bottom:4px">${days.value[p.dataIndex]}</div>估算费用 ${fmtUsd(p.value)}`;
      },
    },
    series: [
      {
        type: "bar",
        name: "估算费用",
        data: costData.value,
        barMaxWidth: 28,
        itemStyle: { color: t.categorical[0], borderRadius: [4, 4, 0, 0] },
      },
    ],
  } as EChartsOption;
});
</script>

<template>
  <n-card size="small">
    <template #header>
      <span class="title">每日 tokens</span>
    </template>
    <template #header-extra>
      <n-radio-group :value="stackBy" size="small" @update:value="(v: StackBy) => emit('update:stackBy', v)">
        <n-radio-button value="type">按 token 类型</n-radio-button>
        <n-radio-button value="model">按模型</n-radio-button>
      </n-radio-group>
    </template>
    <!-- 切换堆叠方式时 series 集合整体变化，必须整体替换，避免旧 series 状态残留 -->
    <v-chart :option="tokenOption" :update-options="{ notMerge: true }" autoresize class="chart-main" />
    <div class="hint">点击图例可隐藏某一类，便于查看量级较小的部分</div>
    <n-space justify="space-between" align="center" class="sub-title">
      <span class="title">每日估算费用（USD）</span>
    </n-space>
    <v-chart :option="costOption" autoresize class="chart-cost" />
  </n-card>
</template>

<style scoped>
.title {
  font-size: 14px;
  font-weight: 600;
}
.sub-title {
  margin-top: 8px;
}
.hint {
  font-size: 12px;
  opacity: 0.6;
}
.chart-main {
  height: 300px;
}
.chart-cost {
  height: 150px;
}
</style>
