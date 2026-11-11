<script setup lang="ts">
import { computed } from "vue";
import { NGrid, NGi, NCard, NText } from "naive-ui";
import type { Summary } from "../api";
import { exactNumber, exactUsd, fmtTokens, fmtUsd, pctChange } from "../../../shared/format";

const props = defineProps<{ summary: Summary | null }>();

interface Card {
  label: string;
  value: string;
  exact: string;
  delta: number | null;
  compareLabel: string;
}

const cards = computed<Card[]>(() => {
  const s = props.summary;
  if (!s) return [];
  return [
    {
      label: "今日 tokens",
      value: fmtTokens(s.today.tokens),
      exact: exactNumber(s.today.tokens),
      delta: pctChange(s.today.tokens, s.yesterday.tokens),
      compareLabel: "较昨日",
    },
    {
      label: "今日费用",
      value: fmtUsd(s.today.costUsd),
      exact: exactUsd(s.today.costUsd),
      delta: pctChange(s.today.costUsd, s.yesterday.costUsd),
      compareLabel: "较昨日",
    },
    {
      label: "近 7 天费用",
      value: fmtUsd(s.last7.costUsd),
      exact: exactUsd(s.last7.costUsd),
      delta: pctChange(s.last7.costUsd, s.prev7.costUsd),
      compareLabel: "较前 7 天",
    },
    {
      label: "近 30 天费用",
      value: fmtUsd(s.last30.costUsd),
      exact: exactUsd(s.last30.costUsd),
      delta: pctChange(s.last30.costUsd, s.prev30.costUsd),
      compareLabel: "较前 30 天",
    },
  ];
});

const fmtDelta = (d: number | null) => (d === null ? "无对比数据" : `${d >= 0 ? "↑" : "↓"} ${Math.abs(d).toFixed(1)}%`);
</script>

<template>
  <n-grid cols="1 560:2 860:4" :x-gap="12" :y-gap="12" responsive="self">
    <n-gi v-for="c in cards" :key="c.label">
      <n-card size="small" :bordered="true">
        <div class="label">{{ c.label }}</div>
        <div class="value" :title="c.exact">{{ c.value }}</div>
        <n-text depth="3" class="delta">{{ c.compareLabel }} {{ fmtDelta(c.delta) }}</n-text>
      </n-card>
    </n-gi>
  </n-grid>
  <n-text v-if="summary?.unpricedModels.length" depth="3" class="note">
    * 以下模型不在价格表中，费用未计入：{{ summary.unpricedModels.join("、") }}
  </n-text>
</template>

<style scoped>
.label {
  font-size: 13px;
  opacity: 0.75;
}
.value {
  font-size: 26px;
  font-weight: 600;
  margin: 4px 0 2px;
}
.delta {
  font-size: 12px;
}
.note {
  display: block;
  font-size: 12px;
  margin-top: 6px;
}
</style>
