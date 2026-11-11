<script setup lang="ts">
import { computed } from "vue";
import { NSpace, NRadioGroup, NRadioButton, NDatePicker, NSelect, NButton, NText } from "naive-ui";
import { useUsageStore, type RangePreset } from "../store";
import { parseYmd, ymd } from "../../../shared/date";

const store = useUsageStore();

const presets: { label: string; value: RangePreset }[] = [
  { label: "今天", value: "today" },
  { label: "7 天", value: "7d" },
  { label: "30 天", value: "30d" },
  { label: "本月", value: "month" },
];

const pickerValue = computed<[number, number]>(() => [
  parseYmd(store.range[0]).getTime(),
  parseYmd(store.range[1]).getTime(),
]);

function onPick(v: [number, number] | null) {
  if (v) store.setRange([ymd(new Date(v[0])), ymd(new Date(v[1]))]);
}

const projectOptions = computed(() => store.allProjects.map((p) => ({ label: p, value: p })));
const modelOptions = computed(() => store.allModels.map((m) => ({ label: m, value: m })));

const lastRefresh = computed(() => {
  const t = store.status?.lastScan?.finishedAt;
  return t ? new Date(t).toLocaleString() : "-";
});
</script>

<template>
  <n-space align="center" :size="[12, 8]" class="bar">
    <n-radio-group :value="store.preset" size="small" @update:value="store.setPreset">
      <n-radio-button v-for="p in presets" :key="p.value" :value="p.value">{{ p.label }}</n-radio-button>
    </n-radio-group>
    <n-date-picker
      type="daterange"
      size="small"
      :value="pickerValue"
      :is-date-disabled="(ts: number) => ts > Date.now()"
      style="width: 240px"
      @update:value="onPick"
    />
    <n-select
      :value="store.projects"
      :options="projectOptions"
      multiple
      filterable
      clearable
      size="small"
      max-tag-count="responsive"
      placeholder="全部项目"
      style="width: 240px"
      @update:value="store.setProjects"
    />
    <n-select
      :value="store.models"
      :options="modelOptions"
      multiple
      filterable
      clearable
      size="small"
      max-tag-count="responsive"
      placeholder="全部模型"
      style="width: 200px"
      @update:value="store.setModels"
    />
    <n-button size="small" :loading="store.refreshing || store.status?.scanning" @click="store.refresh">刷新</n-button>
    <n-text depth="3" style="font-size: 12px">最后刷新：{{ lastRefresh }}</n-text>
  </n-space>
</template>
